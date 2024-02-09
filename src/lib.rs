use std::net::SocketAddr;

mod base64;
pub mod http;
mod sha1;
pub mod websocket;

/// This function is meant to parse the address from command line arguments.
/// For now it just returns local address
pub fn get_socket_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7878))
}
