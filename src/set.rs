//! `set` handles all functions related to pushing blocks to the [`Region`]'s internal block buffer.  

use crate::{
    BLOCKS_PER_REGION, Block, Config, Region,
    region::{BlockBuffer, BlockWithCoordinate},
};
use ahash::AHashMap;
use fixedbitset::FixedBitSet;
use std::ops::{Range, RangeInclusive};

impl Region {
    /// The given bitset size for pending blocks
    pub(crate) fn bitset_size(world_height: &RangeInclusive<isize>) -> usize {
        BLOCKS_PER_REGION as usize
            * (world_height.end() - world_height.start()) as usize
            * BLOCKS_PER_REGION as usize
    }

    /// Set a block at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`crate::to_region_local`].  
    ///
    /// ----
    ///
    /// Returns [`None`] if a buffered block already exists at those coordinates.  
    ///
    /// **Note:** This doesn't actually set the block but writes it to an internal buffer.  
    ///
    /// To actually write the changes to the `chunks`, call [`Region::write_blocks`]
    ///
    /// ## Example
    /// ```no_run
    /// let _ = region.set_block(5, 97, 385, Block::new("dirt"));
    /// // and to actually write the changes to the NBT
    /// region.write_blocks()?;
    /// ```
    pub fn set_block<B: Into<Block>>(&mut self, x: u32, y: i32, z: u32, block: B) -> Option<()> {
        let index = self.get_block_index(x, y, z);
        if !self.seen_blocks.contains(index) {
            self.seen_blocks.insert(index);

            let (chunk_x, chunk_z) = ((x / 16) as u8, (z / 16) as u8);
            let section_y = (y as f64 / 16f64).floor() as i8;

            self.pending_blocks
                .entry((chunk_x, chunk_z))
                .or_default()
                .entry(section_y)
                .or_default()
                .push(BlockWithCoordinate {
                    coordinates: (x, y, z),
                    block: block.into(),
                });

            return Some(());
        }

        None
    }

    /// Due to how the internal buffer is grouped for batching later on.  
    /// You can only define `chunk` and `section` ranges and how many blocks within each section.  
    ///
    /// Overwrites the already existing internal block buffer.  
    ///
    /// Useful if you know which areas in your region that you'll modify.  
    ///
    /// ## Example
    /// ```no_run
    /// let mut region = Region::full_empty((0, 0));
    /// region.allocate_block_buffer(0..16, 4..8, 1..3, 1024);
    /// ```
    pub fn allocate_block_buffer(
        &mut self,
        chunks_x: Range<u8>,
        chunk_z: Range<u8>,
        sections: Range<i8>,
        blocks_per_section: usize,
    ) {
        let buffer = AHashMap::with_capacity(chunks_x.clone().count() * chunk_z.clone().count());

        for x in chunks_x {
            for z in chunk_z.clone() {
                for y in sections.clone() {
                    self.pending_blocks
                        .entry((x, z))
                        .or_insert_with(|| AHashMap::with_capacity(sections.clone().count()))
                        .entry(y)
                        .or_insert_with(|| Vec::with_capacity(blocks_per_section));
                }
            }
        }

        self.set_internal_block_buffer(buffer);
    }

    /// Sets the internal block buffer.  
    ///
    /// Overwrites any and all data related to the buffer.  
    pub fn set_internal_block_buffer(&mut self, buffer: BlockBuffer) {
        self.pending_blocks = buffer;
        self.seen_blocks.clear();
    }

    /// Returns the index for a block in the [`Self::seen_blocks`] bitset based of it's coordinates  
    pub(crate) fn get_block_index(&self, x: u32, y: i32, z: u32) -> usize {
        let y_offset = (y as isize - self.config.world_height.start()) as usize;
        x as usize
            + y_offset * BLOCKS_PER_REGION as usize
            + z as usize
                * BLOCKS_PER_REGION as usize
                * (self.config.world_height.end() - self.config.world_height.start()) as usize
    }

    /// Returns a [`FixedBitSet`] with a default capacity that holds an entire regions blocks for check  
    pub(crate) fn get_default_block_bitset(config: &Config) -> FixedBitSet {
        FixedBitSet::with_capacity(Self::bitset_size(&config.world_height))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Result;

    #[test]
    fn pre_set_block() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_block(1, 2, 3, Block::try_new("minecraft:red_stained_glass")?);

        assert_eq!(region.pending_blocks.len(), 1);
        assert_eq!(region.seen_blocks.count_ones(..), 1);

        Ok(())
    }

    #[test]
    fn set_duplicate_block() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_block(52, -5, 395, Block::try_new("minecraft:red_stained_glass")?);
        let success =
            region.set_block(52, -5, 395, Block::try_new("minecraft:lime_stained_glass")?);

        assert_eq!(success, None);
        assert_eq!(region.pending_blocks.len(), 1);
        assert_eq!(region.seen_blocks.count_ones(..), 1);

        Ok(())
    }

    #[test]
    fn set_block() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_block(6, 52, 95, Block::try_new("minecraft:oak_planks")?);

        assert_eq!(region.pending_blocks.len(), 1);
        assert_eq!(region.seen_blocks.count_ones(..), 1);

        region.write_blocks()?;

        assert_eq!(
            region.get_block(6, 52, 95)?,
            Block::try_new("minecraft:oak_planks")?
        );
        assert_eq!(
            region.get_block(52, 1, 5)?,
            Block::try_new("minecraft:air")?
        );
        assert_eq!(region.pending_blocks.len(), 0);
        assert_eq!(region.seen_blocks.count_ones(..), 0);

        Ok(())
    }
}
