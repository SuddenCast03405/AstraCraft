//! Status-state packet decoding for server-list ping and latency measurement.
use serde_json::Value;

use crate::chat::flatten_component;
use crate::protocol::{read_string, read_varint, ProtocolError};

pub const MAX_STATUS_JSON_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    pub version_name: Option<String>,
    pub protocol: Option<i64>,
    pub online_players: Option<i64>,
    pub max_players: Option<i64>,
    pub description: String,
    pub favicon: Option<String>,
    pub enforces_secure_chat: Option<bool>,
}
#[derive(Debug)]
pub enum StatusError {
    Protocol(ProtocolError),
    WrongPacketId(i32),
    MissingJson,
    JsonTooLarge(usize),
    InvalidJson(String),
    MissingPingPayload,
    TrailingBytes(usize),
}
impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "status packet protocol error: {e}"),
            Self::WrongPacketId(id) => write!(f, "unexpected status packet ID {id}"),
            Self::MissingJson => write!(f, "status response has no JSON string"),
            Self::JsonTooLarge(n) => write!(f, "status JSON too large: {n} bytes"),
            Self::InvalidJson(e) => write!(f, "invalid status JSON: {e}"),
            Self::MissingPingPayload => write!(f, "ping response missing 64-bit payload"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes in status packet"),
        }
    }
}
impl std::error::Error for StatusError {}
impl From<ProtocolError> for StatusError {
    fn from(e: ProtocolError) -> Self { Self::Protocol(e) }
}

pub fn parse_status_response(packet: &[u8]) -> Result<ServerStatus, StatusError> {
    let mut cursor = 0;
    let id = read_varint(packet, &mut cursor)?;
    if id != 0 { return Err(StatusError::WrongPacketId(id)); }
    if cursor >= packet.len() { return Err(StatusError::MissingJson); }
    let json_len_start = cursor;
    let json = read_string(packet, &mut cursor, MAX_STATUS_JSON_BYTES)?;
    let consumed = cursor - json_len_start;
    if consumed > MAX_STATUS_JSON_BYTES + 5 {
        return Err(StatusError::JsonTooLarge(consumed));
    }
    if cursor != packet.len() {
        return Err(StatusError::TrailingBytes(packet.len() - cursor));
    }
    let value: Value = serde_json::from_str(&json).map_err(|e| StatusError::InvalidJson(e.to_string()))?;

    let version = value.get("version");
    let players = value.get("players");
    let description = value.get("description").map(|v| flatten_component(v)).unwrap_or_default();
    Ok(ServerStatus {
        version_name: version.and_then(|v| v.get("name")).and_then(Value::as_str).map(str::to_owned),
        protocol: version.and_then(|v| v.get("protocol")).and_then(Value::as_i64),
        online_players: players.and_then(|v| v.get("online")).and_then(Value::as_i64),
        max_players: players.and_then(|v| v.get("max")).and_then(Value::as_i64),
        description,
        favicon: value.get("favicon").and_then(Value::as_str).map(str::to_owned),
        enforces_secure_chat: value.get("enforcesSecureChat").and_then(Value::as_bool),
    })
}

/// Reads Status Ping Response (packet ID 0x01, then an i64 echo payload).
pub fn parse_ping_response(packet: &[u8]) -> Result<i64, StatusError> {
    let mut cursor = 0;
    let id = read_varint(packet, &mut cursor)?;
    if id != 1 { return Err(StatusError::WrongPacketId(id)); }
    let bytes = packet.get(cursor..cursor + 8).ok_or(StatusError::MissingPingPayload)?;
    cursor += 8;
    if cursor != packet.len() { return Err(StatusError::TrailingBytes(packet.len() - cursor)); }
    Ok(i64::from_be_bytes(bytes.try_into().expect("exactly eight bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{write_string, write_varint};

    #[test]
    fn parses_server_status() {
        let json = r#"{"version":{"name":"1.21.8","protocol":772},"players":{"online":3,"max":20},"description":{"text":"Hello ","extra":[{"text":"world"}]},"enforcesSecureChat":true}"#;
        let mut packet = vec![];
        write_varint(&mut packet, 0);
        write_string(&mut packet, json).unwrap();
        let s = parse_status_response(&packet).unwrap();
        assert_eq!(s.version_name.as_deref(), Some("1.21.8"));
        assert_eq!(s.protocol, Some(772));
        assert_eq!(s.online_players, Some(3));
        assert_eq!(s.max_players, Some(20));
        assert_eq!(s.description, "Hello world");
        assert_eq!(s.enforces_secure_chat, Some(true));
    }

    #[test]
    fn parses_ping_echo() {
        let mut p = vec![1];
        p.extend_from_slice(&123456i64.to_be_bytes());
        assert_eq!(parse_ping_response(&p).unwrap(), 123456);
    }

    #[test]
    fn rejects_wrong_packet_id() {
        assert!(matches!(parse_ping_response(&[0]), Err(StatusError::WrongPacketId(0))));
    }
}
