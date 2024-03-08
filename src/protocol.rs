pub use self::frame::{Frame, FrameError, ServerState};
use crate::mc_string::encode_mc_string;
use crate::mc_string::McStringError;
#[cfg(feature = "java_parse")]
use crate::parse::JavaServerInfo;
use bytes::{Buf, BytesMut};
use mc_varint::{VarInt, VarIntWrite};
use snafu::OptionExt;
use snafu::{Backtrace, GenerateImplicitData, Snafu};
use std::str::FromStr;
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

mod frame;

#[derive(Snafu, Debug)]
pub enum ProtocolError {
    #[snafu(display("io error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("failed to encode string as bytes: {source}"), context(false))]
    StringEncodeFailed {
        #[snafu(backtrace)]
        source: McStringError,
    },
    #[snafu(display(
        "failed to send packet because it is too long (more than {} bytes)",
        i32::MAX
    ))]
    PacketTooLong { backtrace: Backtrace },
    #[snafu(display("connection closed unexpectedly"))]
    ConnectionClosed { backtrace: Backtrace },
    #[snafu(display("failed to parse packet: {source}"), context(false))]
    ParseFailed {
        #[snafu(backtrace)]
        source: FrameError,
    },
    #[snafu(display("srv resolver creation failed: {source}"), context(false))]
    SrvResolveError {
        source: trust_dns_resolver::error::ResolveError,
        backtrace: Backtrace,
    },
    #[snafu(display("packet received out of order"))]
    FrameOutOfOrder { backtrace: Backtrace },
    #[snafu(display("failed to parse server response: {source}"), context(false))]
    JsonParse {
        source: serde_json::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("dns lookup failed for address `{address}`"))]
    DNSLookupFailed {
        address: String,
        backtrace: Backtrace,
    },
}

#[derive(Debug)]
pub struct SlpProtocol {
    hostname: String,
    port: u16,
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
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
    pub async fn get_status(&mut self) -> Result<JavaServerInfo, ProtocolError> {
        self.write_frame(Frame::StatusRequest).await?;
        let frame = self
            .read_frame(None)
            .await?
            .context(ConnectionClosedSnafu)?;
        let frame_data = match frame {
            Frame::StatusResponse { json } => json,
            _ => return FrameOutOfOrderSnafu.fail(),
        };
        Ok(JavaServerInfo::from_str(&frame_data)?)
    }

    #[cfg(feature = "simple")]
    pub async fn get_latency(&mut self) -> Result<Duration, ProtocolError> {
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
            .context(ConnectionClosedSnafu)?;
        match frame {
            Frame::PingResponse { payload: _ } => Ok(ping_time.elapsed()),
            _ => FrameOutOfOrderSnafu.fail(),
        }
    }
}

#[cfg(feature = "java_connect")]
#[instrument]
pub async fn connect(mut addrs: (String, u16)) -> Result<SlpProtocol, ProtocolError> {
    use tokio::net::lookup_host;
    use tracing::{debug, info};
    use trust_dns_resolver::TokioAsyncResolver;

    let resolver = TokioAsyncResolver::tokio_from_system_conf()?;
    if let Ok(records) = resolver
        .srv_lookup(format!("_minecraft._tcp.{}", addrs.0))
        .await
    {
        if let Some(record) = records.iter().next() {
            let record = record.target().to_utf8();
            debug!("Found SRV record: {} -> {}", addrs.0, record);
            addrs.0 = record;
        }
    }

    // lookup_host can return multiple but we just need one so we discard the rest
    let socket_addrs = match lookup_host(addrs.clone()).await?.next() {
        Some(socket_addrs) => socket_addrs,
        None => {
            info!("DNS lookup failed for address");
            return DNSLookupFailedSnafu { address: addrs.0 }.fail();
        }
    };

    match TcpStream::connect(socket_addrs).await {
        Ok(stream) => {
            info!("Connected to SLP server");
            Ok(SlpProtocol::new(addrs.0, addrs.1, stream))
        }
        Err(error) => {
            info!("Failed to connect to SLP server: {}", error);
            Err(error.into())
        }
    }
}
