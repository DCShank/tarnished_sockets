use crate::websocket::OpCode;
use crate::websocket::WebSocket;
use std::io::prelude::*;
use std::thread;
use std::{
    collections::HashMap,
    error::Error,
    net::{SocketAddr, TcpStream},
    thread::JoinHandle,
};

use crate::http::{build_http_response, calculate_websocket_key, validate_handshake, HttpRequest};

mod base64;
mod http;
mod sha1;
mod websocket;

pub fn handle_client(mut stream: TcpStream) -> Result<JoinHandle<()>, Box<dyn Error + Send + Sync>> {
    let request = HttpRequest::build(&stream)?;

    println!("{}", request);

    let _validated = validate_handshake(&request)?;

    // we can safely unwrap here because we've validated the key in validate_handshake.
    // TODO consider a more appropriate way to handle this checking to take advantage of the type
    // system
    let websocket_key = calculate_websocket_key(request.headers.get("Sec-WebSocket-Key").unwrap());
    let mut headers: HashMap<String, String> = HashMap::new();
    headers.insert("Upgrade".to_string(), "websocket".to_string());
    headers.insert("Connection".to_string(), "Upgrade".to_string());
    headers.insert("Sec-WebSocket-Accept".to_string(), websocket_key);
    let response = build_http_response(101, "Switching Protocols", headers);

    stream.write_all(response.as_bytes()).unwrap();

    let mut websocket = WebSocket::new(stream);
    websocket.on_receive = Box::new(|ws, df| match df.opcode {
        OpCode::Text => ws.write_text(std::str::from_utf8(&df.payload).unwrap()),
        OpCode::Binary => ws.write_binary(&df.payload),
        _ => Ok(()),
    });

    let handle = thread::spawn(move || {
        websocket.open();
    });

    Ok(handle)
}

/// This function is meant to parse the address from command line arguments.
/// For now it just returns local address
pub fn get_socket_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7878))
}
