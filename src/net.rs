//! Bounded TCP framing with optional Minecraft zlib packet compression.
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::compression::{CompressionError, PacketCompression};
use crate::protocol::{read_varint, write_varint, ProtocolError, MAX_FRAME_LENGTH};

#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    Protocol(ProtocolError),
    Compression(CompressionError),
}
impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "network I/O error: {e}"),
            Self::Protocol(e) => write!(f, "protocol framing error: {e}"),
            Self::Compression(e) => write!(f, "packet compression error: {e}"),
        }
    }
}
impl std::error::Error for NetworkError {}
impl From<io::Error> for NetworkError { fn from(value: io::Error) -> Self { Self::Io(value) } }
impl From<ProtocolError> for NetworkError { fn from(value: ProtocolError) -> Self { Self::Protocol(value) } }
impl From<CompressionError> for NetworkError { fn from(value: CompressionError) -> Self { Self::Compression(value) } }

pub struct MinecraftConnection {
    stream: TcpStream,
    max_frame_length: usize,
    bytes_in: u64,
    bytes_out: u64,
    compression: Option<PacketCompression>,
}
impl MinecraftConnection {
    pub fn connect(address: &str, timeout: Duration) -> Result<Self, NetworkError> {
        let mut addrs = address.to_socket_addrs()?;
        let mut last_error = None;
        let mut stream = None;
        for addr in addrs.by_ref() {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(s) => { stream = Some(s); break; }
                Err(e) => last_error = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| NetworkError::Io(last_error.unwrap_or_else(
            || io::Error::new(io::ErrorKind::InvalidInput, "address resolved to no endpoints")
        )))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self::new(stream))
    }

    pub fn from_stream(stream: TcpStream) -> Result<Self, NetworkError> {
        stream.set_nodelay(true)?;
        Ok(Self::new(stream))
    }

    fn new(stream: TcpStream) -> Self {
        Self { stream, max_frame_length: MAX_FRAME_LENGTH, bytes_in: 0, bytes_out: 0, compression: None }
    }

    pub fn set_max_frame_length(&mut self, max: usize) { self.max_frame_length = max.min(MAX_FRAME_LENGTH); }

    /// Changes the socket read timeout without changing the write timeout.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<(), NetworkError> {
        self.stream.set_read_timeout(timeout)?;
        Ok(())
    }

    /// Enables compression for packets after the Set Compression packet has been received.
    pub fn set_compression(&mut self, threshold: usize) {
        self.compression = Some(PacketCompression::new(threshold));
    }

    pub fn read_packet(&mut self) -> Result<Vec<u8>, NetworkError> {
        let mut prefix = Vec::with_capacity(5);
        let length = loop {
            let mut b = [0u8; 1];
            self.stream.read_exact(&mut b)?;
            self.bytes_in = self.bytes_in.saturating_add(1);
            prefix.push(b[0]);
            if prefix.len() > 5 { return Err(ProtocolError::VarIntTooLong.into()); }
            if b[0] & 0x80 == 0 {
                let mut cursor = 0;
                break read_varint(&prefix, &mut cursor)? as i64;
            }
        };
        if length < 0 { return Err(ProtocolError::NegativeLength(length as i32).into()); }
        let length = length as usize;
        if length == 0 { return Err(ProtocolError::Truncated("empty packet frame").into()); }
        if length > self.max_frame_length { return Err(ProtocolError::FrameTooLarge(length).into()); }
        let mut payload = vec![0u8; length];
        self.stream.read_exact(&mut payload)?;
        self.bytes_in = self.bytes_in.saturating_add(length as u64);
        match self.compression {
            Some(compression) => Ok(compression.decode(&payload)?),
            None => Ok(payload),
        }
    }

    pub fn write_packet(&mut self, payload: &[u8]) -> Result<(), NetworkError> {
        if payload.is_empty() { return Err(ProtocolError::Truncated("empty packet payload").into()); }
        let framed_payload = match self.compression {
            Some(compression) => compression.encode(payload)?,
            None => payload.to_vec(),
        };
        if framed_payload.len() > self.max_frame_length {
            return Err(ProtocolError::FrameTooLarge(framed_payload.len()).into());
        }
        let mut prefix = Vec::with_capacity(5);
        write_varint(&mut prefix, framed_payload.len() as i32);
        self.stream.write_all(&prefix)?;
        self.stream.write_all(&framed_payload)?;
        self.stream.flush()?;
        self.bytes_out = self.bytes_out.saturating_add((prefix.len() + framed_payload.len()) as u64);
        Ok(())
    }

    pub fn counters(&self) -> (u64, u64) { (self.bytes_in, self.bytes_out) }
    pub fn shutdown(&self) -> Result<(), NetworkError> {
        self.stream.shutdown(std::net::Shutdown::Both)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn tcp_packet_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut conn = MinecraftConnection::from_stream(stream).unwrap();
            assert_eq!(conn.read_packet().unwrap(), vec![0x01, 0x2a]);
            conn.write_packet(&[0x02, 0x7f]).unwrap();
        });
        let stream = TcpStream::connect(address).unwrap();
        let mut client = MinecraftConnection::from_stream(stream).unwrap();
        client.write_packet(&[0x01, 0x2a]).unwrap();
        assert_eq!(client.read_packet().unwrap(), vec![0x02, 0x7f]);
        server.join().unwrap();
    }

    #[test]
    fn compressed_packet_round_trip_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut conn = MinecraftConnection::from_stream(stream).unwrap();
            conn.set_compression(8);
            conn.write_packet(&[0x02; 512]).unwrap();
        });
        let stream = TcpStream::connect(address).unwrap();
        let mut client = MinecraftConnection::from_stream(stream).unwrap();
        client.set_compression(8);
        assert_eq!(client.read_packet().unwrap(), vec![0x02; 512]);
        server.join().unwrap();
    }
}
