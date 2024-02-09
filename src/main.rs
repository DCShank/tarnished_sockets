use std::{error::Error, thread};

use tarnished_sockets::websocket::OpCode;
use tarnished_sockets::{get_socket_addr, http::WebSocketListener};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = get_socket_addr();
    let listener = WebSocketListener::bind(addr)?;

    let mut threads = Vec::new();
    for websocket in listener.incoming() {
        let mut websocket = websocket?;
        websocket.on_receive = Box::new(|ws, df| match df.opcode {
            OpCode::Text => ws.write_text(std::str::from_utf8(&df.payload).unwrap()),
            OpCode::Binary => ws.write_binary(&df.payload),
            _ => Ok(()),
        });
        let thread = thread::spawn(move || {
            websocket.open();
        });
        threads.push(thread);
    }

    Ok(())
}
