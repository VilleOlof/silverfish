//! `chunk` contains the [`ChunkData`] struct and it's core impls.  
//! [`ChunkData`] is a wrapper for the actual chunk nbt and some attached data
//! to keep track of pending blocks and biomes and what blocks/biomes we've seen before.  

use crate::{BiomeCell, Block, BlockWithCoordinate, NbtString, Result, biome::BiomeCellWithId};
use ahash::AHashMap;
use fixedbitset::FixedBitSet;
use simdnbt::owned::NbtCompound;
use std::{ops::RangeInclusive, sync::Arc};

/// A chunk within a region and it's attached data to track pending blocks.  
///
/// Provides some lower level set functions that [`Region`](crate::Region) uses.  
#[derive(Clone)]
pub struct ChunkData {
    /// The chunks actual NBT data  
    ///
    /// This is an [`Arc`] so it can be safetely read in threaded contexts.  
    /// But when it attempts to take ownership in certain functions it will throw
    /// an [`TriedToAccessArc`](crate::Error::TriedToAccessArc) error if theres any other strong references.  
    pub nbt: Arc<NbtCompound>,
    /// The world height, we keep a range copy here since we need it
    /// for Bitset and index calculations
    pub(crate) world_height: RangeInclusive<isize>,

    /// The blocks that have been written but not pushed to the NBT
    pub(crate) pending_blocks: AHashMap<i8, Vec<BlockWithCoordinate>>,
    /// A check of what blocks has been seen before in `pending_blocks`
    /// This is to avoid duplicate coordinate and makes stuff faster.  
    pub(crate) seen_blocks: FixedBitSet,

    /// The biomes that have been written but not pushed to the NBT
    pub(crate) pending_biomes: AHashMap<i8, Vec<BiomeCellWithId>>,
    /// A check of what blocks has been seen before in `pending_biomes`
    pub(crate) seen_biomes: FixedBitSet,

    /// If this is unmarked, the block write logic will skip this one.  
    pub(crate) dirty_blocks: bool,
    /// If this is unmarked, the biome write logic will skip this one.  
    pub(crate) dirty_biomes: bool,
}

impl ChunkData {
    /// How many blocks wide a chunk is.  
    ///
    /// Also how wide/tall a single section is.  
    pub(crate) const WIDTH: usize = 16;

    /// Set a block at the specified coordinates *(local to within the chunk)*.  
    ///
    /// In most cases you'd want to use [`Region::set_block`](crate::Region::set_block) instead since that picks  
    /// the right chunk and handles it for you.  
    pub fn set_block<B: Into<Block>>(
        &mut self,
        x: u32,
        y: i32,
        z: u32,
        block: B,
    ) -> Result<Option<()>> {
        assert!(x < ChunkData::WIDTH as u32 && z < ChunkData::WIDTH as u32);

        let index = self.get_block_index(x, y, z);
        if !self.seen_blocks.contains(index) {
            self.seen_blocks.insert(index);

            let section_y = (y as f64 / ChunkData::WIDTH as f64).floor() as i8;

            self.pending_blocks
                .entry(section_y)
                .or_insert_with(|| Vec::with_capacity(ChunkData::WIDTH.pow(3)))
                .push(BlockWithCoordinate {
                    coordinates: (x, y, z),
                    block: block.into(),
                });
            self.dirty_blocks = true;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Set a biome at the specified cell.  
    ///
    /// In most cases you'd want to use [`Region::set_biome`](crate::Region::set_biome) instead since that picks  
    /// the right chunk and handles it for you.  
    ///
    /// But if you have a [`ChunkData`] and know that these coordinates are within
    /// this specific chunk then go ahead and use this.  
    ///
    /// But be careful.  
    pub fn set_biome<C: Into<BiomeCell>, B: Into<NbtString>>(
        &mut self,
        cell: C,
        biome: B,
    ) -> Result<Option<()>> {
        let cell: BiomeCell = cell.into();
        let biome: NbtString = biome.into();

        let index = self.get_biome_index(&cell);
        if !self.seen_biomes.contains(index) {
            self.seen_biomes.insert(index);

            self.pending_biomes
                .entry(cell.section)
                .or_insert_with(|| Vec::with_capacity((BiomeCell::CELL_SIZE.pow(3)) as usize))
                .push(BiomeCellWithId { cell, id: biome });
            self.dirty_biomes = true;

            return Ok(Some(()));
        }

        Ok(None)
    }

    /// Returns the [`FixedBitSet`] index for these coordinates.  
    pub(crate) fn get_block_index(&self, x: u32, y: i32, z: u32) -> usize {
        let y_offset = (y as isize - self.world_height.start()) as usize;
        x as usize
            + y_offset * ChunkData::WIDTH
            + z as usize
                * ChunkData::WIDTH
                * (self.world_height.end() - -self.world_height.start()) as usize
    }

    /// Returns the index for a biome in the [`Self::seen_biomes`] bitset based of it's cell coordinates  
    pub(crate) fn get_biome_index(&self, cell: &BiomeCell) -> usize {
        let cell_size = BiomeCell::CELL_SIZE as usize;
        let (bx, by, bz) = (
            cell.cell.0 as usize,
            cell.cell.1 as usize,
            cell.cell.2 as usize,
        );

        (cell.section - (*self.world_height.start() / ChunkData::WIDTH as isize) as i8) as usize
            * cell_size
            * cell_size
            * cell_size
            + bx
            + bz * cell_size
            + by * cell_size * cell_size
    }

    /// Returns the [`FixedBitSet`] for seen_biomes
    pub(crate) fn biome_bitset(world_height: usize) -> FixedBitSet {
        let section_count = world_height / ChunkData::WIDTH;
        let size = section_count * (BiomeCell::CELL_SIZE.pow(3)) as usize;
        FixedBitSet::with_capacity(size)
    }

    /// Returns the [`FixedBitSet`] for seen_blocks
    pub(crate) fn block_bitset(world_height: usize) -> FixedBitSet {
        let size = ChunkData::WIDTH * world_height * ChunkData::WIDTH;
        FixedBitSet::with_capacity(size)
    }

    /// Sets the internal block buffer.
    ///
    /// Overwrites any and all data related to the buffer.
    pub fn set_internal_block_buffer(&mut self, buffer: AHashMap<i8, Vec<BlockWithCoordinate>>) {
        self.pending_blocks = buffer;
        self.seen_blocks.clear();
    }

    /// Sets the internal biome buffer.
    ///
    /// Overwrites any and all data related to the buffer.
    pub fn set_internal_biome_buffer(&mut self, buffer: AHashMap<i8, Vec<BiomeCellWithId>>) {
        self.pending_biomes = buffer;
        self.seen_biomes.clear();
    }

    /// Creates a new [`ChunkData`] with empty and cleared buffers.  
    pub fn new(nbt: NbtCompound, world_height: RangeInclusive<isize>) -> ChunkData {
        let world_height_count = world_height.clone().count();
        ChunkData {
            nbt: Arc::new(nbt),
            world_height: world_height.clone(),
            pending_blocks: AHashMap::new(),
            pending_biomes: AHashMap::new(),
            seen_blocks: ChunkData::block_bitset(world_height_count),
            seen_biomes: ChunkData::biome_bitset(world_height_count),
            dirty_blocks: false,
            dirty_biomes: false,
        }
    }
}
