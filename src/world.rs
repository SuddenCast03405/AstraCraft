//! Bounded in-memory world model, independent of packet decoding and rendering.
use std::collections::{BTreeMap, HashMap};

pub const SECTION_EDGE: usize = 16;
pub const SECTION_VOLUME: usize = SECTION_EDGE * SECTION_EDGE * SECTION_EDGE;
pub const DEFAULT_MAX_LOADED_CHUNKS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimension {
    pub min_y: i32,
    pub height: u32,
}
impl Dimension {
    pub fn new(min_y: i32, height: u32) -> Result<Self, WorldError> {
        if height == 0 || height > 60_000_000 || height % 16 != 0 || min_y.rem_euclid(16) != 0 {
            return Err(WorldError::InvalidDimension { min_y, height });
        }
        let max_y = min_y.checked_add(height as i32).ok_or(WorldError::CoordinateOverflow)?;
        if max_y > 30_000_000 || min_y < -30_000_000 {
            return Err(WorldError::InvalidDimension { min_y, height });
        }
        Ok(Self { min_y, height })
    }
    pub fn max_y_exclusive(self) -> i32 { self.min_y + self.height as i32 }
    pub fn contains_y(self, y: i32) -> bool { (self.min_y..self.max_y_exclusive()).contains(&y) }
    pub fn min_section_y(self) -> i32 { self.min_y.div_euclid(16) }
    pub fn section_count(self) -> usize { self.height as usize / 16 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChunkPos { pub x: i32, pub z: i32 }
impl ChunkPos {
    pub fn from_block(x: i32, z: i32) -> Self {
        Self { x: x.div_euclid(16), z: z.div_euclid(16) }
    }
    pub fn local_x(block_x: i32) -> u8 { block_x.rem_euclid(16) as u8 }
    pub fn local_z(block_z: i32) -> u8 { block_z.rem_euclid(16) as u8 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPos { pub x: i32, pub y: i32, pub z: i32 }
impl BlockPos {
    pub fn chunk(self) -> ChunkPos { ChunkPos::from_block(self.x, self.z) }
    pub fn section_y(self) -> i32 { self.y.div_euclid(16) }
    pub fn local(self) -> [u8; 3] {
        [self.x.rem_euclid(16) as u8, self.y.rem_euclid(16) as u8, self.z.rem_euclid(16) as u8]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPose {
    pub x: f64, pub y: f64, pub z: f64,
    pub yaw: f32, pub pitch: f32,
    pub on_ground: bool,
}
impl Default for PlayerPose {
    fn default() -> Self { Self { x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0, on_ground: false } }
}
impl PlayerPose {
    pub fn block_position(self) -> Option<BlockPos> {
        fn floor_i32(v: f64) -> Option<i32> {
            if !v.is_finite() || v < i32::MIN as f64 || v > i32::MAX as f64 { None } else { Some(v.floor() as i32) }
        }
        Some(BlockPos { x: floor_i32(self.x)?, y: floor_i32(self.y)?, z: floor_i32(self.z)? })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    InvalidDimension { min_y: i32, height: u32 },
    CoordinateOverflow,
    InvalidLocalCoordinate { x: u8, y: u8, z: u8 },
    OutsideDimension(i32),
    ChunkCapacity { limit: usize },
}
impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimension { min_y, height } => write!(f, "invalid dimension range min_y={min_y}, height={height}"),
            Self::CoordinateOverflow => write!(f, "world coordinate overflow"),
            Self::InvalidLocalCoordinate { x, y, z } => write!(f, "invalid local block coordinate ({x},{y},{z})"),
            Self::OutsideDimension(y) => write!(f, "block y={y} is outside the active dimension"),
            Self::ChunkCapacity { limit } => write!(f, "loaded chunk limit ({limit}) reached"),
        }
    }
}
impl std::error::Error for WorldError {}

#[derive(Debug, Clone)]
pub struct ChunkSection {
    /// Palette/global registry IDs are kept as opaque numeric state IDs until registries load.
    blocks: Box<[u32; SECTION_VOLUME]>,
    non_air: u16,
}
impl Default for ChunkSection {
    fn default() -> Self { Self { blocks: Box::new([0; SECTION_VOLUME]), non_air: 0 } }
}
impl ChunkSection {
    pub fn get(&self, x: u8, y: u8, z: u8) -> Result<u32, WorldError> {
        let index = block_index(x, y, z)?;
        Ok(self.blocks[index])
    }
    pub fn set(&mut self, x: u8, y: u8, z: u8, state: u32) -> Result<u32, WorldError> {
        let index = block_index(x, y, z)?;
        let old = self.blocks[index];
        if old == 0 && state != 0 { self.non_air = self.non_air.saturating_add(1); }
        if old != 0 && state == 0 { self.non_air = self.non_air.saturating_sub(1); }
        self.blocks[index] = state;
        Ok(old)
    }
    pub fn non_air_count(&self) -> u16 { self.non_air }
    pub fn is_empty(&self) -> bool { self.non_air == 0 }
}
fn block_index(x: u8, y: u8, z: u8) -> Result<usize, WorldError> {
    if x >= 16 || y >= 16 || z >= 16 {
        return Err(WorldError::InvalidLocalCoordinate { x, y, z });
    }
    Ok((y as usize * 16 + z as usize) * 16 + x as usize)
}

#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub pos: ChunkPos,
    sections: BTreeMap<i32, ChunkSection>,
}
impl Chunk {
    pub fn new(pos: ChunkPos) -> Self { Self { pos, sections: BTreeMap::new() } }
    pub fn section(&self, section_y: i32) -> Option<&ChunkSection> { self.sections.get(&section_y) }
    pub fn section_mut(&mut self, section_y: i32) -> &mut ChunkSection {
        self.sections.entry(section_y).or_default()
    }
    pub fn sections(&self) -> impl Iterator<Item = (i32, &ChunkSection)> {
        self.sections.iter().map(|(y, section)| (*y, section))
    }
    pub fn set_block(&mut self, pos: BlockPos, state: u32) -> Result<u32, WorldError> {
        if pos.chunk() != self.pos { return Err(WorldError::InvalidLocalCoordinate { x: pos.local()[0], y: pos.local()[1], z: pos.local()[2] }); }
        let [x, y, z] = pos.local();
        Ok(self.section_mut(pos.section_y()).set(x, y, z, state)?)
    }
    pub fn block(&self, pos: BlockPos) -> Result<u32, WorldError> {
        if pos.chunk() != self.pos { return Err(WorldError::InvalidLocalCoordinate { x: pos.local()[0], y: pos.local()[1], z: pos.local()[2] }); }
        let [x, y, z] = pos.local();
        match self.section(pos.section_y()) { Some(section) => section.get(x, y, z), None => Ok(0) }
    }
}

#[derive(Debug)]
pub struct World {
    pub dimension: Dimension,
    pub player: PlayerPose,
    chunks: HashMap<ChunkPos, Chunk>,
    max_loaded_chunks: usize,
}
impl World {
    pub fn new(dimension: Dimension) -> Self {
        Self::with_chunk_limit(dimension, DEFAULT_MAX_LOADED_CHUNKS)
    }
    pub fn with_chunk_limit(dimension: Dimension, max_loaded_chunks: usize) -> Self {
        Self { dimension, player: PlayerPose::default(), chunks: HashMap::new(), max_loaded_chunks }
    }
    pub fn loaded_chunk_count(&self) -> usize { self.chunks.len() }
    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> { self.chunks.get(&pos) }
    pub fn insert_chunk(&mut self, chunk: Chunk) -> Result<Option<Chunk>, WorldError> {
        if !self.chunks.contains_key(&chunk.pos) && self.chunks.len() >= self.max_loaded_chunks {
            return Err(WorldError::ChunkCapacity { limit: self.max_loaded_chunks });
        }
        Ok(self.chunks.insert(chunk.pos, chunk))
    }
    pub fn unload_chunk(&mut self, pos: ChunkPos) -> Option<Chunk> { self.chunks.remove(&pos) }
    pub fn set_block(&mut self, pos: BlockPos, state: u32) -> Result<u32, WorldError> {
        if !self.dimension.contains_y(pos.y) { return Err(WorldError::OutsideDimension(pos.y)); }
        let chunk_pos = pos.chunk();
        if !self.chunks.contains_key(&chunk_pos) {
            if self.chunks.len() >= self.max_loaded_chunks {
                return Err(WorldError::ChunkCapacity { limit: self.max_loaded_chunks });
            }
            self.chunks.insert(chunk_pos, Chunk::new(chunk_pos));
        }
        self.chunks.get_mut(&chunk_pos).expect("inserted above").set_block(pos, state)
    }
    pub fn block(&self, pos: BlockPos) -> Result<u32, WorldError> {
        if !self.dimension.contains_y(pos.y) { return Err(WorldError::OutsideDimension(pos.y)); }
        match self.chunks.get(&pos.chunk()) { Some(chunk) => chunk.block(pos), None => Ok(0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World { World::new(Dimension::new(-64, 384).unwrap()) }

    #[test]
    fn negative_block_coordinates_use_euclidean_chunk_math() {
        assert_eq!(ChunkPos::from_block(-1, -17), ChunkPos { x: -1, z: -2 });
        assert_eq!(ChunkPos::local_x(-1), 15);
        assert_eq!(ChunkPos::local_z(-17), 15);
    }
    #[test]
    fn block_storage_round_trips_and_tracks_non_air() {
        let mut section = ChunkSection::default();
        assert!(section.is_empty());
        assert_eq!(section.set(15, 15, 15, 42).unwrap(), 0);
        assert_eq!(section.get(15, 15, 15).unwrap(), 42);
        assert_eq!(section.non_air_count(), 1);
        section.set(15, 15, 15, 0).unwrap();
        assert!(section.is_empty());
    }
    #[test]
    fn world_updates_cross_negative_chunk_and_section_boundaries() {
        let mut w = world();
        let pos = BlockPos { x: -1, y: -1, z: -17 };
        assert_eq!(w.set_block(pos, 123).unwrap(), 0);
        assert_eq!(w.block(pos).unwrap(), 123);
        assert!(w.chunk(ChunkPos { x: -1, z: -2 }).is_some());
    }
    #[test]
    fn world_rejects_out_of_dimension_and_chunk_overflow() {
        let mut w = World::with_chunk_limit(Dimension::new(0, 16).unwrap(), 1);
        assert!(matches!(w.set_block(BlockPos { x: 0, y: 16, z: 0 }, 1), Err(WorldError::OutsideDimension(16))));
        w.set_block(BlockPos { x: 0, y: 0, z: 0 }, 1).unwrap();
        assert!(matches!(w.set_block(BlockPos { x: 16, y: 0, z: 0 }, 1), Err(WorldError::ChunkCapacity { limit: 1 })));
    }
    #[test]
    fn pose_rejects_non_finite_and_floors_negative_values() {
        let pose = PlayerPose { x: -0.1, y: 64.9, z: 15.0, ..PlayerPose::default() };
        assert_eq!(pose.block_position(), Some(BlockPos { x: -1, y: 64, z: 15 }));
        assert!(PlayerPose { x: f64::NAN, ..pose }.block_position().is_none());
    }
}
