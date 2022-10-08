use crate::mc_string::{decode_mc_string, encode_mc_string};
use bytes::{Buf, BytesMut};
use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use std::{
    io::{Cursor, Write},
    net::SocketAddr,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};
use tracing::{event, instrument, Level};

// module adapted from tokio's mini-redis
// which is licensed here: https://github.com/tokio-rs/mini-redis/blob/cefca5377af54520904c55764d16fc7c0a291902/LICENSE

mod error {
    use thiserror::Error;

    use crate::mc_string::McStringError;

    #[derive(Error, Debug)]
    pub enum SlpError {
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
        #[error("dns lookup failed for address `{0}`")]
        DNSLookupFailed(String),
        #[error("failed to encode string as bytes: {0}")]
        StringEncodeFailed(#[from] McStringError),
        #[error(
            "failed to send packet because it is too long (more than {} bytes)",
            i32::MAX
        )]
        PacketTooLong,
        #[error("connection closed before packet finished being read")]
        ConnectionClosed,
        #[error("failed to parse packet: {0}")]
        ParseFailed(#[from] FrameError),
    }

    #[derive(Error, Debug)]
    pub enum FrameError {
        #[error("frame is missing data")]
        Incomplete,
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
        #[error("frame declares it has negative length")]
        InvalidLength,
        #[error("cannot parse frame with id {0}")]
        InvalidFrame(i32),
        #[error("failed to decode string: {0}")]
        StringDecodeFailed(#[from] McStringError),
    }
}

use self::error::FrameError;
pub use self::error::SlpError;

#[derive(Debug)]
pub struct SlpProtocol {
    addrs: SocketAddr,
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Frame {
    Handshake {
        protocol: VarInt,
        address: String,
        port: u16,
        // should be 1 for status
        state: VarInt,
    },
    StatusRequest,
    StatusResponse {
        json: String,
    },
}

impl Frame {
    pub const PROTOCOL_VERSION: i32 = 754;
    pub const HANDSHAKE_ID: i32 = 0x00;
    pub const STATUS_REQUEST_ID: i32 = 0x00;
    pub const STATUS_RESPONSE_ID: i32 = 0x00;

    /// Checks if an entire message can be decoded from `buf`
    pub fn check(buf: &mut Cursor<&[u8]>) -> Result<usize, FrameError> {
        let buf_len = buf.get_ref().len();
        // the varint at the beginning contains the size of the rest of the frame
        let remaining_data_len: usize =
            i32::from(buf.read_var_int().map_err(|_| FrameError::Incomplete)?)
                .try_into()
                .map_err(|_| FrameError::InvalidLength)?;
        let header_len = buf.position() as usize;
        let total_len = header_len + remaining_data_len;

        // if we don't have enough data the frame isn't valid yet
        let is_valid = buf_len >= total_len;

        if is_valid {
            Ok(total_len - 1)
        } else {
            Err(FrameError::Incomplete)
        }
    }

    /// The message has already been validated with `check`.
    pub fn parse(src: &mut Cursor<&[u8]>) -> Result<Frame, FrameError> {
        let id = i32::from(src.read_var_int()?);

        match id {
            Self::STATUS_RESPONSE_ID => {
                let json = decode_mc_string(src.chunk())?.to_owned();
                Ok(Frame::StatusResponse { json })
            }
            id => Err(FrameError::InvalidFrame(id)),
        }
    }
}

#[repr(i32)]
pub enum ProtocolState {
    Status = 1,
    Login = 2,
}

impl SlpProtocol {
    pub fn new(addrs: SocketAddr, stream: TcpStream) -> Self {
        Self {
            addrs,
            stream: BufWriter::new(stream),
            buffer: BytesMut::with_capacity(4096),
        }
    }

    pub fn create_handshake_frame(&self) -> Frame {
        let ip = self.addrs.ip().to_string();
        Frame::Handshake {
            protocol: VarInt::from(Frame::PROTOCOL_VERSION),
            address: ip,
            port: self.addrs.port(),
            state: VarInt::from(ProtocolState::Status as i32),
        }
    }

    /// Sends frame data over the connection as a packet.
    #[instrument]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<(), SlpError> {
        let mut packet_data: Vec<u8> = Vec::with_capacity(5);

        match frame {
            Frame::Handshake {
                protocol,
                address,
                port,
                state,
            } => {
                event!(Level::TRACE, "writing handshake frame");
                packet_data.write_var_int(VarInt::from(Frame::HANDSHAKE_ID as i32))?;
                packet_data.write_var_int(protocol)?;
                Write::write(&mut packet_data, &encode_mc_string(&address)?)?;
                Write::write(&mut packet_data, &port.to_be_bytes())?;
                packet_data.write_var_int(state)?;
            }
            Frame::StatusRequest => {
                event!(Level::TRACE, "writing status request frame");
                packet_data.write_var_int(VarInt::from(Frame::STATUS_REQUEST_ID as i32))?;
            }
            Frame::StatusResponse { json } => {
                event!(Level::TRACE, "writing status response frame");
                packet_data.write_var_int(VarInt::from(Frame::STATUS_RESPONSE_ID as i32))?;
                Write::write(&mut packet_data, &encode_mc_string(&json)?)?;
            }
        }

        let len = VarInt::from(i32::try_from(packet_data.len()).unwrap());
        event!(
            Level::TRACE,
            "combining packet length (of {}) and data",
            packet_data.len()
        );
        let mut packet: Vec<u8> = Vec::with_capacity(packet_data.len() + 5);
        packet.write_var_int(len)?;
        Write::write(&mut packet, &packet_data)?;

        event!(Level::TRACE, "sending the packet!");
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub async fn read_frame(&mut self) -> Result<Option<Frame>, SlpError> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            // There is not enough buffered data to read a frame. Attempt to
            // read more data from the socket.
            //
            // On success, the number of bytes is returned. `0` indicates "end
            // of stream".
            let bytes_read = self.stream.read_buf(&mut self.buffer).await?;
            if bytes_read == 0 {
                // The remote closed the connection. For this to be a clean
                // shutdown, there should be no data in the read buffer. If
                // there is, this means that the peer closed the socket while
                // sending a frame.
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err(SlpError::ConnectionClosed);
                }
            }
        }
    }

    pub fn parse_frame(&mut self) -> Result<Option<Frame>, SlpError> {
        let mut buf = Cursor::new(&self.buffer[..]);

        // Check whether a full frame is available
        match Frame::check(&mut buf) {
            Ok(len) => {
                // Get the byte length of the header
                let header_len = (buf.position() as usize) - 1;

                // Parse the frame
                let frame = Frame::parse(&mut buf)?;

                // Discard the frame from the buffer
                self.buffer.advance(header_len + len);

                // Return the frame to the caller.
                Ok(Some(frame))
            }
            // Not enough data has been buffered
            Err(FrameError::Incomplete) => Ok(None),
            // An error was encountered
            Err(e) => Err(e.into()),
        }
    }

    pub async fn disconnect(mut self) -> Result<(), SlpError> {
        self.stream.shutdown().await?;
        Ok(())
    }
}
