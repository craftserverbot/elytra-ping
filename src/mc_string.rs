use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use std::io::Cursor;

mod error {
    use super::*;
    use snafu::Snafu;

    #[derive(Snafu, Debug)]
    pub enum McStringError {
        #[snafu(display("io error: {source}"))]
        Io { source: std::io::Error },
        #[snafu(display(
            "string is too long (is {length} bytes, but expected less than {} bytes)",
            MAX_LEN
        ))]
        TooLong { length: usize },
        #[snafu(display("invalid string format"))]
        InvalidFormat,
    }
}

pub use error::McStringError;

pub const MAX_LEN: i32 = i32::MAX;

pub fn encode_mc_string(string: &str) -> Result<Vec<u8>, McStringError> {
    let len = string.len();
    // VarInt max length is 5 bytes
    let mut bytes = Vec::with_capacity(len + 5);
    bytes
        .write_var_int(VarInt::from(
            i32::try_from(len)
                .ok()
                .ok_or(McStringError::TooLong { length: len })?,
        ))
        .map_err(|io| McStringError::Io { source: io })?;
    bytes.extend_from_slice(string.as_bytes());
    Ok(bytes)
}

pub fn decode_mc_string(bytes: &[u8]) -> Result<&str, McStringError> {
    let mut bytes = Cursor::new(bytes);
    let len: i32 = bytes
        .read_var_int()
        .map_err(|io| McStringError::Io { source: io })?
        .into();
    let len = usize::try_from(len).map_err(|_| McStringError::InvalidFormat)?;

    let string_start = bytes.position() as usize;
    let bytes = bytes.into_inner();
    let string = std::str::from_utf8(&bytes[string_start..string_start + len])
        .map_err(|_| McStringError::InvalidFormat)?;
    Ok(string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_decode_mc_string() {
        const STRING: &str = "hello world!!";
        let bytes = encode_mc_string(STRING).unwrap();
        let decoded_string = decode_mc_string(&bytes).unwrap();
        assert_eq!(decoded_string, STRING);
    }
}
