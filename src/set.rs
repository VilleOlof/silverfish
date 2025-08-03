//! `set` handles all functions related to pushing blocks to the [`Region`]'s internal block buffer.  

use crate::{BiomeCell, Block, CHUNK_OP, ChunkData, Coords, NbtString, Region, Result};
use ahash::AHashMap;
use std::ops::Range;

impl Region {
    /// Set a block at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
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
    /// ```
    /// # use silverfish::{Region, Block};
    /// # let mut region = Region::default();
    /// let _ = region.set_block((5, 97, 385), Block::new("dirt"))?;
    /// // and to actually write the changes to the NBT
    /// region.write_blocks()?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn set_block<C, B: Into<Block>>(&mut self, coords: C, block: B) -> Result<Option<()>>
    where
        C: Into<Coords>,
    {
        let coords: Coords = coords.into();
        let (chunk_x, chunk_z) = (
            (coords.x / ChunkData::WIDTH as u32) as u8,
            (coords.z / ChunkData::WIDTH as u32) as u8,
        );
        let mut chunk_data = self.get_chunk_mut(chunk_x, chunk_z)?;
        // convert the coordinates into chunk local
        // we skip Y until writing since it gets only divided into sections then
        chunk_data.set_block(
            Coords::new(
                coords.x & CHUNK_OP as u32,
                coords.y,
                coords.z & CHUNK_OP as u32,
            ),
            block,
        )
    }

    /// Biomes in Minecraft are stored in 4x4x4 cells within each section.  
    ///
    ///
    /// To specify which cell you want to change the biome of, you'll need to specify:  
    /// - The chunk coordinates *(local to the region, 0..=31)*
    /// - The section Y index *(-4..=19)*
    /// - The cell coordinates within the section *(0..=3)*
    ///
    /// You can use [`coordinates_to_biome_cell`](crate::biome::coordinates_to_biome_cell) to convert region local coordinates to the needed data.  
    ///
    /// Alternatively, you can just give it the coordinates directly since [`Coords`] implements `Into<BiomeCell>`
    ///
    /// ## Example
    /// ```
    /// # let mut region = silverfish::Region::default();
    /// let _ = region.set_biome(((5, 19), 6, (2, 1, 3)), "minecraft:cherry_grove")?;
    /// // to actually write the biomes to the NBT
    /// region.write_biomes()?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn set_biome<C: Into<BiomeCell>, B: Into<NbtString>>(
        &mut self,
        cell: C,
        biome: B,
    ) -> Result<Option<()>> {
        let cell: BiomeCell = cell.into();
        let biome: NbtString = biome.into();

        let mut chunk_data = self.get_chunk_mut(cell.chunk.0, cell.chunk.1)?;
        chunk_data.set_biome(cell, biome)
    }

    /// Due to how the internal buffer is grouped for batching later on.
    /// You can only define `chunk` and `section` ranges and how many blocks within each section.
    ///
    /// Overwrites the already existing internal block buffer.
    ///
    /// Useful if you know which areas in your region that you'll modify.
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # let mut region = Region::default();
    /// region.allocate_block_buffer(0..16, 4..8, 1..3, 1024)?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn allocate_block_buffer(
        &mut self,
        chunks_x: Range<u8>,
        chunk_z: Range<u8>,
        sections: Range<i8>,
        blocks_per_section: usize,
    ) -> Result<()> {
        for x in chunks_x {
            for z in chunk_z.clone() {
                let mut chunk = self.get_chunk_mut(x, z)?;

                let mut map = AHashMap::new();
                for y in sections.clone() {
                    map.insert(y, Vec::with_capacity(blocks_per_section));
                }

                chunk.set_internal_block_buffer(map);
            }
        }

        Ok(())
    }

    /// Due to how the internal buffer is grouped for batching later on.
    /// You can only define `chunk` and `section` ranges and how many biome cells within each section.
    ///
    /// Overwrites the already existing internal biome buffer.
    ///
    /// Useful if you know which areas in your region that you'll modify.
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # let mut region = Region::default();
    /// region.allocate_block_buffer(0..16, 4..8, 1..3, 32)?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn allocate_biome_buffer(
        &mut self,
        chunks_x: Range<u8>,
        chunk_z: Range<u8>,
        sections: Range<i8>,
        cells_per_section: usize,
    ) -> Result<()> {
        for x in chunks_x {
            for z in chunk_z.clone() {
                let mut chunk = self.get_chunk_mut(x, z)?;

                let mut map = AHashMap::new();
                for y in sections.clone() {
                    map.insert(y, Vec::with_capacity(cells_per_section));
                }

                chunk.set_internal_block_buffer(map);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Result;

    #[test]
    fn pre_set_block() -> Result<()> {
        let mut region = Region::default();
        region
            .set_block((1, 2, 3), Block::try_new("minecraft:red_stained_glass")?)?
            .unwrap();

        assert_eq!(region.get_raw_chunk(0, 0)?.unwrap().pending_blocks.len(), 1);
        assert_eq!(
            region
                .get_raw_chunk(0, 0)?
                .unwrap()
                .seen_blocks
                .count_ones(..),
            1
        );

        Ok(())
    }

    #[test]
    fn set_duplicate_block() -> Result<()> {
        let mut region = Region::default();
        region
            .set_block(
                (52, -5, 395),
                Block::try_new("minecraft:red_stained_glass")?,
            )?
            .unwrap();
        let success = region.set_block(
            (52, -5, 395),
            Block::try_new("minecraft:lime_stained_glass")?,
        )?;

        assert_eq!(success, None);
        assert_eq!(
            region.get_raw_chunk(3, 24)?.unwrap().pending_blocks.len(),
            1
        );
        assert_eq!(
            region
                .get_raw_chunk(3, 24)?
                .unwrap()
                .seen_blocks
                .count_ones(..),
            1
        );

        Ok(())
    }

    #[test]
    fn set_block() -> Result<()> {
        let mut region = Region::default();
        region
            .set_block((6, 52, 95), Block::try_new("minecraft:oak_planks")?)?
            .unwrap();

        assert_eq!(region.get_raw_chunk(0, 5)?.unwrap().pending_blocks.len(), 1);
        assert_eq!(
            region
                .get_raw_chunk(0, 5)?
                .unwrap()
                .seen_blocks
                .count_ones(..),
            1
        );

        region.write_blocks()?;

        assert_eq!(
            region.get_block((6, 52, 95))?,
            Block::try_new("minecraft:oak_planks")?
        );
        assert_eq!(
            region.get_block((52, 1, 5))?,
            Block::try_new("minecraft:air")?
        );

        assert_eq!(region.get_raw_chunk(0, 5)?.unwrap().pending_blocks.len(), 0);
        assert_eq!(
            region
                .get_raw_chunk(0, 5)?
                .unwrap()
                .seen_blocks
                .count_ones(..),
            0
        );

        Ok(())
    }
}
