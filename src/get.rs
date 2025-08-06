//! `get` contains functions related to getting blocks from a [`Region`].  

use crate::{
    BLOCKS_PER_REGION, BiomeCell, BiomeCellWithId, Block, CHUNK_OP, ChunkData, Config, Coords,
    Error, NbtString, Region, Result,
    biome::group_cells_into_chunks,
    data::decode_data,
    paletted_blocks::PalettedBlocks,
    region::{get_biome_bit_count, get_block_bit_count},
};
use ahash::AHashMap;
use simdnbt::owned::NbtCompound;

/// Core internal function to search and find blocks within a chunk.  
///
/// This is extrapolated into its own function with a shit ton of `&mut` due
/// to this part using a ton of different data from the parent function.  
fn find_blocks<'a, const N: usize>(
    found_blocks: &mut PalettedBlocks<'a>,
    indexes: &mut [i64; N],
    group: &mut GetChunkGroup,
    chunk_nbt: &NbtCompound,
) -> Result<()> {
    let sections = chunk_nbt
        .list("sections")
        .ok_or(Error::MissingNbtTag("sections"))?
        .compounds()
        .ok_or(Error::InvalidNbtType("sections"))?;

    for section in sections {
        let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
        let blocks_to_get = match group.sections.remove(&y) {
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

        // we dont need to use the len returned as we dont use it anywhere after
        let _ = decode_data(indexes, get_block_bit_count(palette.len()), data);

        // This function returns a lifetime bound to self
        // So a mutable borrow while having the returned value
        // is impossible via rust, and i know that the palette data
        // from the self.chunk.>>> wont ever get removed nor deleted
        // while the caller has the return value and hasnt dropped it.
        // this is just so rust allows us to keep a ref outside this loop
        // since it deems that "chunk" and thus the palette data goes out
        // of scope and gets "drops" before this function returns.
        let palette_ptr = palette as *const _ as *const [NbtCompound];
        let pal_index = found_blocks.insert_palette_only(unsafe { &*palette_ptr });

        for c in blocks_to_get {
            // x and z have already been &'d but y is section specific
            let index = (c.x & CHUNK_OP as u32)
                + ((c.z & CHUNK_OP as u32) * ChunkData::WIDTH as u32)
                + ((c.y & CHUNK_OP as i32) as u32 * (ChunkData::WIDTH.pow(2)) as u32);

            let palette_index: usize = *indexes.get(index as usize).ok_or(Error::OutOfBounds {
                len: indexes.len(),
                index: index as usize,
            })? as usize;

            // a bit confusing both are basically a palette index
            // the first is which "palette" index, and the second
            // is what index in the palette
            found_blocks.insert_at(c, pal_index, palette_index as u32);
        }
    }

    Ok(())
}

impl Region {
    /// Returns the block at the specified coordinates *(local to within the **region**)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::{Region, Block};
    /// # let region = Region::default();
    /// let block = region.get_block((5, 97, 385))?;
    /// assert_eq!(block, Block::new("minecraft:air"));
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_block<C>(&self, coords: C) -> Result<Block>
    where
        C: Into<Coords>,
    {
        let coords: Coords = coords.into();
        // a bit unsure of this funky stuff but uhh, surely works
        self.get_blocks(&[coords.clone()])
            .map(|b| match b.get(coords) {
                Ok(Some(b)) => b,
                _ => unreachable!(
                    "If this was panicked, something VERY wrong with 'PalettedBlocks' happened"
                ),
            })
    }

    // you could make get_blocks even more generic and maybe faster for the user.
    // the slowest part is the grouping, so we "could" leave that up to the user if they wanted
    // or we group it if not, by having an enum that is either "Ungrouped(&[C])" or "Grouped(Vec<GetChunkGroup>)"
    // and then the main "blocks" arg becomes "C: Into<MaybeGrouped>" and if its not grouped in that enum
    // we can just do it automatically. the thing is that "Vec<GetChunkGroup>" is REALLY awkward to group yourself
    // and if tested just using a hashmap of "AHashMap<(u8, u8>, AHashMap<i8, Vec<Coords>>>" and just iterating
    // over it raw isntead of the vec thing, but its always slower than grouping and then iterating over that vec.
    // so im leaving this comment here to mention this design choice and that we could do it or something similar.
    // - ville (2025-08-03, 23:30)

    /// Returns the blocks at the specified coordinates *(local to within the **region**)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```
    /// # let region = silverfish::Region::default();
    /// let blocks = region.get_blocks(&[(5, 97, 385), (5, 97, 386), (52, 12, 52)])?;
    /// assert_eq!(blocks.len(), 3);
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_blocks<'a, C>(&'a self, blocks: &[C]) -> Result<PalettedBlocks<'a>>
    where
        C: Into<Coords> + Copy,
    {
        let mut found_blocks =
            PalettedBlocks::new(self.config.world_height.clone(), BLOCKS_PER_REGION as usize);
        let mut groups = group_region(blocks);

        let mut indexes: [i64; Region::BLOCK_DATA_LEN] = [0; Region::BLOCK_DATA_LEN];

        for chunk_group in groups.iter_mut() {
            let chunk = self
                .chunks
                .get(&chunk_group.coordinate)
                .ok_or(Error::NoChunk(
                    chunk_group.coordinate.0,
                    chunk_group.coordinate.1,
                ))?;

            find_blocks(&mut found_blocks, &mut indexes, chunk_group, &chunk.nbt)?;
        }

        Ok(found_blocks)
    }

    /// Returns the biome at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```
    /// # let region = silverfish::Region::default();
    /// let biome = region.get_biome((82, 62, 7))?;
    /// assert_eq!(biome, "minecraft:plains");
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_biome<C: Into<BiomeCell>>(&self, cell: C) -> Result<NbtString> {
        self.get_biomes(vec![cell]).map(|mut b| b.swap_remove(0).id)
    }

    // get_biomes doesn't need the fancy palettedblock stuff and advanced mechanics
    // since theres only 1,572,864 biome cells per REGION and were fiiiine
    // no body gets that many biomes at once, and if so its their fault and they can split it up :)
    /// Returns the biomes at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`silverfish::to_region_local`](crate::to_region_local).  
    ///
    /// ## Example
    /// ```
    /// # let region = silverfish::Region::default();
    /// let biomes = region.get_biomes(vec![(52, 85, 152), (94, -4, 481)])?;
    /// assert_eq!(biomes.len(), 2);
    /// # Ok::<(), silverfish::Error>(())
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

impl ChunkData {
    /// Returns the block at the specified coordinates *(local to within the **chunk**)*.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::{Region, Block};
    /// # let region = Region::default();
    /// let chunk = region.get_chunk_mut(0, 5)?;
    /// let block = chunk.get_block((8, 1, 14))?;
    /// assert_eq!(block, Block::new("minecraft:air"));
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_block<C>(&self, coords: C) -> Result<Block>
    where
        C: Into<Coords>,
    {
        let coords: Coords = coords.into();
        self.get_blocks(&[coords.clone()])
            .map(|b| match b.get(coords) {
                Ok(Some(b)) => b,
                _ => unreachable!(
                    "If this was panicked, something VERY wrong with 'PalettedBlocks' happened"
                ),
            })
    }

    /// Returns the blocks at the specified coordinates *(local to within the **chunk**)*.  
    ///
    /// ## Example
    /// ```
    /// # let region = silverfish::Region::default();
    /// let chunk = region.get_chunk_mut(0, 2)?;
    /// let blocks = chunk.get_blocks(&[(5, 10, 4), (5, 3, 1), (4, 12, 9)])?;
    /// assert_eq!(blocks.len(), 3);
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_blocks<'a, C>(&'a self, blocks: &[C]) -> Result<PalettedBlocks<'a>>
    where
        C: Into<Coords> + Copy,
    {
        let mut found_blocks =
            PalettedBlocks::new(self.world_height.clone(), BLOCKS_PER_REGION as usize);
        let mut chunk_group = group_chunk(blocks);

        let mut indexes: [i64; Region::BLOCK_DATA_LEN] = [0; Region::BLOCK_DATA_LEN];

        find_blocks(&mut found_blocks, &mut indexes, &mut chunk_group, &self.nbt)?;

        Ok(found_blocks)
    }
}

pub struct GetChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: AHashMap<i8, Vec<Coords>>,
}

/// Groups a list of blocks into their own sections and chunks within a region  
fn group_region<C>(blocks: &[C]) -> Vec<GetChunkGroup>
where
    C: Into<Coords> + Copy,
{
    let mut map: AHashMap<(u8, u8), AHashMap<i8, Vec<Coords>>> =
        AHashMap::with_capacity(mca::REGION_SIZE * mca::REGION_SIZE);

    for coords in blocks {
        let coords: Coords = (*coords).into();
        let (chunk_x, chunk_z) = (
            (coords.x / ChunkData::WIDTH as u32) as u8,
            (coords.z / ChunkData::WIDTH as u32) as u8,
        );
        let section_y = (coords.y as f64 / ChunkData::WIDTH as f64).floor() as i8;

        map.entry((chunk_x, chunk_z))
            .or_insert_with(|| {
                AHashMap::with_capacity(
                    Config::DEFAULT_WORLD_HEIGHT.clone().count() / ChunkData::WIDTH,
                )
            })
            .entry(section_y)
            .or_insert_with(|| Vec::with_capacity(16))
            .push(coords);
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

/// Groups a list of blocks into their own sections within a chunk
pub fn group_chunk<C>(blocks: &[C]) -> GetChunkGroup
where
    C: Into<Coords> + Copy,
{
    if blocks.len() == 0 {
        // undefined behavior, we cant backtrack from this
        panic!("Tried to group a block array of 0 length")
    }

    let mut map: AHashMap<i8, Vec<Coords>> =
        AHashMap::with_capacity(Config::DEFAULT_WORLD_HEIGHT.clone().count() / ChunkData::WIDTH);

    for coords in blocks {
        let coords: Coords = (*coords).into();
        let section_y = (coords.y as f64 / ChunkData::WIDTH as f64).floor() as i8;

        map.entry(section_y)
            .or_insert_with(|| Vec::with_capacity(16))
            .push(coords);
    }

    // all of these are already assumed to be within the same chunk
    let coords: Coords = blocks[0].into();
    let chunk_pos = (
        (coords.x / ChunkData::WIDTH as u32) as u8,
        (coords.z / ChunkData::WIDTH as u32) as u8,
    );

    GetChunkGroup {
        coordinate: chunk_pos,
        sections: map,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_block() -> Result<()> {
        let mut region = Region::default();
        region.set_block((5, 52, 17), "minecraft:crafter")?;
        region.write_blocks()?;
        let block = region.get_block((5, 52, 17))?;
        assert_eq!(block, Block::new("minecraft:crafter"));

        Ok(())
    }

    #[test]
    fn get_blocks_region() -> Result<()> {
        let mut region = Region::default();
        region.set_block((82, 14, 92), "minecraft:lime_concrete")?;
        region.set_block((56, 192, 25), "minecraft:red_concrete")?;
        region.set_block((482, -52, 131), "minecraft:yellow_concrete")?;
        region.write_blocks()?;

        let blocks = region.get_blocks(&[(82, 14, 92), (56, 192, 25), (482, -52, 131)])?;
        assert_eq!(blocks.len(), 3);
        println!("{:?}", blocks.get_all());
        assert!(blocks.contains(&Block::new("minecraft:lime_concrete")));
        assert!(blocks.contains(&Block::new("minecraft:red_concrete")));
        assert!(blocks.contains(&Block::new("minecraft:yellow_concrete")));

        Ok(())
    }

    #[test]
    fn get_blocks_chunk() -> Result<()> {
        let region = Region::default();

        let mut chunk = region.get_chunk_mut(0, 1)?;
        chunk.set_block((5, 1, 1), "minecraft:magenta_concrete")?;
        chunk.set_block((5, 12, 6), "minecraft:pink_concrete")?;
        chunk.write_blocks((0, 1), region.get_config())?;

        let blocks = chunk.get_blocks(&[(5, 1, 1), (5, 12, 6)])?;
        assert_eq!(blocks.len(), 2);
        assert!(blocks.contains(&Block::new("minecraft:magenta_concrete")));
        assert!(blocks.contains(&Block::new("minecraft:pink_concrete")));

        Ok(())
    }

    #[test]
    fn invalid_get_coords() {
        let region = Region::default();
        assert!(region.get_block((852, 14, 5212)).is_err())
    }
}
