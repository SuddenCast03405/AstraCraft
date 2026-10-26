use std::fmt;

pub const MAX_FRAME_LENGTH: usize = 2 * 1024 * 1024;
pub const MAX_STRING_BYTES: usize = 32_767;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    Truncated(&'static str),
    VarIntTooLong,
    NegativeLength(i32),
    FrameTooLarge(usize),
    InvalidUtf8,
    TrailingBytes(usize),
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(what) => write!(f, "truncated packet while reading {what}"),
            Self::VarIntTooLong => write!(f, "VarInt exceeds 5 bytes"),
            Self::NegativeLength(n) => write!(f, "negative length: {n}"),
            Self::FrameTooLarge(n) => write!(f, "frame too large: {n} bytes"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes"),
        }
    }
}
impl std::error::Error for ProtocolError {}

pub fn read_varint(input: &[u8], cursor: &mut usize) -> Result<i32, ProtocolError> {
    let mut result = 0u32;
    for index in 0..5 {
        let byte = *input.get(*cursor).ok_or(ProtocolError::Truncated("VarInt"))?;
        *cursor += 1;
        if index == 4 && (byte & 0xf0) != 0 {
            return Err(ProtocolError::VarIntTooLong);
        }
        result |= ((byte & 0x7f) as u32) << (7 * index);
        if byte & 0x80 == 0 {
            return Ok(result as i32);
        }
    }
    Err(ProtocolError::VarIntTooLong)
}

pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            return;
        }
        out.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
}

pub fn read_string(input: &[u8], cursor: &mut usize, max_bytes: usize) -> Result<String, ProtocolError> {
    let length = read_varint(input, cursor)?;
    if length < 0 { return Err(ProtocolError::NegativeLength(length)); }
    let length = length as usize;
    if length > max_bytes || length > MAX_STRING_BYTES {
        return Err(ProtocolError::FrameTooLarge(length));
    }
    let end = cursor.checked_add(length).ok_or(ProtocolError::FrameTooLarge(length))?;
    let bytes = input.get(*cursor..end).ok_or(ProtocolError::Truncated("string"))?;
    *cursor = end;
    String::from_utf8(bytes.to_vec()).map_err(|_| ProtocolError::InvalidUtf8)
}

pub fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), ProtocolError> {
    if value.len() > MAX_STRING_BYTES {
        return Err(ProtocolError::FrameTooLarge(value.len()));
    }
    write_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

pub fn encode_packet(packet_payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if packet_payload.len() > MAX_FRAME_LENGTH {
        return Err(ProtocolError::FrameTooLarge(packet_payload.len()));
    }
    let mut out = Vec::with_capacity(packet_payload.len() + 5);
    write_varint(&mut out, packet_payload.len() as i32);
    out.extend_from_slice(packet_payload);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_round_trip() {
        for n in [0, 1, 127, 128, 255, 2147483647, -1, i32::MIN] {
            let mut bytes = Vec::new();
            write_varint(&mut bytes, n);
            let mut cursor = 0;
            assert_eq!(read_varint(&bytes, &mut cursor).unwrap(), n);
            assert_eq!(cursor, bytes.len());
        }
    }

    #[test]
    fn packet_frame_prefix() {
        assert_eq!(encode_packet(&[0x01, 0x02]).unwrap(), vec![2, 1, 2]);
    }

    #[test]
    fn strings_round_trip() {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "AstraCraft").unwrap();
        let mut cursor = 0;
        assert_eq!(read_string(&bytes, &mut cursor, 100).unwrap(), "AstraCraft");
    }
}
