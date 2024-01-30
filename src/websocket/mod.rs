use std::{error::Error, fmt::Display, net::TcpStream};

#[derive(Debug)]
struct WebSocket {
    socket: TcpStream,
    awaiting_pong: bool,
}

struct DataFrame {
    fin: bool,
    // rsv1-3 can be ignored, they are for extensions TODO research what extensions are
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    opcode: OpCode,
    mask_key: u32,
    payload_length: u128,
    payload: Vec<u8>,
}

/// OpCode enum for the possible 4-bit opcodes
/// Values outside the range of 4 bits are invalid
#[repr(u8)]
enum OpCode {
    Continuation = 0x0,
    Text = 0x1, // Encoded in utf-8
    Binary = 0x2,
}

impl TryFrom<u32> for OpCode {
    type Error = WebSocketError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(OpCode::Continuation),
            0x1 => Ok(OpCode::Text),
            0x2 => Ok(OpCode::Binary),
            0x3..=0x7 => Err(WebSocketError::BadOpCode(value)),
            0x10..=0xFF => Err(WebSocketError::BadOpCode(value)),   // Op codes are only 4 bits
            opcode => Err(WebSocketError::OpCodeNotImplemented(opcode)),
        }
    }
}

#[derive(Debug)]
enum WebSocketError {
    BadOpCode(u32),
    OpCodeNotImplemented(u32),
}

impl Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketError::BadOpCode(opcode) => {
                write!(f, "Bad in opcode {} in dataframe", opcode)
            }
            WebSocketError::OpCodeNotImplemented(opcode) => {
                write!(
                    f,
                    "Received opcode {} which hasn't yet been implemented",
                    opcode
                )
            }
        }
    }
}

impl Error for WebSocketError {}
