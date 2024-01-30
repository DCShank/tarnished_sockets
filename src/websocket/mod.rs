use std::{error::Error, fmt::Display, io::Read, net::TcpStream};

#[derive(Debug)]
pub struct WebSocket {
    socket: TcpStream,
    awaiting_pong: bool,
}

impl WebSocket {
    pub fn read_dataframe(&mut self) -> Result<DataFrame, WebSocketError> {
        let mut header_bytes: [u8; 2] = [0; 2];
        self.socket.read_exact(&mut header_bytes)?;

        let byte = header_bytes[0];
        let (fin, rsv1, rsv2, rsv3, opcode) = (
            bit(byte, 7),
            bit(byte, 6),
            bit(byte, 5),
            bit(byte, 4),
            byte & 0x0F as u8,
        );

        // handle message length parsing
        let (mask, payload_length) = (bit(header_bytes[1], 7) as bool, header_bytes[1] & 0x7F);
        if !mask {
            return Err(WebSocketError::UnencodedMessage);
        }

        let payload_length = match payload_length {
            0..=125 => payload_length as u64,
            126 => {
                let mut length_bytes: [u8; 2] = [0; 2];
                self.socket.read_exact(&mut length_bytes)?;
                (length_bytes[0] as u64) << 8 + length_bytes[1] as u64
            }
            127 => {
                let mut length_bytes: [u8; 8] = [0; 8];
                self.socket.read_exact(&mut length_bytes)?;
                // The most significant bit cannot be 1
                if bit(length_bytes[0], 7) {
                    return Err(WebSocketError::BadPayloadLength);
                }
                u64::from_be_bytes(length_bytes)
            }
            _ => panic!("Found a payload length value that is impossible"),
        };

        //TODO get the payload

        Ok(DataFrame {
            fin,
            rsv1,
            rsv2,
            rsv3,
            opcode: OpCode::try_from(opcode)?,
            mask,
            mask_key: 0,
            payload_length,
            payload: Vec::new(),
        })
    }
}

/// Gets the bit at position `position`. Positions are assumed to be big endian, so the 7th
/// position is the most significant bit
fn bit(byte: u8, position: u8) -> bool {
    ((byte >> position) & 1) != 0
}

struct DataFrame {
    fin: bool,
    // rsv1-3 can be ignored, they are for extensions TODO research what extensions are
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    opcode: OpCode,
    mask: bool,
    payload_length: u64,
    mask_key: u32,
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

impl TryFrom<u8> for OpCode {
    type Error = WebSocketError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(OpCode::Continuation),
            0x1 => Ok(OpCode::Text),
            0x2 => Ok(OpCode::Binary),
            0x3..=0x7 => Err(WebSocketError::BadOpCode(value)),
            0x10..=0xFF => Err(WebSocketError::BadOpCode(value)), // Op codes are only 4 bits
            opcode => Err(WebSocketError::OpCodeNotImplemented(opcode)),
        }
    }
}

#[derive(Debug)]
enum WebSocketError {
    BadOpCode(u8),
    OpCodeNotImplemented(u8),
    Io(std::io::Error),
    UnencodedMessage,
    BadPayloadLength,
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
            WebSocketError::Io(error) => error.fmt(f),
            WebSocketError::UnencodedMessage => write!(f, "Mask bit set to 0"),
            WebSocketError::BadPayloadLength => write!(f, "Payload length was > 2^63-1"),
        }
    }
}

impl Error for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(value: std::io::Error) -> Self {
        WebSocketError::Io(value)
    }
}
