//! Minimal Minecraft Java 1.21.8 offline-mode login and configuration handshake.
//! This deliberately does not implement Microsoft account authentication or world rendering.
use std::time::Duration;

use md5::{Digest, Md5};

use crate::handshake::{self, NextState};
use crate::net::MinecraftConnection;
use crate::protocol::{read_string, read_varint, write_string, write_varint};

fn offline_uuid(username: &str) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

fn packet_id(packet: &[u8]) -> Result<(i32, usize), String> {
    let mut cursor = 0;
    let id = read_varint(packet, &mut cursor).map_err(|e| e.to_string())?;
    Ok((id, cursor))
}

fn read_text(packet: &[u8], cursor: &mut usize) -> Result<String, String> {
    read_string(packet, cursor, 32_767).map_err(|e| e.to_string())
}

fn disconnect_reason(packet: &[u8], cursor: usize) -> String {
    let raw = &packet[cursor..];
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) {
        if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
            return text.to_owned();
        }
        return value.to_string();
    }
    String::from_utf8_lossy(raw).into_owned()
}

fn login_success_username(packet: &[u8], mut cursor: usize) -> Result<String, String> {
    if packet.len() < cursor + 16 { return Err("Login Success packet is missing UUID".into()); }
    cursor += 16; // UUID is encoded as 16 raw bytes.
    let username = read_text(packet, &mut cursor)?;
    let properties = read_varint(packet, &mut cursor).map_err(|e| e.to_string())?;
    if !(0..=1024).contains(&properties) { return Err(format!("invalid login property count: {properties}")); }
    for _ in 0..properties {
        let _name = read_text(packet, &mut cursor)?;
        let _value = read_text(packet, &mut cursor)?;
        let has_signature = *packet.get(cursor).ok_or("truncated login property signature flag")?;
        cursor += 1;
        if has_signature != 0 { let _signature = read_text(packet, &mut cursor)?; }
    }
    Ok(username)
}

fn send_client_information(conn: &mut MinecraftConnection) -> Result<(), String> {
    let mut p = Vec::with_capacity(32);
    write_varint(&mut p, 0x00); // Client Information (Configuration, serverbound)
    write_string(&mut p, "en_us").map_err(|e| e.to_string())?;
    p.push(8); // view distance
    write_varint(&mut p, 0); // chat mode: enabled
    p.push(1); // chat colors
    p.push(0x7f); // all skin parts
    write_varint(&mut p, 1); // main hand: right
    p.push(0); // text filtering disabled
    p.push(1); // allow server listings
    write_varint(&mut p, 0); // particle status: all
    conn.write_packet(&p).map_err(|e| e.to_string())
}

/// Processes the Java 1.21.8 (protocol 772) Play state until the server disconnects.
fn play_loop(conn: &mut MinecraftConnection) -> Result<(), String> {
    const CLIENTBOUND_DISCONNECT: i32 = 0x1c;
    const CLIENTBOUND_KEEP_ALIVE: i32 = 0x26;
    const CLIENTBOUND_PING: i32 = 0x36;
    const SERVERBOUND_KEEP_ALIVE: i32 = 0x1b;
    const SERVERBOUND_PONG: i32 = 0x2c;

    conn.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    println!("Play session active. Waiting for server packets (Ctrl+C to stop).");
    let mut received = 0u64;
    let mut keep_alives = 0u64;
    let mut unknown_ids = std::collections::BTreeSet::new();
    loop {
        let packet = conn.read_packet().map_err(|e| format!("Play state read failed after {received} packets: {e}"))?;
        let (id, cursor) = packet_id(&packet)?;
        received = received.saturating_add(1);
        match id {
            CLIENTBOUND_DISCONNECT => {
                println!("Server disconnected: {}", disconnect_reason(&packet, cursor));
                println!("Play session ended after {received} packets; keep-alive replies: {keep_alives}.");
                return Ok(());
            }
            CLIENTBOUND_KEEP_ALIVE => {
                if packet.len() != cursor + 8 { return Err(format!("invalid Play Keep Alive length: expected 8 bytes, got {}", packet.len().saturating_sub(cursor))); }
                let mut response = Vec::with_capacity(9);
                write_varint(&mut response, SERVERBOUND_KEEP_ALIVE);
                response.extend_from_slice(&packet[cursor..cursor + 8]);
                conn.write_packet(&response).map_err(|e| e.to_string())?;
                keep_alives = keep_alives.saturating_add(1);
                println!("Keep-alive #{keep_alives} acknowledged.");
            }
            CLIENTBOUND_PING => {
                if packet.len() != cursor + 4 { return Err(format!("invalid Play Ping length: expected 4 bytes, got {}", packet.len().saturating_sub(cursor))); }
                let mut response = Vec::with_capacity(5);
                write_varint(&mut response, SERVERBOUND_PONG);
                response.extend_from_slice(&packet[cursor..cursor + 4]);
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            other => if unknown_ids.insert(other) {
                println!("Play packet 0x{other:02x} received ({} payload bytes; decoder pending).", packet.len().saturating_sub(cursor));
            }
        }
    }
}

/// Connects using offline-mode identity, negotiates login compression and processes
/// configuration packets until the server finishes configuration or rejects the client.
pub fn connect_offline(host: &str, port: u16, username: &str) -> Result<(), String> {
    if username.is_empty() || username.len() > 16 || !username.is_ascii() {
        return Err("username must be 1..=16 ASCII bytes".into());
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("username may contain only ASCII letters, digits, and underscores".into());
    }
    let address = format!("{host}:{port}");
    println!("Connecting to {address} as {username} (offline mode; no account authentication)...");
    let mut conn = MinecraftConnection::connect(&address, Duration::from_secs(12))
        .map_err(|e| e.to_string())?;
    conn.write_packet(&handshake::handshake(host, port, NextState::Login).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    conn.write_packet(&handshake::login_start(username, offline_uuid(username)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let mut accepted_name = None;
    for _ in 0..128 {
        let packet = conn.read_packet().map_err(|e| format!("login read failed: {e}"))?;
        let (id, cursor) = packet_id(&packet)?;
        match id {
            0x00 => return Err(format!("server disconnected during login: {}", disconnect_reason(&packet, cursor))),
            0x01 => return Err("server requires online-mode encryption/authentication; this build supports offline-mode servers only".into()),
            0x02 => {
                let name = login_success_username(&packet, cursor)?;
                println!("Login accepted for {name}; entering Configuration state.");
                accepted_name = Some(name);
                let mut ack = Vec::with_capacity(1);
                write_varint(&mut ack, 0x03); // Login Acknowledged
                conn.write_packet(&ack).map_err(|e| e.to_string())?;
                send_client_information(&mut conn)?;
                break;
            }
            0x03 => {
                let mut c = cursor;
                let threshold = read_varint(&packet, &mut c).map_err(|e| e.to_string())?;
                if threshold < 0 { return Err(format!("server sent invalid compression threshold {threshold}")); }
                conn.set_compression(threshold as usize);
                println!("Negotiated packet compression threshold: {threshold} bytes");
            }
            0x04 => {
                let mut response = Vec::new();
                write_varint(&mut response, 0x02); // Login Plugin Response
                let mut c = cursor;
                let message_id = read_varint(&packet, &mut c).map_err(|e| e.to_string())?;
                write_varint(&mut response, message_id);
                response.push(0); // no plugin response payload
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            0x05 => {
                let mut c = cursor;
                let key = read_text(&packet, &mut c)?;
                let mut response = Vec::new();
                write_varint(&mut response, 0x04); // Login Cookie Response
                write_string(&mut response, &key).map_err(|e| e.to_string())?;
                response.push(0); // no cookie payload
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            other => return Err(format!("unexpected login packet ID 0x{other:02x}")),
        }
        if accepted_name.is_some() { break; }
    }
    let name = accepted_name.ok_or("login did not complete within 128 packets")?;
    println!("Configuration negotiation started for {name}.");

    for _ in 0..4096 {
        let packet = conn.read_packet().map_err(|e| format!("configuration read failed: {e}"))?;
        let (id, cursor) = packet_id(&packet)?;
        match id {
            0x00 => { // Cookie Request
                let mut c = cursor;
                let key = read_text(&packet, &mut c)?;
                let mut response = Vec::new();
                write_varint(&mut response, 0x01);
                write_string(&mut response, &key).map_err(|e| e.to_string())?;
                response.push(0);
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            0x01 => { // Plugin Message: no custom channels are implemented.
                let mut c = cursor;
                let channel = read_text(&packet, &mut c).unwrap_or_else(|_| "unknown channel".into());
                println!("Ignoring configuration plugin message on {channel}.");
            }
            0x02 => return Err(format!("server disconnected during configuration: {}", disconnect_reason(&packet, cursor))),
            0x03 => { // Finish Configuration
                let mut ack = Vec::new();
                write_varint(&mut ack, 0x03);
                conn.write_packet(&ack).map_err(|e| e.to_string())?;
                println!("Configuration finished. Server accepted the client into Play state.");
                if std::env::var_os("ASTRACRAFT_LOGIN_PROBE").is_some() {
                    println!("Probe mode: closing after successful Play-state transition.");
                    return Ok(());
                }
                return play_loop(&mut conn);
            }
            0x04 => { // Keep Alive: echo the 64-bit ID.
                if packet.len() < cursor + 8 { return Err("truncated configuration Keep Alive".into()); }
                let mut response = Vec::with_capacity(9);
                write_varint(&mut response, 0x04);
                response.extend_from_slice(&packet[cursor..cursor + 8]);
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            0x05 => { // Ping: echo its 32-bit ID as Pong.
                if packet.len() < cursor + 4 { return Err("truncated configuration Ping".into()); }
                let mut response = Vec::with_capacity(5);
                write_varint(&mut response, 0x05);
                response.extend_from_slice(&packet[cursor..cursor + 4]);
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            0x0e => { // Select Known Packs (clientbound); select none until pack support exists.
                let mut c = cursor;
                let count = read_varint(&packet, &mut c).map_err(|e| e.to_string())?;
                if !(0..=256).contains(&count) { return Err(format!("invalid known-pack count {count}")); }
                for _ in 0..count {
                    let _namespace = read_text(&packet, &mut c)?;
                    let _id = read_text(&packet, &mut c)?;
                    let _version = read_text(&packet, &mut c)?;
                }
                let mut response = Vec::new();
                write_varint(&mut response, 0x07); // Select Known Packs (serverbound)
                write_varint(&mut response, 0); // select zero known packs
                conn.write_packet(&response).map_err(|e| e.to_string())?;
            }
            0x06 | 0x07 | 0x08 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d => {
                // Reset Chat, Registry Data, resource-pack notices, tags and feature flags.
                // These are currently consumed but not interpreted.
            }
            other => return Err(format!("unexpected configuration packet ID 0x{other:02x}")),
        }
    }
    Err("configuration did not finish within 4096 packets".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_is_stable_and_version_marked() {
        let a = offline_uuid("Steve");
        let b = offline_uuid("Steve");
        assert_eq!(a, b);
        assert_eq!(a[6] >> 4, 3);
        assert_eq!(a[8] >> 6, 2);
        assert_ne!(a, offline_uuid("Alex"));
    }

    #[test]
    fn login_success_parser_reads_username_and_properties() {
        let mut packet = vec![0x02];
        packet.extend_from_slice(&[0; 16]);
        write_string(&mut packet, "Steve").unwrap();
        write_varint(&mut packet, 0);
        assert_eq!(login_success_username(&packet, 1).unwrap(), "Steve");
    }
}
