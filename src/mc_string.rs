use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use std::io::Cursor;

mod error {
    use super::*;
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum McStringError {
        #[error("io error: {0}")]
        Io(#[from] std::io::Error),
        #[error(
            "string is too long (is {0} bytes, but expected less than {} bytes)",
            MAX_LEN
        )]
        TooLong(usize),
        #[error("invalid string format")]
        InvalidFormat,
    }
}

pub use error::McStringError;

pub const MAX_LEN: i32 = i32::MAX;

pub fn encode_mc_string(string: &str) -> Result<Vec<u8>, McStringError> {
    let len = string.len();
    // VarInt max length is 5 bytes
    let mut bytes = Vec::with_capacity(len + 5);
    bytes.write_var_int(VarInt::from(
        i32::try_from(len).ok().ok_or(McStringError::TooLong(len))?,
    ))?;
    bytes.extend_from_slice(string.as_bytes());
    Ok(bytes)
}

pub fn decode_mc_string(bytes: &[u8]) -> Result<&str, McStringError> {
    let mut bytes = Cursor::new(bytes);
    let len: i32 = bytes.read_var_int()?.into();
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
