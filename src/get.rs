//! `get` contains functions related to getting blocks from a [`Region`].  

use crate::{Block, Error, NbtConversion, Region, Result, region::get_bit_count};

impl Region {
    /// Returns the block at the specified coordinates *(local to within the region)*.  
    ///
    /// ## Example
    /// ```rust
    /// let block = region.get_block(5, 97, 385)?;
    /// assert_eq!(block, Block::new("dirt"));
    /// ```
    pub fn get_block(&self, x: u32, y: i32, z: u32) -> Result<Block> {
        if x as usize > Self::REGION_X_Z_WIDTH || z as usize > Self::REGION_X_Z_WIDTH {
            return Err(Error::CoordinatesOutOfRegionBounds(x, z));
        }

        let (chunk_x, chunk_z) = (
            (x as f64 / 16f64).floor() as u8,
            (z as f64 / 16f64).floor() as u8,
        );
        let section_y = (y as f64 / 16f64).floor() as i8;
        let index = (x & 15) + ((z & 15) * 16) + ((y & 15) as u32 * 16 * 16);

        let chunk = self
            .chunks
            .get(&(chunk_x, chunk_z))
            .ok_or(Error::NoChunk(chunk_x, chunk_z))?;
        let sections = chunk
            .list("sections")
            .ok_or(Error::MissingNbtTag("sections"))?
            .compounds()
            .ok_or(Error::InvalidNbtType("sections"))?;
        let section = sections
            .iter()
            // we write it to -99 so the match will always fail if Y doesnt exist
            // try_find is unstable :(
            .find(|s| s.byte("Y").unwrap_or(-99) == section_y)
            .ok_or(Error::NoSectionFound(section_y))?;

        let state = section
            .compound("block_states")
            .ok_or(Error::MissingNbtTag("block_states"))?;

        let data = state
            .long_array("data")
            .map(|d| d.to_vec())
            .unwrap_or(vec![0; 4096]);
        let palette = state
            .list("palette")
            .ok_or(Error::MissingNbtTag("palette"))?
            .compounds()
            .ok_or(Error::InvalidNbtType("palette"))?;

        let bit_count: u32 = get_bit_count(palette.len());

        let mut indexes: Vec<i64> = Vec::with_capacity(4096);

        let mut offset: u32 = 0;
        let mask = (1 << bit_count) - 1;
        for data_block in data.iter() {
            while (offset * bit_count) + bit_count <= 64 {
                let block = (data_block >> (offset * bit_count)) & mask;

                indexes.push(block);

                offset += 1
            }
            offset = 0;
        }

        let palette_index: usize = *indexes.get(index as usize).ok_or(Error::OutOfBounds {
            len: indexes.len(),
            index: index as usize,
        })? as usize;
        let block = palette.get(palette_index).ok_or(Error::OutOfBounds {
            len: palette.len(),
            index: palette_index,
        })?;

        return Ok(Block::from_compound(block)?);
    }
}
