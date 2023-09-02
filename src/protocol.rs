use crate::mc_string::McStringError;
use crate::mc_string::{decode_mc_string, encode_mc_string};
#[cfg(feature = "java_parse")]
use crate::parse::JavaServerInfo;
use bytes::{Buf, BytesMut};
use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use snafu::{Backtrace, GenerateImplicitData, Snafu};
use std::array::TryFromSliceError;
use std::{
    fmt::Debug,
    io::{Cursor, Write},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};
use tracing::{debug, event, instrument, trace, Level};

// module adapted from tokio's mini-redis
// which is licensed here: https://github.com/tokio-rs/mini-redis/blob/cefca5377af54520904c55764d16fc7c0a291902/LICENSE

#[derive(Snafu, Debug)]
#[snafu(visibility(pub(crate)), module)]
pub enum ProtocolError {
    #[snafu(display("io error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("dns lookup failed for address `{address}`"))]
    DNSLookupFailed {
        address: String,
        backtrace: Backtrace,
    },
    #[snafu(display("failed to encode string as bytes: {source}"), context(false))]
    StringEncodeFailed {
        source: McStringError,
        backtrace: Backtrace,
    },
    #[snafu(display(
        "failed to send packet because it is too long (more than {} bytes)",
        i32::MAX
    ))]
    PacketTooLong { backtrace: Backtrace },
    #[snafu(display("connection closed before packet finished being read"))]
    ConnectionClosed { backtrace: Backtrace },
    #[snafu(display("failed to parse packet: {source}"), context(false))]
    ParseFailed {
        source: FrameError,
        backtrace: Backtrace,
    },
    #[snafu(display("srv resolver creation failed: {source}"), context(false))]
    SrvResolveError {
        source: trust_dns_resolver::error::ResolveError,
        backtrace: Backtrace,
    },
}

#[derive(Snafu, Debug)]
#[snafu(visibility(pub(crate)), module)]
pub enum FrameError {
    #[snafu(display("frame is missing data"))]
    Incomplete { backtrace: Backtrace },
    #[snafu(display("io error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("frame declares it has negative length"))]
    InvalidLength { backtrace: Backtrace },
    #[snafu(display("cannot parse frame with id {id}"))]
    InvalidFrame { id: i32, backtrace: Backtrace },
    #[snafu(display("failed to decode string: {source}"), context(false))]
    StringDecodeFailed {
        source: McStringError,
        backtrace: Backtrace,
    },
    #[snafu(
        display("failed to decode ping response payload: {source}"),
        context(false)
    )]
    PingResponseDecodeFailed {
        source: TryFromSliceError,
        backtrace: Backtrace,
    },
}

#[cfg(feature = "simple")]
#[derive(Snafu, Debug)]
#[snafu(visibility(pub(crate)), module)]
pub enum PingError {
    #[snafu(display("connection failed"), context(false))]
    Protocol {
        source: ProtocolError,
        backtrace: Backtrace,
    },
    #[snafu(display("connection closed"))]
    ConnectionClosed { backtrace: Backtrace },
    #[snafu(display("invalid response from server"))]
    InvalidResponse { backtrace: Backtrace },
    #[snafu(display("failed to parse server response"), context(false))]
    Parse {
        source: serde_json::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("server did not respond in time"))]
    Timeout { backtrace: Backtrace },
}

#[derive(Debug)]
pub struct SlpProtocol {
    hostname: String,
    port: u16,
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
    PingRequest {
        payload: i64,
    },
    PingResponse {
        payload: i64,
    },
}

/// Controls what packets a server can recieve
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerState {
    /// Waiting for the Handshake packet
    Handshake,
    /// Ready to respond to status and ping requests
    Status,
}

impl Frame {
    pub const PROTOCOL_VERSION: i32 = 754;
    pub const HANDSHAKE_ID: i32 = 0x00;
    pub const STATUS_REQUEST_ID: i32 = 0x00;
    pub const STATUS_RESPONSE_ID: i32 = 0x00;
    pub const PING_REQUEST_ID: i32 = 0x01;
    pub const PING_RESPONSE_ID: i32 = 0x01;

    /// Checks if an entire message can be decoded from `buf`, advancing the cursor past the header
    pub fn check(buf: &mut Cursor<&[u8]>) -> Result<(), FrameError> {
        let available_data = buf.get_ref().len();

        // the varint at the beginning contains the size of the rest of the frame
        let remaining_data_len: usize =
            i32::from(buf.read_var_int().map_err(|_| FrameError::Incomplete {
                backtrace: Backtrace::generate(),
            })?)
            .try_into()
            .map_err(|_| FrameError::InvalidLength {
                backtrace: Backtrace::generate(),
            })?;
        let header_len = buf.position() as usize;
        let total_len = header_len + remaining_data_len;

        // if we don't have enough data the frame isn't valid yet
        let is_valid = available_data >= total_len;

        if is_valid {
            trace!("Valid frame, packet size: {total_len}, header size: {header_len}, body size: {remaining_data_len}, downloaded: {available_data}");
            Ok(())
        } else {
            trace!("Invalid frame, packet size: {total_len}, downloaded: {available_data}");
            Err(FrameError::Incomplete {
                backtrace: Backtrace::generate(),
            })
        }
    }

    /// Parse the body of a frame, after the message has already been validated with `check`.
    ///
    /// # Arguments
    ///
    /// * `src` - The buffer containing the message
    /// * `server_state` - Switches between which type of frame to accept. Set to None to accept frames for the client.
    pub fn parse(
        cursor: &mut Cursor<&[u8]>,
        server_state: Option<ServerState>,
    ) -> Result<Frame, FrameError> {
        let id = i32::from(cursor.read_var_int()?);

        match server_state {
            Some(ServerState::Handshake) => {
                if id == Self::HANDSHAKE_ID {
                    let protocol = cursor.read_var_int()?;
                    let address = decode_mc_string(cursor)?.to_owned();
                    let port = cursor.get_u16();
                    let state = cursor.read_var_int()?;
                    return Ok(Frame::Handshake {
                        protocol,
                        address,
                        port,
                        state,
                    });
                }
            }
            Some(ServerState::Status) => {
                match id {
                    Self::STATUS_REQUEST_ID => {
                        return Ok(Frame::StatusRequest);
                    }
                    Self::PING_REQUEST_ID => {
                        // ping request a contains (usually) meaningless Java long
                        let payload = cursor.get_i64();
                        return Ok(Frame::PingRequest { payload });
                    }
                    _ => {}
                }
            }
            None => {
                match id {
                    Self::STATUS_RESPONSE_ID => {
                        let json = decode_mc_string(cursor)?.to_owned();
                        return Ok(Frame::StatusResponse { json });
                    }
                    Self::PING_RESPONSE_ID => {
                        // ping response contains the same Java long as the request
                        let payload = cursor.get_i64();
                        return Ok(Frame::PingResponse { payload });
                    }
                    _ => {}
                }
            }
        }

        Err(FrameError::InvalidFrame {
            id,
            backtrace: Backtrace::generate(),
        })
    }
}

#[repr(i32)]
pub enum ProtocolState {
    Status = 1,
    Login = 2,
}
impl SlpProtocol {
    pub fn new(hostname: String, port: u16, stream: TcpStream) -> Self {
        Self {
            hostname,
            port,
            stream: BufWriter::new(stream),
            buffer: BytesMut::with_capacity(4096),
        }
    }

    pub fn create_handshake_frame(&self) -> Frame {
        Frame::Handshake {
            protocol: VarInt::from(Frame::PROTOCOL_VERSION),
            address: self.hostname.to_owned(),
            port: self.port,
            state: VarInt::from(ProtocolState::Status as i32),
        }
    }

    /// Sends frame data over the connection as a packet.
    #[instrument]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<(), ProtocolError> {
        debug!("Writing frame: {frame:?}");

        let mut packet_data: Vec<u8> = Vec::with_capacity(5);

        match frame {
            Frame::Handshake {
                protocol,
                address,
                port,
                state,
            } => {
                trace!("writing handshake frame");
                packet_data.write_var_int(VarInt::from(Frame::HANDSHAKE_ID))?;
                packet_data.write_var_int(protocol)?;
                Write::write(&mut packet_data, &encode_mc_string(&address)?)?;
                Write::write(&mut packet_data, &port.to_be_bytes())?;
                packet_data.write_var_int(state)?;
            }
            Frame::StatusRequest => {
                trace!("writing status request frame");
                packet_data.write_var_int(VarInt::from(Frame::STATUS_REQUEST_ID))?;
            }
            Frame::StatusResponse { json } => {
                trace!("writing status response frame");
                packet_data.write_var_int(VarInt::from(Frame::STATUS_RESPONSE_ID))?;
                Write::write(&mut packet_data, &encode_mc_string(&json)?)?;
            }
            Frame::PingRequest { payload } => {
                trace!("writing ping request frame");
                packet_data.write_var_int(VarInt::from(Frame::PING_REQUEST_ID))?;
                Write::write(&mut packet_data, &payload.to_be_bytes())?;
            }
            Frame::PingResponse { payload } => {
                trace!("writing ping response frame");
                packet_data.write_var_int(VarInt::from(Frame::PING_RESPONSE_ID))?;
                Write::write(&mut packet_data, &payload.to_be_bytes())?;
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

        trace!("sending the packet!");
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Recieve and parse a frame from the connection.
    ///
    /// # Arguments
    ///
    /// * `server_state` - Switches between which type of frame to accept. Set to None to accept frames for the client.
    pub async fn read_frame(
        &mut self,
        server_state: Option<ServerState>,
    ) -> Result<Option<Frame>, ProtocolError> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame(server_state)? {
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
                    return Err(ProtocolError::ConnectionClosed {
                        backtrace: Backtrace::generate(),
                    });
                }
            }
        }
    }

    /// Parse the most recent frame from the connection, removing it from the buffer.
    ///
    /// # Arguments
    ///
    /// * `server_state` - Switches between which type of frame to accept. Set to None to accept frames for the client.
    pub fn parse_frame(
        &mut self,
        server_state: Option<ServerState>,
    ) -> Result<Option<Frame>, ProtocolError> {
        let mut cursor = Cursor::new(&self.buffer[..]);

        // Check whether a full frame is available
        match Frame::check(&mut cursor) {
            Ok(()) => {
                let frame = Frame::parse(&mut cursor, server_state)?;

                trace!("Discarding frame from buffer");
                // current cursor position is the entire frame
                self.buffer.advance(cursor.position() as usize);

                // Return the frame to the caller.
                Ok(Some(frame))
            }
            // Not enough data has been buffered
            Err(FrameError::Incomplete { .. }) => Ok(None),
            // An error was encountered
            Err(e) => Err(e.into()),
        }
    }

    pub async fn disconnect(mut self) -> Result<(), ProtocolError> {
        self.stream.shutdown().await?;
        Ok(())
    }

    #[cfg(feature = "simple")]
    pub async fn handshake(&mut self) -> Result<(), ProtocolError> {
        self.write_frame(self.create_handshake_frame()).await?;
        Ok(())
    }

    #[cfg(feature = "simple")]
    pub async fn get_status(&mut self) -> Result<JavaServerInfo, PingError> {
        use std::str::FromStr;

        self.write_frame(Frame::StatusRequest).await?;
        let frame = self
            .read_frame(None)
            .await?
            .ok_or_else(|| ping_error::ConnectionClosedSnafu.build())?;
        let frame_data = match frame {
            Frame::StatusResponse { json } => json,
            _ => return Err(ping_error::InvalidResponseSnafu.build()),
        };
        Ok(JavaServerInfo::from_str(&frame_data)?)
    }

    #[cfg(feature = "simple")]
    pub async fn get_latency(&mut self) -> Result<Duration, PingError> {
        use std::time::Instant;
        const PING_PAYLOAD: i64 = 54321;

        let ping_time = Instant::now();

        self.write_frame(Frame::PingRequest {
            payload: PING_PAYLOAD,
        })
        .await?;
        let frame = self
            .read_frame(None)
            .await?
            .ok_or_else(|| ping_error::ConnectionClosedSnafu.build())?;
        match frame {
            Frame::PingResponse { payload: _ } => Ok(ping_time.elapsed()),
            _ => Err(ping_error::InvalidResponseSnafu.build()),
        }
    }
}
