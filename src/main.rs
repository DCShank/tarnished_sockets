use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::str::FromStr;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = get_socket_addr();
    let listener = TcpListener::bind(addr)?;

    for stream in listener.incoming() {
        let _ = handle_client(stream?);
    }

    Ok(())
}

fn handle_client(stream: TcpStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let request = HttpRequest::build(stream)?;

    println!("{:?}", request);

    Ok(())
}

/// This function is meant to parse the address from command line arguments.
/// For now it just returns local address
fn get_socket_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7878))
}

#[derive(Debug)]
enum ServerError {
    HttpRequestParse,
    HandshakeValidation,
    InvalidHttpMethod,
    IO(std::io::Error),
}

impl From<std::io::Error> for ServerError {
    fn from(error: std::io::Error) -> ServerError {
        ServerError::IO(error)
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::HttpRequestParse => {
                write!(f, "Error while parsing the HTTP request")
            }
            ServerError::HandshakeValidation => {
                write!(f, "Error validating the websocket handshake")
            }
            ServerError::InvalidHttpMethod => {
                write!(f, "Invalid HTTP method in request")
            }
            ServerError::IO(err) => err.fmt(f),
        }
    }
}

impl Error for ServerError {}

#[derive(Debug)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
    HEAD,
}

impl FromStr for HttpMethod {
    type Err = ServerError;

    fn from_str(input: &str) -> Result<HttpMethod, Self::Err> {
        match input {
            "GET" => Ok(HttpMethod::GET),
            "POST" => Ok(HttpMethod::POST),
            "PUT" => Ok(HttpMethod::PUT),
            "DELETE" => Ok(HttpMethod::DELETE),
            "CONNECT" => Ok(HttpMethod::CONNECT),
            "OPTIONS" => Ok(HttpMethod::OPTIONS),
            "TRACE" => Ok(HttpMethod::TRACE),
            "PATCH" => Ok(HttpMethod::PATCH),
            "HEAD" => Ok(HttpMethod::HEAD),
            _ => Err(ServerError::InvalidHttpMethod),
        }
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: HttpMethod,
    uri: String, // TODO find a builtin uri type!
    http_version: String,
    headers: HashMap<String, String>,
}

impl HttpRequest {
    fn build(stream: TcpStream) -> Result<HttpRequest, ServerError> {
        let mut lines = BufReader::new(stream).lines();
        let line = lines
            .next()
            .ok_or(ServerError::HttpRequestParse)??;
        let mut split_line = line.split(' ');
        let method = split_line.next().ok_or(ServerError::HttpRequestParse)?;
        let method = HttpMethod::from_str(method)?;

        let uri = split_line.next().ok_or(ServerError::HttpRequestParse)?;
        let uri = String::from(uri);
        // TODO uri validitiy checking

        let http_version = split_line.next().ok_or(ServerError::HttpRequestParse)?;
        let http_version = String::from(http_version);
        // TODO http_version checking, also a different type for http version

        let mut request = HttpRequest {
            method,
            uri,
            http_version,
            headers: HashMap::new(),
        };

        while let Some(Ok(line)) = lines.next() {
            if line.is_empty() {
                break;
            }

            let (key, value) = line.split_once(':').ok_or(ServerError::HttpRequestParse)?;
            request
                .headers
                .insert(String::from(key), String::from(value));
        }

        Ok(request)
    }
}
