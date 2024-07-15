use std::io::Cursor;

use bytes::Buf;
use mc_varint::{VarInt, VarIntRead};
use snafu::{Backtrace, OptionExt, Snafu};
use tracing::trace;

use crate::mc_string::{decode_mc_string, McStringError};

#[derive(Snafu, Debug)]
pub enum FrameError {
    /// Received an incomplete frame.
    Incomplete { backtrace: Backtrace },
    /// I/O error.
    #[snafu(display("I/O error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    /// Received a frame with an invalid length.
    InvalidLength { backtrace: Backtrace },
    /// Received a frame with an invalid id.
    InvalidFrameId { id: i32, backtrace: Backtrace },
    /// Failed to decode string.
    #[snafu(display("Failed to decode string: {source}"), context(false))]
    StringDecodeFailed {
        #[snafu(backtrace)]
        source: McStringError,
    },
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

/// Controls what packets a server can receive
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[deprecated(
    since = "5.1.0",
    note = "Elytra Ping as a SLP server is untested and not supported"
)]
pub enum ServerState {
    /// Waiting for the Handshake packet
    Handshake,
    /// Ready to respond to status and ping requests
    Status,
}

impl Frame {
    pub const PROTOCOL_VERSION: i32 = 767;
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
            i32::from(buf.read_var_int().ok().context(IncompleteSnafu)?)
                .try_into()
                .ok()
                .context(InvalidLengthSnafu)?;
        let header_len = buf.position() as usize;
        let total_len = header_len + remaining_data_len;

        // if we don't have enough data the frame isn't valid yet
        let is_valid = available_data >= total_len;

        if is_valid {
            trace!("Valid frame, packet size: {total_len}, header size: {header_len}, body size: {remaining_data_len}, downloaded: {available_data}");
            Ok(())
        } else {
            trace!("Invalid frame, packet size: {total_len}, downloaded: {available_data}");
            IncompleteSnafu.fail()
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
                    let address = decode_mc_string(cursor)?;
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
                        let json = decode_mc_string(cursor)?;
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

        InvalidFrameIdSnafu { id }.fail()
    }
}
