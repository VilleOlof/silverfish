//! `get` contains functions related to getting blocks from a [`Region`].  

use crate::{
    BiomeCell, BiomeCellWithId, Block, Error, NbtString, Region, Result,
    biome::group_cells_into_chunks,
    data::decode_data,
    region::{BlockWithCoordinate, get_biome_bit_count, get_block_bit_count},
};
use ahash::AHashMap;

impl Region {
    /// Returns the block at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```no_run
    /// let block = region.get_block(5, 97, 385)?;
    /// assert_eq!(block, Block::new("dirt"));
    /// ```
    pub fn get_block(&self, x: u32, y: i32, z: u32) -> Result<Block> {
        self.get_blocks(&[(x, y, z)])
            .map(|mut b| b.swap_remove(0).block)
    }

    // TODO: get_block isnt in parallel, and i tried and it was basically as fast/slow
    // like 15 seconds to get all the blocks?, really?, when it takes sub second to write to all blocks?

    /// Returns the blocks at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```no_run
    /// let blocks = region.get_blocks(&[(5, 97, 385), (5, 97, 386), 52, 12, 52])?;
    /// assert_eq!(blocks.len(), 3);
    /// ```
    pub fn get_blocks(&self, blocks: &[(u32, i32, u32)]) -> Result<Vec<BlockWithCoordinate>> {
        let mut found_blocks = Vec::with_capacity(blocks.len());
        let mut groups = group_coordinates_into_chunks(blocks);

        for chunk_group in groups.iter_mut() {
            let chunk = self
                .chunks
                .get(&chunk_group.coordinate)
                .ok_or(Error::NoChunk(
                    chunk_group.coordinate.0,
                    chunk_group.coordinate.1,
                ))?;

            let sections = chunk
                .nbt
                .list("sections")
                .ok_or(Error::MissingNbtTag("sections"))?
                .compounds()
                .ok_or(Error::InvalidNbtType("sections"))?;

            let mut indexes: [i64; Region::BLOCK_DATA_LEN] = [0; Region::BLOCK_DATA_LEN];

            for section in sections {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let blocks_to_get = match chunk_group.sections.remove(&y) {
                    Some(blocks) => blocks,
                    None => continue,
                };

                let state = section
                    .compound("block_states")
                    .ok_or(Error::MissingNbtTag("block_states"))?;

                let data = state.long_array("data");
                let palette = state
                    .list("palette")
                    .ok_or(Error::MissingNbtTag("palette"))?
                    .compounds()
                    .ok_or(Error::InvalidNbtType("palette"))?;

                decode_data(&mut indexes, get_block_bit_count(palette.len()), data);

                for (x, y, z) in blocks_to_get {
                    let index = (x & 15) + ((z & 15) * 16) + ((y & 15) as u32 * 16 * 16);

                    let palette_index: usize =
                        *indexes.get(index as usize).ok_or(Error::OutOfBounds {
                            len: indexes.len(),
                            index: index as usize,
                        })? as usize;
                    let block = palette.get(palette_index).ok_or(Error::OutOfBounds {
                        len: palette.len(),
                        index: palette_index,
                    })?;

                    let block = Block::from_compound(block)?;
                    found_blocks.push(BlockWithCoordinate {
                        coordinates: (x, y, z),
                        block,
                    });
                }
            }
        }

        Ok(found_blocks)
    }

    /// Returns the biome at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```no_run
    /// let biome = region.get_biome((82, 62, 7))?;
    /// assert_eq!(biome, "minecraft:meadow");
    /// ```
    pub fn get_biome<C: Into<BiomeCell>>(&self, cell: C) -> Result<NbtString> {
        self.get_biomes(vec![cell]).map(|mut b| b.swap_remove(0).id)
    }

    /// Returns the biomes at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```no_run
    /// let biomes = region.get_biomes(vec![(52, 85, 152), (94, -4, 481)])?;
    /// assert_eq!(biomes.len(), 2);
    /// ```
    pub fn get_biomes<C: Into<BiomeCell>>(&self, cells: Vec<C>) -> Result<Vec<BiomeCellWithId>> {
        let mut found_biomes = Vec::with_capacity(cells.len());
        let mut groups = group_cells_into_chunks(cells);

        for chunk_group in groups.iter_mut() {
            let chunk = self
                .chunks
                .get(&chunk_group.coordinate)
                .ok_or(Error::NoChunk(
                    chunk_group.coordinate.0,
                    chunk_group.coordinate.1,
                ))?;

            let sections = chunk
                .nbt
                .list("sections")
                .ok_or(Error::MissingNbtTag("sections"))?
                .compounds()
                .ok_or(Error::InvalidNbtType("sections"))?;

            for section in sections {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let biomes_to_get = match chunk_group.sections.remove(&y) {
                    Some(biomes) => biomes,
                    None => continue,
                };

                let state = section
                    .compound("biomes")
                    .ok_or(Error::MissingNbtTag("biomes"))?;

                let data = state.long_array("data");
                let palette = state
                    .list("palette")
                    .ok_or(Error::MissingNbtTag("palette"))?
                    .strings()
                    .ok_or(Error::InvalidNbtType("palette"))?;

                let mut indexes: [i64; Region::BIOME_DATA_LEN] = [0; Region::BIOME_DATA_LEN];
                decode_data(&mut indexes, get_biome_bit_count(palette.len()), data);

                for cell in biomes_to_get {
                    let (x, y, z) = (cell.cell.0, cell.cell.1, cell.cell.2);
                    let index = (x
                        + z * BiomeCell::CELL_SIZE
                        + y * BiomeCell::CELL_SIZE * BiomeCell::CELL_SIZE)
                        as usize;

                    let palette_index: usize =
                        *indexes.get(index as usize).ok_or(Error::OutOfBounds {
                            len: indexes.len(),
                            index: index as usize,
                        })? as usize;
                    let id = palette.get(palette_index).ok_or(Error::OutOfBounds {
                        len: palette.len(),
                        index: palette_index,
                    })?;

                    found_biomes.push(BiomeCellWithId {
                        cell,
                        id: NbtString::from_mutf8str(Some(id))
                            .ok_or(Error::InvalidNbtType("biome palette id isn't a string"))?,
                    });
                }
            }
        }

        Ok(found_biomes)
    }
}

pub(crate) struct GetChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: AHashMap<i8, Vec<(u32, i32, u32)>>,
}

/// Groups a list of blocks into their own sections and chunks within a region  
fn group_coordinates_into_chunks(blocks: &[(u32, i32, u32)]) -> Vec<GetChunkGroup> {
    let mut map: AHashMap<(u8, u8), AHashMap<i8, Vec<(u32, i32, u32)>>> = AHashMap::new();

    for (x, y, z) in blocks {
        let (chunk_x, chunk_z) = (
            (*x as f64 / 16f64).floor() as u8,
            (*z as f64 / 16f64).floor() as u8,
        );
        let section_y = (*y as f64 / 16f64).floor() as i8;

        map.entry((chunk_x, chunk_z))
            .or_insert_with(|| AHashMap::with_capacity(24))
            .entry(section_y)
            .or_insert_with(|| Vec::with_capacity(4096))
            .push((*x, *y, *z));
    }

    let mut chunk_groups = Vec::with_capacity(map.len());
    for ((chunk_x, chunk_z), section_map) in map {
        chunk_groups.push(GetChunkGroup {
            coordinate: (chunk_x, chunk_z),
            sections: section_map,
        });
    }

    chunk_groups
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_block() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_block(5, 52, 17, "minecraft:crafter")?;
        region.write_blocks()?;
        let block = region.get_block(5, 52, 17)?;
        assert_eq!(block, Block::new("minecraft:crafter"));

        Ok(())
    }

    #[test]
    fn get_blocks() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_block(82, 14, 92, "minecraft:lime_concrete")?;
        region.set_block(56, 192, 25, "minecraft:red_concrete")?;
        region.set_block(482, -52, 131, "minecraft:yellow_concrete")?;
        region.write_blocks()?;

        let blocks = region.get_blocks(&[(82, 14, 92), (56, 192, 25), (482, -52, 131)])?;
        assert_eq!(blocks.len(), 3);

        let blocks = blocks.into_iter().map(|b| b.block).collect::<Vec<Block>>();
        assert!(blocks.contains(&Block::new("minecraft:lime_concrete")));
        assert!(blocks.contains(&Block::new("minecraft:red_concrete")));
        assert!(blocks.contains(&Block::new("minecraft:yellow_concrete")));

        Ok(())
    }

    #[test]
    fn invalid_get_coords() {
        let region = Region::full_empty((0, 0));
        assert!(region.get_block(852, 14, 5212).is_err())
    }
}
