use std::{
    error::Error,
    fmt::Display,
    io::{BufReader, Read, Write},
    net::TcpStream,
};

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

    pub fn has_data(&mut self) -> Result<bool, WebSocketError> {
        let mut zero_buf: [u8; 0] = [];
        self.socket.set_nonblocking(true)?;
        let result = self.socket.peek(&mut zero_buf);
        self.socket.set_nonblocking(false)?;

        match result {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(WebSocketError::Io(e)),
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
                u16::from_be_bytes(length_bytes) as u64
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

        // TODO Benchmark a bunch of the possible ways to read in this data!!!!

        /* Solution using a pre allocated vector
        let mut payload = vec![0u8; payload_length.try_into().unwrap()];
        BufReader::new(self.socket.by_ref().take(payload_length)).read(&mut payload)?; // Check that this actually reads the amount

        // Immutable solution, creating a new payload
        let payload = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask_key[index % 4])
            .collect();

        // Inplace mutable solution. Requires two passes -- One to read in the data, one to modify
        payload
            .iter_mut()
            .enumerate()
            .for_each(|(index, byte)| {
                *byte ^= mask_key[index % 4]
            });
        */

        // TODO Verify that this works for 0-length payloads!!
        // Immutable in place "one pass" (depending on how rust cleans up the map) solution
        // This solution may be slow depending on how bytes is implemented
        let payload: Vec<u8> = BufReader::new(Read::by_ref(&mut self.socket).take(payload_length))
            .bytes()
            .enumerate()
            // Unfortunately, bytes returns a Result<u8, E>. This means we have to do this ugliness
            .map(|(index, byte)| -> Result<u8, WebSocketError> { Ok(byte? ^ mask_key[index % 4]) })
            .collect::<Result<Vec<u8>, WebSocketError>>()?;

        Ok(DataFrame {
            fin,
            rsv1,
            rsv2,
            rsv3,
            opcode,
            mask,
            mask_key: Some(mask_key),
            payload_length,
            payload,
        })
    }

    pub fn send(&mut self, message: &str) -> Result<(), WebSocketError> {
        let df = DataFrame::new(
            true,
            false,
            false,
            false,
            OpCode::Text,
            false,
            message.len() as u64,
            None,
            message.into(),
        );

        self.socket.write_all(&df.serialize()?);

        Ok(())
    }
}

/// Gets the bit at position `position`. Positions are assumed to be big endian, so the 7th
/// position is the most significant bit
fn bit(byte: u8, position: u8) -> bool {
    ((byte >> position) & 1) != 0
}

#[derive(Debug)]
pub struct DataFrame {
    pub fin: bool,
    // rsv1-3 can be ignored, they are for extensions TODO research what extensions are
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    pub opcode: OpCode,
    mask: bool,
    pub payload_length: u64,
    mask_key: Option<[u8; 4]>,
    pub payload: Vec<u8>, // store the payload decoded.
}

impl DataFrame {
    /// New method. Right now it doesn't do anything fancy
    pub fn new(
        fin: bool,
        rsv1: bool,
        rsv2: bool,
        rsv3: bool,
        opcode: OpCode,
        mask: bool,
        payload_length: u64,
        mask_key: Option<[u8; 4]>,
        payload: Vec<u8>,
    ) -> DataFrame {
        DataFrame {
            fin,
            rsv1,
            rsv2,
            rsv3,
            opcode,
            mask,
            payload_length,
            mask_key,
            payload,
        }
    }

    pub fn serialize(mut self) -> Result<Vec<u8>, WebSocketError> {
        let mut header_byte = 0u8;
        header_byte |= (self.fin as u8) << 7;
        header_byte |= (self.rsv1 as u8) << 6;
        header_byte |= (self.rsv2 as u8) << 5;
        header_byte |= (self.rsv3 as u8) << 4;
        header_byte |= self.opcode as u8;

        let mut mask_payload_byte = 0u8;
        mask_payload_byte |= (self.mask as u8) << 7;

        let payload_length = PayloadLengthKind::try_from(self.payload_length)?;

        let length_bytes = match payload_length {
            PayloadLengthKind::Bit7(length) => {
                mask_payload_byte |= length;
                None
            }
            PayloadLengthKind::Bit16(length) => {
                mask_payload_byte |= 126;
                Some(length.to_be_bytes().to_vec())
            }
            PayloadLengthKind::Bit63(length) => {
                mask_payload_byte |= 127;
                Some(length.to_be_bytes().to_vec())
            }
        };

        let mut serialized = Vec::new();
        serialized.push(header_byte);
        serialized.push(mask_payload_byte);
        if let Some(mut length_bytes) = length_bytes {
            serialized.append(&mut length_bytes);
        }
        serialized.append(&mut self.payload);

        Ok(serialized)
    }

    pub fn get_message(&self) -> &Vec<u8> {
        &self.payload
    }
}

/// OpCode enum for the possible 4-bit opcodes
/// Values outside the range of 4 bits are invalid
#[derive(Debug)]
#[repr(u8)]
pub enum OpCode {
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

/* Possible design
 *  Have a PayloadLength struct, and a PayloadLengthKind enum
 *  PayloadLengthKind knows how to convert between the key values (0..125, 126, 127)
 *  PayloadLength knows how to convert between u64 values (or possibly usize?)
 *
 *  PayloadLength is just a wrapper around PayloadLengthKind?
 *  This all would mostly be useful to enforce the rules around what values are allowed
 */

enum PayloadLengthKind {
    Bit7(u8),
    Bit16(u16),
    Bit63(u64),
}

impl TryFrom<u64> for PayloadLengthKind {
    type Error = WebSocketError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            value if value < (1 << 7) => Ok(PayloadLengthKind::Bit7(value as u8)),
            value if value < (1 << 16) => Ok(PayloadLengthKind::Bit16(value as u16)),
            value if value < (1 << 63) => Ok(PayloadLengthKind::Bit63(value)),
            _ => Err(WebSocketError::BadPayloadLength),
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
