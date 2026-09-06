use std::{fmt::Debug, sync::Arc};

use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};

const ADDRESS: &str = "http://localhost:9000";

/// Limit how many request can be handling in the same time
const RATE_LIMIT: usize = 1000;

const HTTP_DEBUG: bool = true;

const MAIN_HTML: &str = "src/front-end/main.html";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let listener = TcpListener::bind(ADDRESS).await?;
    println!("Server running at {ADDRESS}");

    let semaphore = Arc::new(Semaphore::new(RATE_LIMIT));

    loop {
        let (mut socket, addr) = match listener.accept().await {
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
            if let Err(e) = handle(&mut socket).await {
                eprintln!(
                    "ERROR: From `task::handle`: {e:?}\n
                    \rINFO: Ignoring error and continue..."
                );
            }
            drop(permit);
        });
    }
}

/// This is the main request handler function
pub async fn handle(socket: &mut TcpStream) -> Result<(), Error> {
    let read_line = async |socket: &mut TcpStream| -> Result<String, Error> {
        const LINE_LIMIT: usize = 128;
        let mut line = Vec::new();
        for _ in 0..LINE_LIMIT {
            line.resize(line.len() + 1, 0);
            let len = line.len();
            socket.read_exact(&mut line[len - 1..]).await?;
            if line.ends_with(b"\r\n") {
                break;
            }
        }
        String::from_utf8(line)
            .map_err(|_| Error::ParseError)
            .map(|r| {
                if HTTP_DEBUG {
                    print!("{r}");
                }
                r
            })
    };

    loop {
        let mut method = String::new();
        let mut path = String::new();
        let mut content_length = 0;

        loop {
            let line = read_line(socket).await?;

            match line.as_str() {
                _ if line.starts_with("GET") => {
                    let mut p = line.split_whitespace();
                    method = p.next().ok_or(Error::ParseError)?.to_string();
                    path = p.next().ok_or(Error::ParseError)?.to_string();
                }

                _ if line.starts_with("Content-Length") => {
                    content_length = line
                        .strip_prefix("Content-Length: ")
                        .ok_or(Error::ParseError)?
                        .strip_suffix("\r\n")
                        .ok_or(Error::ParseError)?
                        .parse()
                        .map_err(|_| Error::ParseError)?;
                }

                "\r\n" => break,

                _ => {}
            }
        }

        let _ = content_length; // ignore for now

        let (status, content_type, body, connection) = match (method.as_str(), path.as_str()) {
            ("GET", "/") => (
                "200 OK",
                "text/html; charset=utf-8",
                tokio::fs::read(MAIN_HTML).await.expect("File Not Found"),
                "close",
            ),

            _ => (
                "404 NOT FOUND",
                "text/plain",
                b"Not found".to_vec(),
                "close",
            ),
        };

        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n",
            body.len()
        );

        socket.write_all(response.as_bytes()).await?;
        socket.write_all(&body).await?;

        if connection == "close" {
            break;
        }
    }

    Ok(())
}

pub enum Error {
    Io(io::ErrorKind),
    ParseError,
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value.kind())
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Io(e) => write!(f, "{e:?}"),
            Self::ParseError => write!(f, "Failed to Parse input"),
        }
    }
}
