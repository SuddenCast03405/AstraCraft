//! Protocol 772 Play-state packet decoders for initial world entry and player teleport.
use crate::protocol::{read_string, read_varint, write_varint};

#[derive(Debug, Clone, PartialEq)]
pub struct JoinGame {
    pub entity_id: i32,
    pub hardcore: bool,
    pub worlds: Vec<String>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub dimension_id: i32,
    pub dimension_name: String,
    pub hashed_seed: i64,
    pub game_mode: i8,
    pub previous_game_mode: u8,
    pub debug: bool,
    pub flat: bool,
    pub portal_cooldown: i32,
    pub sea_level: i32,
    pub secure_chat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerTeleport {
    pub teleport_id: i32,
    pub x: f64, pub y: f64, pub z: f64,
    pub yaw: f32, pub pitch: f32,
    pub relative_flags: u8,
}

fn take<const N: usize>(p: &[u8], c: &mut usize, what: &'static str) -> Result<[u8; N], String> {
    let end = c.checked_add(N).ok_or("packet cursor overflow")?;
    let bytes = p.get(*c..end).ok_or_else(|| format!("truncated {what}"))?;
    *c = end;
    Ok(bytes.try_into().expect("fixed-size slice"))
}
fn varint(p: &[u8], c: &mut usize) -> Result<i32, String> {
    read_varint(p, c).map_err(|e| e.to_string())
}
fn string(p: &[u8], c: &mut usize) -> Result<String, String> {
    read_string(p, c, 32767).map_err(|e| e.to_string())
}
fn boolean(p: &[u8], c: &mut usize, what: &'static str) -> Result<bool, String> {
    match take::<1>(p, c, what)?[0] { 0 => Ok(false), 1 => Ok(true), n => Err(format!("invalid {what} boolean {n}")) }
}

/// Decode clientbound Join Game (packet 0x2b, protocol 772).
pub fn decode_join_game(p: &[u8]) -> Result<JoinGame, String> {
    let mut c = 0;
    let entity_id = i32::from_be_bytes(take(p, &mut c, "entity ID")?);
    let hardcore = boolean(p, &mut c, "hardcore")?;
    let count = varint(p, &mut c)?;
    if !(0..=1024).contains(&count) { return Err(format!("invalid world-name count {count}")); }
    let mut worlds = Vec::with_capacity(count as usize);
    for _ in 0..count { worlds.push(string(p, &mut c)?); }
    let max_players = varint(p, &mut c)?;
    let view_distance = varint(p, &mut c)?;
    let simulation_distance = varint(p, &mut c)?;
    let reduced_debug = boolean(p, &mut c, "reduced-debug-info")?;
    let _enable_respawn_screen = boolean(p, &mut c, "enable-respawn-screen")?;
    let _limited_crafting = boolean(p, &mut c, "limited-crafting")?;
    let dimension_id = varint(p, &mut c)?;
    let dimension_name = string(p, &mut c)?;
    let hashed_seed = i64::from_be_bytes(take(p, &mut c, "hashed seed")?);
    let game_mode = take::<1>(p, &mut c, "game mode")?[0] as i8;
    let previous_game_mode = take::<1>(p, &mut c, "previous game mode")?[0];
    let debug = boolean(p, &mut c, "debug")?;
    let flat = boolean(p, &mut c, "flat")?;
    let has_death_location = boolean(p, &mut c, "death-location presence")?;
    if has_death_location {
        let _death_dimension = string(p, &mut c)?;
        let _packed_position = take::<8>(p, &mut c, "death location")?;
    }
    let portal_cooldown = varint(p, &mut c)?;
    let sea_level = varint(p, &mut c)?;
    let secure_chat = boolean(p, &mut c, "secure-chat")?;
    if c != p.len() { return Err(format!("Join Game has {} trailing bytes", p.len() - c)); }
    let _ = reduced_debug;
    Ok(JoinGame { entity_id, hardcore, worlds, max_players, view_distance, simulation_distance,
        dimension_id, dimension_name, hashed_seed, game_mode, previous_game_mode, debug, flat,
        portal_cooldown, sea_level, secure_chat })
}

/// Decode clientbound Synchronize Player Position (packet 0x41).
pub fn decode_player_teleport(p: &[u8]) -> Result<PlayerTeleport, String> {
    let mut c = 0;
    let teleport_id = varint(p, &mut c)?;
    let x = f64::from_be_bytes(take(p, &mut c, "teleport X")?);
    let y = f64::from_be_bytes(take(p, &mut c, "teleport Y")?);
    let z = f64::from_be_bytes(take(p, &mut c, "teleport Z")?);
    let _dx = take::<8>(p, &mut c, "teleport delta X")?;
    let _dy = take::<8>(p, &mut c, "teleport delta Y")?;
    let _dz = take::<8>(p, &mut c, "teleport delta Z")?;
    let yaw = f32::from_be_bytes(take(p, &mut c, "teleport yaw")?);
    let pitch = f32::from_be_bytes(take(p, &mut c, "teleport pitch")?);
    let relative_flags = take::<1>(p, &mut c, "relative flags")?[0];
    if c != p.len() { return Err(format!("Player Position has {} trailing bytes", p.len() - c)); }
    if !x.is_finite() || !y.is_finite() || !z.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
        return Err("Player Position contains non-finite coordinates or angles".into());
    }
    Ok(PlayerTeleport { teleport_id, x, y, z, yaw, pitch, relative_flags })
}

pub fn teleport_confirm_packet(teleport_id: i32) -> Vec<u8> {
    let mut p = Vec::with_capacity(6);
    write_varint(&mut p, 0x00); // Serverbound Teleport Confirm
    write_varint(&mut p, teleport_id);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{write_string, write_varint};

    #[test]
    fn decodes_protocol_772_join_game() {
        let mut p = Vec::new();
        p.extend_from_slice(&42i32.to_be_bytes());
        p.extend_from_slice(&[1]);
        write_varint(&mut p, 2);
        write_string(&mut p, "minecraft:overworld").unwrap();
        write_string(&mut p, "minecraft:the_nether").unwrap();
        for n in [20, 10, 8] { write_varint(&mut p, n); }
        p.extend_from_slice(&[0, 1, 0]);
        write_varint(&mut p, 0);
        write_string(&mut p, "minecraft:overworld").unwrap();
        p.extend_from_slice(&123i64.to_be_bytes());
        p.extend_from_slice(&[0, 255, 0, 1, 0]);
        write_varint(&mut p, 0);
        write_varint(&mut p, 63);
        p.push(1);
        let got = decode_join_game(&p).unwrap();
        assert_eq!(got.entity_id, 42);
        assert!(got.hardcore);
        assert_eq!(got.worlds.len(), 2);
        assert_eq!(got.dimension_name, "minecraft:overworld");
        assert_eq!(got.game_mode, 0);
        assert!(got.flat);
        assert!(got.secure_chat);
    }

    #[test]
    fn teleport_confirm_uses_serverbound_packet_zero() {
        assert_eq!(teleport_confirm_packet(300), vec![0, 0xac, 0x02]);
    }

    #[test]
    fn rejects_non_finite_teleport() {
        let mut p = Vec::new();
        write_varint(&mut p, 1);
        p.extend_from_slice(&f64::NAN.to_be_bytes());
        p.extend_from_slice(&[0; 48]);
        p.extend_from_slice(&[0; 5]);
        assert!(decode_player_teleport(&p).is_err());
    }
}
