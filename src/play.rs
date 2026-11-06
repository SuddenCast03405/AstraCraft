//! Protocol 772 Play-state packet decoders for initial world entry and player teleport.
use crate::protocol::{read_string, read_varint, write_varint};

use crate::world::{Chunk, ChunkPos, Dimension, SECTION_VOLUME};

const MAX_CHUNK_DATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_PALETTE_ENTRIES: usize = 4096;

#[derive(Debug, Clone)]
pub struct DecodedChunk {
    pub chunk: Chunk,
    /// Number of bytes consumed from the packet payload (remaining bytes contain block entities/light).
    pub consumed: usize,
    pub section_count: usize,
}

fn take_bytes<'a>(p: &'a [u8], c: &mut usize, n: usize, what: &'static str) -> Result<&'a [u8], String> {
    let end = c.checked_add(n).ok_or("chunk cursor overflow")?;
    let bytes = p.get(*c..end).ok_or_else(|| format!("truncated {what}"))?;
    *c = end;
    Ok(bytes)
}

fn decode_paletted_values(p: &[u8], c: &mut usize, entries: usize, indirect_max_bits: u8) -> Result<Vec<u32>, String> {
    let bits = take::<1>(p, c, "palette bits")?[0];
    if bits > 32 { return Err(format!("unsupported palette bit width {bits}")); }
    let palette = if bits == 0 {
        vec![varint(p, c)? as u32]
    } else if bits <= indirect_max_bits {
        let count = varint(p, c)?;
        if count <= 0 || count as usize > MAX_PALETTE_ENTRIES {
            return Err(format!("invalid palette length {count}"));
        }
        let mut values = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let value = varint(p, c)?;
            if value < 0 { return Err("negative palette entry".into()); }
            values.push(value as u32);
        }
        values
    } else {
        Vec::new() // Direct/global palette: packed values are registry IDs.
    };

    let data_count = varint(p, c)?;
    if data_count < 0 { return Err(format!("negative packed-long count {data_count}")); }
    let data_count = data_count as usize;
    let expected = if bits == 0 { 0 } else {
        let per_long = 64usize / bits as usize;
        entries.div_ceil(per_long)
    };
    if data_count != expected {
        return Err(format!("packed-long count mismatch: got {data_count}, expected {expected}"));
    }
    if data_count > 4096 { return Err(format!("packed-long array too large: {data_count}")); }
    let raw = take_bytes(p, c, data_count.checked_mul(8).ok_or("packed-long size overflow")?, "packed-long array")?;
    let longs: Vec<u64> = raw.chunks_exact(8)
        .map(|b| u64::from_be_bytes(b.try_into().expect("eight-byte chunk")))
        .collect();

    let mut decoded = Vec::with_capacity(entries);
    if bits == 0 {
        decoded.resize(entries, *palette.first().ok_or("missing single-value palette")?);
        return Ok(decoded);
    }
    let width = bits as usize;
    let per_long = 64 / width;
    let mask = if bits == 32 { u32::MAX as u64 } else { (1u64 << bits) - 1 };
    for index in 0..entries {
        let long = longs[index / per_long];
        let shift = (index % per_long) * width;
        let raw_value = ((long >> shift) & mask) as u32;
        let value = if palette.is_empty() {
            raw_value
        } else {
            *palette.get(raw_value as usize).ok_or_else(|| format!("palette index {raw_value} out of range ({} entries)", palette.len()))?
        };
        decoded.push(value);
    }
    Ok(decoded)
}

/// Decode the Chunk Data packet's chunk coordinates, heightmap NBT and section byte array.
/// Section count and vertical origin come from the active dimension registry entry.
pub fn decode_chunk_data(payload: &[u8], dimension: Dimension) -> Result<DecodedChunk, String> {
    let mut c = 0;
    let x = i32::from_be_bytes(take::<4>(payload, &mut c, "chunk X")?);
    let z = i32::from_be_bytes(take::<4>(payload, &mut c, "chunk Z")?);
    let (_heightmaps, nbt_len) = crate::nbt::read_root_prefix(&payload[c..]).map_err(|e| format!("invalid chunk heightmaps NBT: {e:?}"))?;
    c = c.checked_add(nbt_len).ok_or("chunk NBT cursor overflow")?;
    let data_len = varint(payload, &mut c)?;
    if data_len < 0 || data_len as usize > MAX_CHUNK_DATA_BYTES {
        return Err(format!("invalid chunk section-data length {data_len}"));
    }
    let section_bytes = take_bytes(payload, &mut c, data_len as usize, "chunk section data")?;
    let mut sc = 0;
    let mut chunk = Chunk::new(ChunkPos { x, z });
    for section_index in 0..dimension.section_count() {
        let non_air = i16::from_be_bytes(take::<2>(section_bytes, &mut sc, "section non-air count")?);
        if !(0..=4096).contains(&non_air) { return Err(format!("invalid section non-air count {non_air}")); }
        let blocks = decode_paletted_values(section_bytes, &mut sc, SECTION_VOLUME, 8)?;
        // Biome palette contains 4x4x4 values. It is validated and consumed, but not yet stored.
        let _biomes = decode_paletted_values(section_bytes, &mut sc, 64, 3)?;
        let section_y = dimension.min_section_y() + section_index as i32;
        let section = chunk.section_mut(section_y);
        for (index, state) in blocks.into_iter().enumerate() {
            let x = (index % 16) as u8;
            let z = ((index / 16) % 16) as u8;
            let y = (index / 256) as u8;
            section.set(x, y, z, state).map_err(|e| e.to_string())?;
        }
    }
    if sc != section_bytes.len() {
        return Err(format!("{} trailing bytes in chunk section data", section_bytes.len() - sc));
    }
    Ok(DecodedChunk { chunk, consumed: c, section_count: dimension.section_count() })
}


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
    fn decodes_single_value_chunk_section_and_heightmap_nbt() {
        let dimension = crate::world::Dimension::new(-64, 16).unwrap();
        let mut section = Vec::new();
        section.extend_from_slice(&1i16.to_be_bytes()); // one non-air block
        section.push(0); write_varint(&mut section, 5); write_varint(&mut section, 0); // block states
        section.push(0); write_varint(&mut section, 1); write_varint(&mut section, 0); // biomes
        let mut payload = Vec::new();
        payload.extend_from_slice(&(-2i32).to_be_bytes());
        payload.extend_from_slice(&3i32.to_be_bytes());
        payload.extend_from_slice(&[10, 0, 0, 0]); // empty root compound
        write_varint(&mut payload, section.len() as i32);
        payload.extend_from_slice(&section);
        let decoded = decode_chunk_data(&payload, dimension).unwrap();
        assert_eq!(decoded.chunk.pos, ChunkPos { x: -2, z: 3 });
        assert_eq!(decoded.section_count, 1);
        assert_eq!(decoded.chunk.block(crate::world::BlockPos { x: -32, y: -64, z: 48 }).unwrap(), 5);
        assert_eq!(decoded.consumed, payload.len());
    }

    #[test]
    fn rejects_malformed_chunk_palette_lengths() {
        let mut section = Vec::new();
        section.extend_from_slice(&0i16.to_be_bytes());
        section.push(4); write_varint(&mut section, 1); write_varint(&mut section, 0);
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0; 8]);
        payload.extend_from_slice(&[10, 0, 0, 0]);
        write_varint(&mut payload, section.len() as i32);
        payload.extend_from_slice(&section);
        assert!(decode_chunk_data(&payload, crate::world::Dimension::new(0, 16).unwrap()).is_err());
    }

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
