use std::{error::Error, fmt::Display, io::Read, net::TcpStream};

#[derive(Debug)]
pub struct WebSocket {
    socket: TcpStream,
    awaiting_pong: bool,
}

impl WebSocket {
    pub fn new(socket: TcpStream) -> WebSocket {
        WebSocket {
            socket,
            awaiting_pong: false,
        }
    }

    pub fn read_dataframe(&mut self) -> Result<DataFrame, WebSocketError> {
        let mut header_bytes: [u8; 2] = [0; 2];
        self.socket.read_exact(&mut header_bytes)?;

        let byte = header_bytes[0];
        let (fin, rsv1, rsv2, rsv3, opcode) = (
            bit(byte, 7),
            bit(byte, 6),
            bit(byte, 5),
            bit(byte, 4),
            OpCode::try_from(byte & 0x0F as u8)?,
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
                ((length_bytes[0] as u64) << 8) + length_bytes[1] as u64
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

        let mut mask_key: [u8; 4] = [0; 4];
        self.socket.read_exact(&mut mask_key)?;

        // TODO IMPORTANT
        // right now, we assume that usize is of size u64. This isn't necessarily true on 32 bit
        // systems. For now I'm creating a buffer with capacity that is a u64, but this would panic
        // and fail on a 32 bit system. To correct this, i probably need to write a vector that
        // can take larger sizes / not usize
        // TODO check if there's a way to initialize a vector with all zeroes that isn't this
        // macro? I'm assuming the macro is fine for now
        let mut payload = vec![0u8; payload_length.try_into().unwrap()];
        self.socket.read_exact(&mut payload)?; // Check that this actually reads the amount
        let payload = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask_key[index % 4])
            .collect();

        Ok(DataFrame {
            fin,
            rsv1,
            rsv2,
            rsv3,
            opcode,
            mask,
            mask_key,
            payload_length,
            payload,
        })
    }
}

/// Gets the bit at position `position`. Positions are assumed to be big endian, so the 7th
/// position is the most significant bit
fn bit(byte: u8, position: u8) -> bool {
    ((byte >> position) & 1) != 0
}

#[derive(Debug)]
pub struct DataFrame {
    fin: bool,
    // rsv1-3 can be ignored, they are for extensions TODO research what extensions are
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    opcode: OpCode,
    mask: bool,
    payload_length: u64,
    mask_key: [u8; 4],
    payload: Vec<u8>, // store the payload decoded.
}

impl DataFrame {
    pub fn get_message(&self) -> &Vec<u8> {
        &self.payload
    }
}

/// OpCode enum for the possible 4-bit opcodes
/// Values outside the range of 4 bits are invalid
#[derive(Debug)]
#[repr(u8)]
enum OpCode {
    Continuation = 0x0,
    Text = 0x1, // Encoded in utf-8
    Binary = 0x2,
    Ping = 0x9,
    Pong = 0xA,
}

impl TryFrom<u8> for OpCode {
    type Error = WebSocketError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(OpCode::Continuation),
            0x1 => Ok(OpCode::Text),
            0x2 => Ok(OpCode::Binary),
            0x3..=0x7 => Err(WebSocketError::BadOpCode(value)),
            0x9 => Ok(OpCode::Ping),
            0xA => Ok(OpCode::Pong),
            0x10..=0xFF => Err(WebSocketError::BadOpCode(value)), // Op codes are only 4 bits
            opcode => Err(WebSocketError::OpCodeNotImplemented(opcode)),
        }
    }
}

#[derive(Debug)]
pub enum WebSocketError {
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
