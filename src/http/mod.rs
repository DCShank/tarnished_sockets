use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::io::{prelude::*, BufRead, BufReader};
use std::iter::Map;
use std::net::{Incoming, TcpListener, TcpStream, ToSocketAddrs};
use std::str::FromStr;

use crate::base64;
use crate::sha1;
use crate::websocket::WebSocket;

fn validate_handshake(
    handshake_request: HttpRequest,
) -> Result<WebSocketHandshakeRequest, ServerError> {
    if let HttpMethod::GET = handshake_request.method {
    } else {
        return Err(ServerError::HandshakeValidation);
    }

    // TODO validate uri

    // TODO correct to actually check HTTP version is greater than or equal to 1.1
    if handshake_request.http_version != "HTTP/1.1" {
        return Err(ServerError::HandshakeValidation);
    }

    // TODO validate host

    match handshake_request.headers.get("Connection") {
        Some(string) if string.contains("Upgrade") => {}
        _ => return Err(ServerError::HandshakeValidation),
    }

    match handshake_request.headers.get("Upgrade").map(String::as_str) {
        Some("websocket") => {}
        _ => return Err(ServerError::HandshakeValidation),
    }

    match handshake_request
        .headers
        .get("Sec-WebSocket-Version")
        .map(String::as_str)
    {
        Some(_version) => {}
        _ => return Err(ServerError::HandshakeValidation),
    }

    match handshake_request.headers.get("Sec-WebSocket-Key") {
        Some(key) => Ok(WebSocketHandshakeRequest {
            sec_websocket_key: key.to_string(),
        }),
        _ => Err(ServerError::HandshakeValidation),
    }
}

#[derive(Debug)]
pub enum HttpMethod {
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

// Is there a way to automate this process? maybe there's a macro..
impl Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::DELETE => write!(f, "DELETE"),
            HttpMethod::CONNECT => write!(f, "CONNECT"),
            HttpMethod::OPTIONS => write!(f, "OPTIONS"),
            HttpMethod::TRACE => write!(f, "TRACE"),
            HttpMethod::PATCH => write!(f, "PATCH"),
            HttpMethod::HEAD => write!(f, "HEAD"),
        }
    }
}

#[derive(Debug)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub uri: String, // TODO find a builtin uri type!
    pub http_version: String,
    pub headers: HashMap<String, String>,
}

impl HttpRequest {
    pub fn build(stream: &TcpStream) -> Result<HttpRequest, ServerError> {
        let mut lines = BufReader::new(stream).lines();
        let line = lines.next().ok_or(ServerError::HttpRequestParse)??;
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

            // TODO this should do more verification of these additional headers, but for now just
            // throwing them in a hashmap is ok
            let (key, value) = line.split_once(": ").ok_or(ServerError::HttpRequestParse)?;
            request
                .headers
                .insert(String::from(key), String::from(value));
        }

        Ok(request)
    }
}

impl Display for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}{}",
            self.method,
            self.uri,
            self.http_version,
            self.headers
                .iter()
                .map(|(key, value)| {
                    let mut joined = String::from(key);
                    joined.push_str(": ");
                    joined.push_str(value);
                    joined
                })
                .fold(String::from(""), |mut init, line| {
                    init.push_str("\n");
                    init.push_str(&line);
                    init
                })
        )
    }
}

struct WebSocketHandshakeRequest {
    sec_websocket_key: String,
}

pub fn build_http_response(code: u16, desc: &str, headers: HashMap<String, String>) -> String {
    let status_line = format!("HTTP/1.1 {code} {desc}");
    let mut response = headers
        .iter()
        .map(|(key, value)| {
            let mut joined = String::from(key);
            joined.push_str(": ");
            joined.push_str(value);
            joined
        })
        .fold(String::from(status_line), |mut init, line| {
            init.push_str("\r\n");
            init.push_str(&line);
            init
        });
    response.push_str("\r\n\r\n");
    response
}

static MAGIC_KEY_STRING: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
pub fn calculate_websocket_key(client_key: &str) -> String {
    // concat client key with magic key
    let to_hash = format!("{client_key}{MAGIC_KEY_STRING}");
    let hash = sha1::hash(&to_hash);
    let encoded = base64::encode(hash);
    encoded
}

pub struct WebSocketListener(TcpListener);

impl WebSocketListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Result<WebSocketListener, ServerError> {
        Ok(WebSocketListener(TcpListener::bind(addr)?))
    }

    pub fn incoming(
        &self,
    ) -> Map<
        Incoming,
        fn(Result<TcpStream, std::io::Error>) -> Result<WebSocket, Box<dyn Error + Send + Sync>>,
    > {
        self.0
            .incoming()
            .map(|stream_result| WebSocketListener::handle_client(stream_result?))
    }

    fn handle_client(mut stream: TcpStream) -> Result<WebSocket, Box<dyn Error + Send + Sync>> {
        let request = HttpRequest::build(&stream)?;

        // TODO remove
        println!("{}", request);

        let websocket_key = validate_handshake(request)?.sec_websocket_key;
        let mut headers: HashMap<String, String> = HashMap::new();
        headers.insert("Upgrade".to_string(), "websocket".to_string());
        headers.insert("Connection".to_string(), "Upgrade".to_string());
        headers.insert("Sec-WebSocket-Accept".to_string(), websocket_key);
        let response = build_http_response(101, "Switching Protocols", headers);

        // TODO the unwrap here isn't great as the client could have closed the socket or something
        stream.write_all(response.as_bytes()).unwrap();
        // TODO handle the stream closing here due to the client not accepting the response

        let websocket = WebSocket::new(stream);
        Ok(websocket)
    }
}

impl From<TcpListener> for WebSocketListener {
    fn from(listener: TcpListener) -> Self {
        WebSocketListener(listener)
    }
}

#[derive(Debug)]
pub enum ServerError {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_websocket_key_works() {
        let calculated = calculate_websocket_key("dGhlIHNhbXBsZSBub25jZQ==");
        let expected = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

        assert_eq!(calculated, expected);
    }
}
