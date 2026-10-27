//! Version-pinned packet constructors for the handshake and status states.
//! Packet IDs here are state-specific; do not reuse them in Login or Play.
use crate::protocol::{write_string, write_varint, ProtocolError};
use crate::session::PROTOCOL_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextState { Status, Login }

#[derive(Debug)]
pub enum PacketBuildError {
    Protocol(ProtocolError),
    InvalidPort(u16),
    InvalidUsernameLength(usize),
}
impl std::fmt::Display for PacketBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "packet encoding error: {e}"),
            Self::InvalidPort(p) => write!(f, "invalid TCP port: {p}"),
            Self::InvalidUsernameLength(n) => write!(f, "username length must be 1..=16 bytes, got {n}"),
        }
    }
}
impl std::error::Error for PacketBuildError {}
impl From<ProtocolError> for PacketBuildError {
    fn from(value: ProtocolError) -> Self { Self::Protocol(value) }
}

/// Builds Handshake packet (packet ID 0x00).
pub fn handshake(server_address: &str, port: u16, next: NextState) -> Result<Vec<u8>, PacketBuildError> {
    if port == 0 { return Err(PacketBuildError::InvalidPort(port)); }
    let mut p = Vec::with_capacity(server_address.len() + 16);
    write_varint(&mut p, 0x00);
    write_varint(&mut p, PROTOCOL_VERSION);
    write_string(&mut p, server_address)?;
    p.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut p, match next { NextState::Status => 1, NextState::Login => 2 });
    Ok(p)
}

/// Builds Status Request (packet ID 0x00 in Status state).
pub fn status_request() -> Vec<u8> { vec![0x00] }

/// Builds Ping Request (packet ID 0x01 followed by signed 64-bit payload).
pub fn status_ping(payload: i64) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.push(0x01);
    p.extend_from_slice(&payload.to_be_bytes());
    p
}

/// Builds Login Start (packet ID 0x00 in Login state) using a caller-provided UUID.
/// This does not authenticate an account; online-mode servers require the official
/// encrypted/session authentication flow.
pub fn login_start(username: &str, uuid: [u8; 16]) -> Result<Vec<u8>, PacketBuildError> {
    let len = username.as_bytes().len();
    if len == 0 || len > 16 { return Err(PacketBuildError::InvalidUsernameLength(len)); }
    let mut p = Vec::with_capacity(len + 20);
    write_varint(&mut p, 0x00);
    write_string(&mut p, username)?;
    p.extend_from_slice(&uuid);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{read_string, read_varint};

    #[test]
    fn handshake_fields_are_encoded() {
        let p = handshake("localhost", 25565, NextState::Login).unwrap();
        let mut c = 0;
        assert_eq!(read_varint(&p, &mut c).unwrap(), 0);
        assert_eq!(read_varint(&p, &mut c).unwrap(), PROTOCOL_VERSION);
        assert_eq!(read_string(&p, &mut c, 255).unwrap(), "localhost");
        assert_eq!(u16::from_be_bytes([p[c], p[c+1]]), 25565);
        c += 2;
        assert_eq!(read_varint(&p, &mut c).unwrap(), 2);
        assert_eq!(c, p.len());
    }

    #[test]
    fn login_start_has_username_and_uuid() {
        let p = login_start("Player", [7; 16]).unwrap();
        assert_eq!(p[0], 0);
        let mut c = 1;
        assert_eq!(read_string(&p, &mut c, 16).unwrap(), "Player");
        assert_eq!(&p[c..], &[7; 16]);
    }

    #[test]
    fn rejects_long_username() {
        assert!(login_start("this_name_is_way_too_long", [0; 16]).is_err());
    }
}
