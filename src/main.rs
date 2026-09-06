use std::{convert::Infallible, sync::Arc};

use hyper::{Request, Response, StatusCode, server::conn::http1, service};
use hyper_util::rt::TokioIo;
use tokio::{fs, net::TcpListener, sync::Semaphore};

/// The server address, format!(http://{ADDRESS}) is the link to server
const ADDRESS: &str = "localhost:9000";

/// Limit how many request can be handling in the same time
const RATE_LIMIT: usize = 1000;

/// match with "/"
const MAIN_HTML: &str = "src/front-end/main.html";

async fn handle(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<http_body_util::Full<hyper::body::Bytes>>, Infallible> {
    use http_body_util::Full;
    use hyper::body::Bytes;

    Ok(match req.uri().path() {
        "/" => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(Full::new(Bytes::from(
                fs::read_to_string(MAIN_HTML).await.unwrap(),
            )))
            .unwrap(),

        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/html")
            .body(Full::new(Bytes::from(
                "<html><body><h1>404 Not Found</h1></body></html>",
            )))
            .unwrap(),
    })
}

#[tokio::main]
async fn main() {
    let listener = match TcpListener::bind(ADDRESS).await {
        Ok(v) => v,
        Err(e) => panic!("FATAL ERROR: From `TcpListener::bind`: {e:?}"),
    };

    println!("Server running at http://{ADDRESS}");

    let semaphore = Arc::new(Semaphore::new(RATE_LIMIT));

    loop {
        let (socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "ERROR: From `listener.accept()`: {e:?}\n
                    \rINFO: Ignoring error and continue..."
                );
                continue;
            }
        };

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore still open");

        println!("New Client: {addr:?}");

        tokio::spawn(async move {
            let io = TokioIo::new(socket);
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service::service_fn(handle))
                .await
            {
                eprintln!(
                    "ERROR: From `serve_connection`: {e:?}\n
                    \rINFO: Ignoring error and continue..."
                );
            }
            drop(permit);
        });
    }
}
