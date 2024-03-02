use std::net::SocketAddr;

pub mod http;
pub mod websocket;
pub mod event_queue;

/// This function is meant to parse the address from command line arguments.
/// For now it just returns local address
pub fn get_socket_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7878))
}
