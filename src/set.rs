//! `set` handles all functions related to pushing blocks to the [`Region`]'s internal block buffer.  

use crate::{Block, Region, region::BlockWithCoordinate};
use fixedbitset::FixedBitSet;

impl Region {
    pub(crate) const BITSET_SIZE: usize =
        Self::REGION_X_Z_WIDTH * Self::REGION_Y_WIDTH * Self::REGION_X_Z_WIDTH;

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
    /// region.write_blocks();
    /// ```
    #[inline(always)]
    pub fn set_block(&mut self, x: u32, y: i32, z: u32, block: Block) -> Option<()> {
        let index = self.get_block_index(x, y, z);
        if !self.seen_blocks.contains(index) {
            self.seen_blocks.insert(index);
            self.pending_blocks.push(BlockWithCoordinate {
                coordinates: (x, y, z),
                block,
            });
            return Some(());
        }

        None
    }

    /// Returns the index for a block in the [`Self::seen_blocks`] bitset based of it's coordinates  
    #[inline(always)]
    pub(crate) fn get_block_index(&self, x: u32, y: i32, z: u32) -> usize {
        let y_offset = (y as isize - Self::REGION_Y_MIN) as usize;
        x as usize
            + y_offset * Self::REGION_X_Z_WIDTH
            + z as usize * Self::REGION_X_Z_WIDTH * Self::REGION_Y_WIDTH
    }

    /// Returns a [`FixedBitSet`] with a default capacity that holds an entire regions blocks for check  
    #[inline(always)]
    pub(crate) fn get_default_bitset() -> FixedBitSet {
        FixedBitSet::with_capacity(Self::BITSET_SIZE)
    }
}
