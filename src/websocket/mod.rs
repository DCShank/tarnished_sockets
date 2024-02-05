use std::{
    error::Error,
    fmt::Display,
    io::{BufReader, Read, Write},
    net::TcpStream,
};

pub struct WebSocket {
    socket: WebSocketStream,
    on_close: Box<dyn Fn(CloseCode, Vec<u8>) -> Result<(), WebSocketError> + Send + Sync>,
    pub on_receive: Box<dyn Fn(&mut WebSocketStream, &DataFrame) -> Result<(), WebSocketError> + Send + Sync>,
    on_ping: Box<dyn Fn() -> Result<(), WebSocketError> + Send + Sync>,
    on_pong: Box<dyn Fn() -> Result<(), WebSocketError> + Send + Sync>,
    //TODO add a basic method queue system
    ping_delay: usize,
    missed_pongs: usize,
    //on_missed_pong: Box<dyn Fn(usize) -> Result<(), WebSocketError>>,
}

impl WebSocket {
    pub fn new(stream: TcpStream) -> WebSocket {

        WebSocket{
            socket: WebSocketStream::new(stream),
            on_close: Box::new(|_, _| {Ok(())}),
            on_receive: Box::new(|_, _| {Ok(())}),
            on_ping: Box::new(|| {Ok(())}),
            on_pong: Box::new(|| {Ok(())}),
            ping_delay: 1000,
            missed_pongs: 0,
            //on_missed_pong: Box::new(|_| {Ok(())}),
        }
    }

    //TODO rename this. Or, if it must be called open, I could refactor so this method is actually
    //more akin to "opening" the socket
    pub fn open(mut self) {
        if let Err(error) = self.run() {
            &self.handle_error(error);
            return
        }
    }

    fn run(&mut self) -> Result<(), WebSocketError> {
        loop {
            if self.socket.has_data()? {
                let dataframe = self.socket.read_dataframe()?;

                match dataframe.opcode {
                    OpCode::Continuation => todo!(),
                    OpCode::Text => (self.on_receive)(&mut self.socket, &dataframe),
                    OpCode::Binary => (self.on_receive)(&mut self.socket, &dataframe),
                    OpCode::Close => {
                        println!("{:?}", dataframe);
                        let close_code = if dataframe.payload_length >= 2 {
                            let close_code_bytes: [u8; 2] =
                                (dataframe.payload[0], dataframe.payload[1]).into();
                            CloseCode::try_from(u16::from_be_bytes(close_code_bytes))
                            .unwrap_or(CloseCode::PolicyViolated)
                        } else {
                            CloseCode::Normal   
                        };
                        (self.on_close)(close_code, vec![]);
                        self.socket.send_close(close_code, "");
                        self.socket.close();
                        return Ok(())
                    }
                    OpCode::Ping => {
                        (self.on_ping)();
                        self.socket.send_pong()
                    }

                    OpCode::Pong => {
                        (self.on_pong)();
                        self.missed_pongs = 0;
                        Ok(())
                    }
                }?;
            }
        }
    }

    pub fn close(&mut self, close_code: CloseCode) {
        if let Err(error)= self.socket.send_close(close_code, "") {
            //drop(self)
            self.socket.close();
        }
        while let Ok(df) = self.socket.read_dataframe() {
            match df.opcode {
                OpCode::Close => {
                    self.socket.close();
                    return
                }
                _ => (),
            }
        }
        self.socket.close();
        return
    }

    fn handle_error(&mut self, error: WebSocketError) {
        println!("{:?}", error);
        match error {
            WebSocketError::BadOpCode(code) => self.close(CloseCode::ProtocolError),
            WebSocketError::OpCodeNotImplemented(_) => self.close(CloseCode::PolicyViolated),
            WebSocketError::Io(_) => self.close(CloseCode::ServerError),
            WebSocketError::UnencodedMessage => self.close(CloseCode::ProtocolError),
            WebSocketError::BadPayloadLength(_) => self.close(CloseCode::MessageTooBig), 
            WebSocketError::UnusedCloseCode(_) => self.close(CloseCode::ProtocolError),
            WebSocketError::Close(code) => self.close(code),
        }
    }
}

#[derive(Debug)]
pub struct WebSocketStream {
    socket: TcpStream,
}

impl WebSocketStream {
    pub fn new(socket: TcpStream) -> WebSocketStream {
        WebSocketStream { socket }
    }

    fn close(&self) -> Result<(), WebSocketError> {
        Ok(self.socket.shutdown(std::net::Shutdown::Both)?)
    }

    // TODO this implementation must be changed to an async/await Futures implementation once we
    // have started work on the executor
    fn has_data(&mut self) -> Result<bool, WebSocketError> {
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

    fn read_dataframe(&mut self) -> Result<DataFrame, WebSocketError> {
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
                    return Err(WebSocketError::BadPayloadLength(u64::from_be_bytes(
                        length_bytes,
                    )));
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

    fn send(&mut self, payload: Vec<u8>, opcode: OpCode) -> Result<(), WebSocketError> {
        let df = DataFrame::new(
            true,
            false,
            false,
            false,
            opcode,
            false,
            payload.len() as u64,
            None,
            payload,
        );

        println!("{:?}", df);

        self.socket.write_all(&df.serialize()?)?;

        Ok(())
    }

    pub fn send_text(&mut self, message: &str) -> Result<(), WebSocketError> {
        self.send(message.into(), OpCode::Text)
    }

    pub fn send_binary(&mut self, message: &Vec<u8>) -> Result<(), WebSocketError> {
        self.send(message.to_vec(), OpCode::Binary)
    }

    fn send_pong(&mut self) -> Result<(), WebSocketError> {
        self.socket.write_all(&PONG_DATAFRAME)?;
        Ok(())
    }

    fn send_ping(&mut self) -> Result<(), WebSocketError> {
        self.socket.write_all(&PING_DATAFRAME)?;
        Ok(())
    }

    fn send_close(&mut self, close_code: CloseCode, message: &str) -> Result<(), WebSocketError> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&Into::<u16>::into(close_code).to_be_bytes());
        payload.extend_from_slice(message.as_bytes());
        self.send(payload, OpCode::Close)
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

// TODO consider redesigning the DataFrame struct
// It might be better to have DataFrame be a perfect representation of the bytes, ie.
// header_byte: u8
// mask_payload_len_byte: u8
// additional_payload_length: Vec<u8>
// (or possible payload_length_extended_u16: u16 and payload_length_extended_u63: u64)
// mask_key: u32
// payload: Vec<u8>
//
// This would allow for easier conversion to and from data.
// also for cleaner enums for control frames?
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

        let payload_length = PayloadLength::try_from(self.payload_length)?;

        let length_bytes = match payload_length {
            PayloadLength::Bit7(length) => {
                mask_payload_byte |= length;
                None
            }
            PayloadLength::Bit16(length) => {
                mask_payload_byte |= 126;
                Some(length.to_be_bytes().to_vec())
            }
            PayloadLength::Bit63(length) => {
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
        // Should never be Some(mask_key) for messages from the server
        if let Some(mask_key) = self.mask_key {
            serialized.append(&mut mask_key.to_vec());
        }
        serialized.append(&mut self.payload);

        Ok(serialized)
    }

    pub fn get_message(&self) -> &Vec<u8> {
        &self.payload
    }
}

const PING_DATAFRAME: [u8; 2] = [0x89, 0x00];
const PONG_DATAFRAME: [u8; 2] = [0x8A, 0x00];

/// OpCode enum for the possible 4-bit opcodes
/// Values outside the range of 4 bits are invalid
#[derive(Debug)]
#[repr(u8)]
pub enum OpCode {
    Continuation = 0x0,
    Text = 0x1, // Encoded in utf-8
    Binary = 0x2,
    Close = 0x8,
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
            0x8 => Ok(OpCode::Close),
            0x9 => Ok(OpCode::Ping),
            0xA => Ok(OpCode::Pong),
            0x10..=0xFF => Err(WebSocketError::BadOpCode(value)), // Op codes are only 4 bits
            opcode => Err(WebSocketError::OpCodeNotImplemented(opcode)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CloseCode {
    /// 0..=999 Unused
    /// 1000 Indicates a normal closure due to the purpose of the connection being fulfilled
    Normal,
    /// 1001 Indicates that an endpoint is "going away", such as a server going down or a browser
    /// navigating away
    GoingAway,
    /// 1002 Endpoint is terminating the connection due to a protocol error
    ProtocolError,
    /// 1003 Endpoint is terminating the connection because it has a received a type of data it cannot
    /// accept
    Unsupported,
    //1004 is reserved
    //1005 Cannot be a close control frame from an endpoint
    /// 1006 Sent from a client when the endpoint has closed the connection abnormally
    ConnectionClosedAbnormally,
    /// 1007 Indicates that an endpoint is terminating the connection because it has received data
    /// within a message that wasn't consistent with the type (opcode) of the message
    InconsistentData,
    /// 1008 Endpoint is terminating the connection be a received message violates it's policy. Can be
    /// used as a generic message for when there isn't a more accurate status code.
    PolicyViolated,
    /// 1009 Endpoint is terminating the connection because it has a received a message that is too big
    MessageTooBig,
    /// 1010 Client endpoint is terminating the connection because it did not receive extension
    /// negotiations that it expected in the handshake.
    MissingExtension,
    /// 1011 Server is terminating the connection due to an unexpected condition
    ServerError,
    /// 1015
    TLSFailure,

    /// 1000-2999 Reserved for protocol status codes
    ProtocolStatusCode(u16),

    /// 3000-3999 Reserved for registered use by libraries, frameworks, and applications
    RegisteredLibraryStatusCode(u16),

    /// 4000-4999 Reserved for private use by applications
    ApplicationStatusCode(u16),

    /// Any value outside this range
    Other(u16),
}

impl TryFrom<u16> for CloseCode {
    type Error = WebSocketError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0..=999 => Err(WebSocketError::UnusedCloseCode(value)),
            1016..=2999 => Ok(CloseCode::ProtocolStatusCode(value)),
            3000..=3999 => Ok(CloseCode::RegisteredLibraryStatusCode(value)),
            4000..=4999 => Ok(CloseCode::ApplicationStatusCode(value)),
            5000.. => Ok(CloseCode::Other(value)),
            1000 => Ok(CloseCode::Normal),
            1001 => Ok(CloseCode::GoingAway),
            1002 => Ok(CloseCode::ProtocolError),
            1003 => Ok(CloseCode::Unsupported),
            1006 => Ok(CloseCode::ConnectionClosedAbnormally),
            1007 => Ok(CloseCode::InconsistentData),
            1008 => Ok(CloseCode::PolicyViolated),
            1009 => Ok(CloseCode::MessageTooBig),
            1010 => Ok(CloseCode::MissingExtension),
            1011 => Ok(CloseCode::ServerError),
            1015 => Ok(CloseCode::TLSFailure),
            value => Ok(CloseCode::ProtocolStatusCode(value)),
        }
    }
}

impl From<CloseCode> for u16 {
    fn from(code: CloseCode) -> Self {
        match code {
            CloseCode::ProtocolStatusCode(val)
            | CloseCode::RegisteredLibraryStatusCode(val)
            | CloseCode::ApplicationStatusCode(val)
            | CloseCode::Other(val) => val,
            CloseCode::Normal => 1000,
            CloseCode::GoingAway => 1001,
            CloseCode::ProtocolError => 1002,
            CloseCode::Unsupported => 1003,
            CloseCode::ConnectionClosedAbnormally => 1006,
            CloseCode::InconsistentData => 1007,
            CloseCode::PolicyViolated => 1008,
            CloseCode::MessageTooBig => 1009,
            CloseCode::MissingExtension => 1010,
            CloseCode::ServerError => 1011,
            CloseCode::TLSFailure => 1015,
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

enum PayloadLength {
    Bit7(u8),
    Bit16(u16),
    Bit63(u64),
}

impl TryFrom<u64> for PayloadLength {
    type Error = WebSocketError;

    fn try_from(length: u64) -> Result<Self, Self::Error> {
        match length {
            length if length < (1 << 7) => Ok(PayloadLength::Bit7(length as u8)),
            length if length < (1 << 16) => Ok(PayloadLength::Bit16(length as u16)),
            length if length < (1 << 63) => Ok(PayloadLength::Bit63(length)),
            length => Err(WebSocketError::BadPayloadLength(length)),
        }
    }
}

#[derive(Debug)]
pub enum WebSocketError {
    BadOpCode(u8),
    OpCodeNotImplemented(u8),
    Io(std::io::Error),
    UnencodedMessage,
    BadPayloadLength(u64),
    UnusedCloseCode(u16),
    Close(CloseCode),
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
            WebSocketError::BadPayloadLength(length) => write!(f, "Payload length was {}", length),
            WebSocketError::UnusedCloseCode(code) => {
                write!(f, "Received unused close code {}", code)
            }
            WebSocketError::Close(code) => write!(f, "Close requested with code {}", Into::<u16>::into(*code)),
        }
    }
}

impl Error for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(value: std::io::Error) -> Self {
        WebSocketError::Io(value)
    }
}
