use bytes::Buf;
use mc_varint::{VarInt, VarIntRead, VarIntWrite};
use snafu::{OptionExt, Snafu};
use std::io::Cursor;

#[derive(Snafu, Debug)]
pub enum McStringError {
    #[snafu(display("io error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: snafu::Backtrace,
    },
    #[snafu(display(
        "string is too long (is {length} bytes, but expected less than {} bytes)",
        MAX_LEN
    ))]
    TooLong {
        length: usize,
        backtrace: snafu::Backtrace,
    },
    #[snafu(display("invalid string format"))]
    InvalidFormat { backtrace: snafu::Backtrace },
}

pub const MAX_LEN: i32 = i32::MAX;

pub fn encode_mc_string(string: &str) -> Result<Vec<u8>, McStringError> {
    let len = string.len();
    // VarInt max length is 5 bytes
    let mut bytes = Vec::with_capacity(len + 5);
    bytes.write_var_int(VarInt::from(
        i32::try_from(len)
            .ok()
            .context(TooLongSnafu { length: len })?,
    ))?;
    bytes.extend_from_slice(string.as_bytes());
    Ok(bytes)
}

pub fn decode_mc_string(cursor: &mut Cursor<&[u8]>) -> Result<String, McStringError> {
    let len: i32 = cursor.read_var_int()?.into();
    let len = usize::try_from(len).ok().context(InvalidFormatSnafu)?;

    let bytes = cursor.chunk();
    if len > bytes.len() {
        return InvalidFormatSnafu.fail();
    }
    let string = std::str::from_utf8(bytes.get(..len).context(InvalidFormatSnafu)?)
        .ok()
        .context(InvalidFormatSnafu)?
        .to_string();
    cursor.advance(len);
    Ok(string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_decode_mc_string() {
        const STRING: &str = "hello world!!";
        let bytes = encode_mc_string(STRING).unwrap();
        let mut cursor = Cursor::new(bytes.as_slice());
        let decoded_string = decode_mc_string(&mut cursor).unwrap();
        assert_eq!(decoded_string, STRING);
    }
}
