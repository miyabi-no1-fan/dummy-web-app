use axum::{
    Router,
    error_handling::HandleErrorLayer,
    extract::DefaultBodyLimit,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing,
};
use std::{
    io::Write,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod transform;

/// The server address, `format!(http://{ADDRESS})` is the link to server
const ADDRESS: &str = "localhost:9000";
const DIST_DIR: &str = "client/dist";
const STYLE_CSS: &str = "client/style.css";
async fn index_html() -> impl IntoResponse {
    Html(std::include_str!("../client/index.html"))
}

const MAX_BODY_BYTES: usize = 50 << 20;
const CONCURRENCY_LIMIT: usize = 12;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// All image total size in bytes must not exceed this value.
///
/// This does not limit how many image you can have at a time.
pub const IMAGE_LEN_LIMIT: usize = 1 << 30;

#[tokio::main]
async fn main() {
    // some magic for logging idk
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/", routing::get(index_html))
        .route_service("/style.css", ServeFile::new(STYLE_CSS))
        .route("/submit", routing::post(submit))
        .nest_service("/dist", ServeDir::new(DIST_DIR))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|_| async {
                    StatusCode::SERVICE_UNAVAILABLE
                }))
                .load_shed()
                .concurrency_limit(CONCURRENCY_LIMIT)
                .timeout(REQUEST_TIMEOUT),
        );

    let listener = match TcpListener::bind(ADDRESS).await {
        Ok(v) => v,
        Err(e) => panic!("FATAL ERROR: From `TcpListener::bind`: {e}"),
    };

    println!("Server running at http://{ADDRESS}");

    axum::serve(listener, app)
        .await
        .expect("Should never happen");
}

async fn submit(body: axum::body::Bytes) -> impl IntoResponse {
    let handle_request = move || -> Result<Vec<u8>, StatusCode> {
        let mut buf: &[u8] = &body;

        let count = buf[0];
        buf = buf.get(1..).ok_or(StatusCode::BAD_REQUEST)?;

        let mut mat = [[1.0, 0.0], [0.0, 1.0]];

        for _ in 0..count {
            if buf.len() < 16 {
                return Err(StatusCode::BAD_REQUEST);
            }
            mat = transform::matrix_mul(
                mat,
                [
                    [
                        f32::from_le_bytes(buf[0..4].try_into().unwrap()) as f64,
                        f32::from_le_bytes(buf[4..8].try_into().unwrap()) as f64,
                    ],
                    [
                        f32::from_le_bytes(buf[8..12].try_into().unwrap()) as f64,
                        f32::from_le_bytes(buf[12..16].try_into().unwrap()) as f64,
                    ],
                ],
            );
            buf = &buf[16..];
        }

        let mut image = {
            let reader = std::io::Cursor::new(buf);

            {
                let (width, height) = image::ImageReader::new(reader.clone())
                    .with_guessed_format()
                    .or(Err(StatusCode::BAD_REQUEST))?
                    .into_dimensions()
                    .or(Err(StatusCode::BAD_REQUEST))?;

                if width as usize * height as usize * 4 > IMAGE_LEN_LIMIT {
                    return Err(StatusCode::BAD_REQUEST);
                }
            }

            image::ImageReader::new(reader)
                .with_guessed_format()
                .or(Err(StatusCode::BAD_REQUEST))?
                .decode()
                .or(Err(StatusCode::BAD_REQUEST))?
        }
        .into_rgba8();

        image = {
            let (new_image, width, height) = transform::linear_transform::<u8>(
                &image,
                image.width(),
                image.height(),
                4, // rgba8 is 4 u8
                mat,
            )
            .ok_or(StatusCode::BAD_REQUEST)?;

            image::ImageBuffer::from_raw(width, height, new_image).ok_or_else(|| {
                tracing::error!("{}:{}, internal `linear_transform` error", file!(), line!());
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        };

        let mut result = std::io::Cursor::new(Vec::new());

        // `result` expected binary layout:
        // [width: u32 little endian][height: u32 little endian][png file]
        {
            let (width, height): (u32, u32) = (image.width(), image.height());

            result
                .write(&width.to_le_bytes())
                .and(result.write(&height.to_le_bytes()))
                .expect("write to Vec can't fail");

            image
                .write_to(&mut result, image::ImageFormat::Png)
                .or(Err(StatusCode::BAD_REQUEST))?;
        }

        Ok(result.into_inner())
    };

    let response_time = Instant::now();

    match tokio::task::spawn_blocking(handle_request)
        .await
        // if handle_request panic by any chance
        .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
    {
        Ok(result) => {
            tracing::info!("response time: {:?}ms", response_time.elapsed().as_millis());
            (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (
                        header::HeaderName::from_static("x-image-content-type"),
                        "image/png",
                    ),
                ],
                result,
            )
                .into_response()
        }
        Err(error_code) => error_code.into_response(),
    }
}
