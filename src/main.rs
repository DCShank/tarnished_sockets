use std::error::Error;
use std::net::TcpListener;

use tarnished_sockets::{get_socket_addr, handle_client};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = get_socket_addr();
    let listener = TcpListener::bind(addr)?;

    let mut threads = Vec::new();
    // TODO handle sending 400 responses for invalid requests!
    for stream in listener.incoming() {
        let thread = handle_client(stream?)?;
        threads.push(thread);
    }

    Ok(())
}
