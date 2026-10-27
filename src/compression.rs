//! Minecraft packet compression layer (zlib, negotiated per connection).
//! Input/output are packet payloads after the outer TCP frame prefix.
use std::io::{Read, Write};

use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};

use crate::protocol::{read_varint, write_varint, ProtocolError, MAX_FRAME_LENGTH};

#[derive(Debug)]
pub enum CompressionError {
    Protocol(ProtocolError),
    Zlib(std::io::Error),
    InvalidDataLength(i32),
    ThresholdViolation { declared: usize, threshold: usize },
    DecompressedTooLarge(usize),
    LengthMismatch { declared: usize, actual: usize },
}
impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "compression packet framing: {e}"),
            Self::Zlib(e) => write!(f, "zlib error: {e}"),
            Self::InvalidDataLength(n) => write!(f, "invalid uncompressed data length: {n}"),
            Self::ThresholdViolation { declared, threshold } =>
                write!(f, "compressed packet length {declared} is below threshold {threshold}"),
            Self::DecompressedTooLarge(n) => write!(f, "decompressed packet exceeds limit: {n}"),
            Self::LengthMismatch { declared, actual } =>
                write!(f, "decompressed length mismatch: declared {declared}, got {actual}"),
        }
    }
}
impl std::error::Error for CompressionError {}
impl From<ProtocolError> for CompressionError {
    fn from(e: ProtocolError) -> Self { Self::Protocol(e) }
}
impl From<std::io::Error> for CompressionError {
    fn from(e: std::io::Error) -> Self { Self::Zlib(e) }
}

#[derive(Debug, Clone, Copy)]
pub struct PacketCompression {
    threshold: usize,
    max_packet_length: usize,
    level: u32,
}
impl PacketCompression {
    pub fn new(threshold: usize) -> Self {
        Self { threshold, max_packet_length: MAX_FRAME_LENGTH, level: 3 }
    }
    pub fn with_limits(mut self, max_packet_length: usize, level: u32) -> Self {
        self.max_packet_length = max_packet_length.min(MAX_FRAME_LENGTH).max(1);
        self.level = level.min(9);
        self
    }
    pub fn threshold(&self) -> usize { self.threshold }

    /// Encodes the inner compression envelope; the TCP frame prefix is added by net.rs.
    pub fn encode(&self, packet: &[u8]) -> Result<Vec<u8>, CompressionError> {
        if packet.len() > self.max_packet_length {
            return Err(CompressionError::DecompressedTooLarge(packet.len()));
        }
        let mut out = Vec::with_capacity(packet.len().saturating_add(8));
        if packet.len() < self.threshold {
            write_varint(&mut out, 0);
            out.extend_from_slice(packet);
        } else {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(self.level));
            encoder.write_all(packet)?;
            let compressed = encoder.finish()?;
            write_varint(&mut out, packet.len() as i32);
            out.extend_from_slice(&compressed);
        }
        Ok(out)
    }

    /// Decodes the inner compression envelope with a hard decompressed-size cap.
    pub fn decode(&self, envelope: &[u8]) -> Result<Vec<u8>, CompressionError> {
        let mut cursor = 0;
        let declared = read_varint(envelope, &mut cursor)?;
        if declared < 0 { return Err(CompressionError::InvalidDataLength(declared)); }
        if declared == 0 {
            let raw = &envelope[cursor..];
            if raw.len() > self.max_packet_length {
                return Err(CompressionError::DecompressedTooLarge(raw.len()));
            }
            if raw.len() >= self.threshold && self.threshold > 0 {
                return Err(CompressionError::ThresholdViolation {
                    declared: raw.len(), threshold: self.threshold,
                });
            }
            return Ok(raw.to_vec());
        }

        let declared = declared as usize;
        if declared > self.max_packet_length {
            return Err(CompressionError::DecompressedTooLarge(declared));
        }
        if declared < self.threshold {
            return Err(CompressionError::ThresholdViolation { declared, threshold: self.threshold });
        }
        let compressed = &envelope[cursor..];
        let decoder = ZlibDecoder::new(compressed);
        let mut limited = decoder.take(self.max_packet_length as u64 + 1);
        let mut output = Vec::with_capacity(declared.min(64 * 1024));
        limited.read_to_end(&mut output)?;
        if output.len() > self.max_packet_length {
            return Err(CompressionError::DecompressedTooLarge(output.len()));
        }
        if output.len() != declared {
            return Err(CompressionError::LengthMismatch { declared, actual: output.len() });
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncompressed_below_threshold_round_trip() {
        let c = PacketCompression::new(64);
        let packet = [0x01, 0x02, 0x03];
        let envelope = c.encode(&packet).unwrap();
        assert_eq!(envelope[0], 0);
        assert_eq!(c.decode(&envelope).unwrap(), packet);
    }

    #[test]
    fn compressed_round_trip() {
        let c = PacketCompression::new(8);
        let packet = vec![0x42; 1024];
        let envelope = c.encode(&packet).unwrap();
        assert_ne!(envelope[0], 0);
        assert_eq!(c.decode(&envelope).unwrap(), packet);
    }

    #[test]
    fn rejects_declared_oversize_before_inflating() {
        let c = PacketCompression::new(0).with_limits(1024, 1);
        // VarInt 2048, with no compressed payload needed to test the cap.
        assert!(matches!(c.decode(&[0x80, 0x10]), Err(CompressionError::DecompressedTooLarge(2048))));
    }

    #[test]
    fn rejects_raw_payload_over_threshold() {
        let c = PacketCompression::new(2);
        assert!(matches!(c.decode(&[0, 1, 2]), Err(CompressionError::ThresholdViolation { .. })));
    }
}
