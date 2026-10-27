//! End-to-end Minecraft server-list ping using the framed TCP transport.
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::handshake::{handshake, status_ping, status_request, NextState};
use crate::net::{MinecraftConnection, NetworkError};
use crate::server_status::{parse_ping_response, parse_status_response, ServerStatus, StatusError};

#[derive(Debug)]
pub enum PingError {
    Network(NetworkError),
    Status(StatusError),
    Clock,
}
impl std::fmt::Display for PingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "server ping network error: {e}"),
            Self::Status(e) => write!(f, "server ping protocol error: {e}"),
            Self::Clock => write!(f, "system clock is before Unix epoch"),
        }
    }
}
impl std::error::Error for PingError {}
impl From<NetworkError> for PingError {
    fn from(e: NetworkError) -> Self { Self::Network(e) }
}
impl From<StatusError> for PingError {
    fn from(e: StatusError) -> Self { Self::Status(e) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingResult {
    pub status: ServerStatus,
    pub latency: Duration,
}

/// Performs a server-list status handshake and ping.
/// host is the hostname/IP without a port; port is the server TCP port.
pub fn ping_server(host: &str, port: u16, timeout: Duration) -> Result<PingResult, PingError> {
    let socket_address = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut connection = MinecraftConnection::connect(&socket_address, timeout)?;
    let handshake_packet = handshake(host, port, NextState::Status).map_err(|e| {
        match e {
            crate::handshake::PacketBuildError::Protocol(p) => PingError::Network(NetworkError::Protocol(p)),
            crate::handshake::PacketBuildError::InvalidPort(_) =>
                PingError::Network(NetworkError::Protocol(crate::protocol::ProtocolError::Truncated("invalid port"))),
            crate::handshake::PacketBuildError::InvalidUsernameLength(_) =>
                PingError::Network(NetworkError::Protocol(crate::protocol::ProtocolError::Truncated("invalid username"))),
        }
    })?;
    connection.write_packet(&handshake_packet)?;
    connection.write_packet(&status_request())?;
    let status_packet = connection.read_packet()?;
    let status = parse_status_response(&status_packet)?;

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| PingError::Clock)?.as_millis() as i64;
    let start = Instant::now();
    connection.write_packet(&status_ping(nonce))?;
    let pong = connection.read_packet()?;
    let echoed = parse_ping_response(&pong)?;
    if echoed != nonce {
        return Err(PingError::Status(StatusError::MissingPingPayload));
    }
    let latency = start.elapsed();
    let _ = connection.shutdown();
    Ok(PingResult { status, latency })
}
