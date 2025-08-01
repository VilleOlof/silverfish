//! `write` handles all functions related to actually writing the [`Region`]'s internal block buffer
//! to it's chunks within the [`Region`], handles batching, encoding/decoding section data, etc.  

use crate::{
    BiomeCell, Block, CHUNK_OP, Config, Error, NbtString, Region, Result,
    chunk::ChunkData,
    data::{decode_data, encode_data},
    region::{clean_palette, get_biome_bit_count, get_block_bit_count, is_valid_chunk},
};
use ahash::AHashMap;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use simdnbt::owned::{NbtCompound, NbtList};
use std::sync::Arc;

impl Region {
    pub(crate) const BLOCK_DATA_LEN: usize = ChunkData::WIDTH.pow(3);
    pub(crate) const BIOME_DATA_LEN: usize = BiomeCell::CELL_SIZE.pow(3) as usize;

    /// Takes all pending block writes and applies all the blocks to the actual chunk NBT
    ///
    /// This function writes all the chunks within the region in parallel.    
    pub fn write_blocks(&mut self) -> Result<()> {
        self.chunks
            .par_iter_mut()
            .filter(|c| c.dirty_blocks)
            .try_for_each(|mut ref_mut| {
                let coords = *ref_mut.key();
                ref_mut.write_blocks(coords, &self.get_config())
            })?;

        Ok(())
    }

    /// Takes all pending biomes changes and writes them to the chunk NBT
    ///
    /// This function writes all the chunks within the region in parallel.    
    pub fn write_biomes(&mut self) -> Result<()> {
        self.chunks
            .par_iter_mut()
            .filter(|c| c.dirty_biomes)
            .try_for_each(|mut ref_mut| {
                let coords = *ref_mut.key();
                ref_mut.write_biomes(coords)
            })?;

        Ok(())
    }

    /// Set a single section (16\*16\*16) to a single [`Block`].  
    ///
    /// Writes the changes directly to the NBT.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # let mut region = Region::default();
    /// region.set_section((13, 15), 1, "minecraft:stone")?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn set_section<B: Into<Block>>(
        &mut self,
        chunk: (u8, u8),
        section: i8,
        block: B,
    ) -> Result<()> {
        Ok(self.set_sections(vec![(chunk, section, block)])?)
    }

    /// Set an entire section (16\*16\*16) to one single [`Block`].  
    ///
    /// Useful if you want to mass set a big area to one single block.
    ///
    /// Writes the changes directly to the NBT.  
    ///
    /// Argument tuple is: `((chunk_x, chunk_z), section_y, block)`
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # let mut region = Region::default();
    /// region.set_sections(vec![((5, 12), 6, "dirt"), ((14, 5), -1, "stone")])?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn set_sections<B: Into<Block>>(&mut self, sections: Vec<((u8, u8), i8, B)>) -> Result<()> {
        sections
            .into_iter()
            // we have to map because of block and it's into
            // rayon doesnt like it otherwise
            .map(|(cc, sy, b)| (cc, sy, b.into()))
            .collect::<Vec<((u8, u8), i8, Block)>>()
            .into_par_iter()
            .try_for_each(|(chunk_coords, section_y, block)| {
                assert!(
                    chunk_coords.0 < mca::REGION_SIZE as u8
                        && chunk_coords.1 < mca::REGION_SIZE as u8
                );

                // again, this part is just copied but hard to extrapolate
                let update_lighting = self.get_config().update_lighting;
                let mut chunk_data = self.get_chunk_mut(chunk_coords.0, chunk_coords.1)?;
                let nbt = Arc::get_mut(&mut chunk_data.nbt)
                    .ok_or(Error::TriedToAccessArc("chunk_data.nbt"))?;

                is_valid_chunk(&nbt, chunk_coords)?;

                // clear heightmaps if they exist since they can become outdated after this
                if let Some(height_maps) = nbt.compound_mut("Heightmaps") {
                    height_maps.clear();
                };

                if update_lighting {
                    *nbt.byte_mut("isLightOn")
                        .ok_or(Error::MissingNbtTag("isLightOn"))? = 0;
                }

                let chunk_ptr = nbt as *mut NbtCompound;
                let sections: &mut Vec<NbtCompound> = unsafe {
                    match (*chunk_ptr)
                        .list_mut("sections")
                        .ok_or(Error::MissingNbtTag("sections"))?
                    {
                        NbtList::Compound(c) => c,
                        _ => return Err(Error::InvalidNbtList("sections")),
                    }
                };

                let block_entities: &mut Vec<NbtCompound> = unsafe {
                    match (*chunk_ptr)
                        .list_mut("block_entities")
                        .ok_or(Error::MissingNbtTag("block_entities"))?
                    {
                        NbtList::Compound(c) => c,
                        NbtList::Empty => &mut vec![],
                        _ => return Err(Error::InvalidNbtList("block_entities")),
                    }
                };

                let section = sections
                    .iter_mut()
                    .try_find(|s| {
                        Ok::<bool, Error>(
                            s.byte("Y").ok_or(Error::MissingNbtTag("Y"))? == section_y,
                        )
                    })?
                    .ok_or(Error::MissingNbtTag("couldn't find section"))?;

                if self.get_config().update_lighting {
                    section.remove("BlockLight");
                    section.remove("SkyLight");
                }

                let state = section
                    .compound_mut("block_states")
                    .ok_or(Error::MissingNbtTag("block_states"))?;

                // when setting a single section, remove its data field and make sure
                // the palette only has a single block inside it
                state.remove("data");
                let palette = match state.list_mut("palette").unwrap() {
                    NbtList::Compound(c) => c,
                    _ => return Err(Error::InvalidNbtList("palette")),
                };

                palette.clear();
                palette.push(block.to_compound()?);

                assert_eq!(palette.len(), 1);

                // TODO most block entity things doesnt have tests for them
                // and im unsure if this actually works since i suck at math :)
                for i in 0..block_entities.len() {
                    let x = block_entities[i]
                        .int("x")
                        .ok_or(Error::MissingNbtTag("x"))?
                        & CHUNK_OP;
                    let y = block_entities[i]
                        .int("y")
                        .ok_or(Error::MissingNbtTag("y"))?
                        & CHUNK_OP;
                    let z = block_entities[i]
                        .int("z")
                        .ok_or(Error::MissingNbtTag("z"))?
                        & CHUNK_OP;

                    // check if x y z is within
                    if (chunk_coords.0..chunk_coords.0 + ChunkData::WIDTH as u8)
                        .contains(&(x as u8))
                        || ((section_y * ChunkData::WIDTH as i8)
                            ..(section_y * ChunkData::WIDTH as i8) + ChunkData::WIDTH as i8)
                            .contains(&(y as i8))
                        || (chunk_coords.1..chunk_coords.1 + ChunkData::WIDTH as u8)
                            .contains(&(z as u8))
                    {
                        block_entities.remove(i);
                    }
                }

                Ok::<(), Error>(())
            })?;

        Ok(())
    }
}

impl ChunkData {
    /// Writes the pending changes to the current chunk NBT
    pub fn write_blocks(&mut self, chunk_coords: (u8, u8), config: &Config) -> Result<()> {
        // we keep these here since we re-use these to hold onto their memory allocations.
        let mut old_indexes: [i64; Region::BLOCK_DATA_LEN] = [0; Region::BLOCK_DATA_LEN];
        let mut cached_palette_indexes: AHashMap<Block, i64> = AHashMap::with_capacity(4);
        let mut block_entity_cache: AHashMap<(i32, i32, i32), bool> = AHashMap::new();

        //  missing chunk etc is set via /set_block since pending is in chunks
        let nbt = Arc::get_mut(&mut self.nbt).ok_or(Error::TriedToAccessArc("chunk_data.nbt"))?;
        is_valid_chunk(&nbt, chunk_coords)?;

        // clear heightmaps if they exist since they can become outdated after this
        if let Some(height_maps) = nbt.compound_mut("Heightmaps") {
            height_maps.clear();
        };

        if config.update_lighting {
            *nbt.byte_mut("isLightOn")
                .ok_or(Error::MissingNbtTag("isLightOn"))? = 0;
        }

        // we do a little bit of unsafe :tf:
        let chunk_ptr = nbt as *mut NbtCompound;
        let sections: &mut Vec<NbtCompound> = unsafe {
            match (*chunk_ptr)
                .list_mut("sections")
                .ok_or(Error::MissingNbtTag("sections"))?
            {
                NbtList::Compound(c) => c,
                _ => return Err(Error::InvalidNbtList("sections")),
            }
        };

        let block_entities: &mut Vec<NbtCompound> = unsafe {
            match (*chunk_ptr)
                .list_mut("block_entities")
                .ok_or(Error::MissingNbtTag("block_entities"))?
            {
                NbtList::Compound(c) => c,
                NbtList::Empty => &mut vec![],
                _ => return Err(Error::InvalidNbtList("block_entities")),
            }
        };

        // a little cache so we can find the index directly and remove it instead of looking up the coords everytime
        for be in block_entities.iter() {
            let x = be.int("x").ok_or(Error::MissingNbtTag("x"))? & CHUNK_OP;
            let y = be.int("y").ok_or(Error::MissingNbtTag("y"))? & CHUNK_OP;
            let z = be.int("z").ok_or(Error::MissingNbtTag("z"))? & CHUNK_OP;

            block_entity_cache.insert((x, y, z), false);
        }

        for section in sections.iter_mut() {
            let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
            let pending_blocks = match self.pending_blocks.remove(&y) {
                Some(pending_blocks) => pending_blocks,
                None => continue,
            };

            if config.update_lighting {
                section.remove("BlockLight");
                section.remove("SkyLight");
            }

            let state = section
                .compound_mut("block_states")
                .ok_or(Error::MissingNbtTag("block_states"))?;

            // more unsafe :D
            let state_ptr = state as *mut NbtCompound;
            let palette = unsafe {
                match (*state_ptr)
                    .list_mut("palette")
                    .ok_or(Error::MissingNbtTag("palette"))?
                {
                    NbtList::Compound(c) => c,
                    _ => return Err(Error::InvalidNbtList("palette")),
                }
            };
            let data = unsafe { (*state_ptr).long_array("data") };

            let data_len = decode_data(&mut old_indexes, get_block_bit_count(palette.len()), data);

            // this *should* check for bad files
            for idx in old_indexes.iter_mut() {
                if *idx < 0 || *idx >= palette.len() as i64 {
                    return Err(Error::InvalidPaletteIndex(*idx));
                }
            }

            for block in pending_blocks {
                // micro perf thing would be to keep track of "unique blocks"
                // and if its just 1 unique block for this entire .write_blocks()
                // we dont need to do any of this pretty much
                // and if its a set of like 1-3 blocks, a vec would prob be faster than ahashmap.
                // anyhow, this cached_palette_index and keeping track of the indexes
                // is the slowest part of write_blocks(), like the hashing, getting etc.
                let palette_index = match cached_palette_indexes.get(&block.block) {
                    Some(idx) => *idx,
                    None => {
                        // we just try to find the pos directly, and if there is a pos, goood
                        // otherwise we can push and use the last index directly
                        let palette_index = palette.iter().position(|c| &block.block == c);
                        if let Some(palette_index) = palette_index {
                            cached_palette_indexes.insert(block.block, palette_index as i64);
                            palette_index as i64
                        } else {
                            let block_nbt = block.block.clone().to_compound()?;
                            // if we push we already know its the last current index
                            let palette_index = palette.len() as i64;
                            palette.push(block_nbt);
                            cached_palette_indexes.insert(block.block, palette_index);
                            palette_index
                        }
                    }
                };

                let (x, y, z) = (
                    block.coordinates.0,
                    block.coordinates.1 & CHUNK_OP, // divide Y into section here
                    block.coordinates.2,
                );
                let index = (x
                    + z * ChunkData::WIDTH as u32
                    + y as u32 * ChunkData::WIDTH as u32 * ChunkData::WIDTH as u32)
                    as usize;

                old_indexes[index] = palette_index;

                // if block entity at these coords, mark for deletion
                match block_entity_cache.get_mut(&(x as i32, y, z as i32)) {
                    Some(be) => *be = true,
                    None => (),
                };
            }

            cached_palette_indexes.clear();

            clean_palette(&mut old_indexes, data_len, palette);

            // remove any marked block entities
            block_entities.retain(|be| {
                let x = be.int("x").unwrap() & CHUNK_OP;
                let y = be.int("y").unwrap() & CHUNK_OP;
                let z = be.int("z").unwrap() & CHUNK_OP;

                match block_entity_cache.get(&(x, y, z)) {
                    Some(delete) if *delete => false,
                    _ => true,
                }
            });
            block_entity_cache.clear();

            if palette.len() == 1 {
                // if theres only 1 palette we can remove the data
                state.remove("data");
                continue;
            }

            encode_data(
                get_block_bit_count(palette.len()),
                &old_indexes,
                data_len,
                state,
            );
        }

        // we could to a per block unset of each incase this fails mid point it "could" be ran again
        self.seen_blocks.clear();
        // unmark it as dirt after processing
        self.dirty_blocks = false;

        Ok::<(), Error>(())
    }

    /// Writes the pending changes to the current chunk NBT
    pub fn write_biomes(&mut self, chunk_coords: (u8, u8)) -> Result<()> {
        // keep these here to hold onto their memory allocations.
        let mut old_indexes: [i64; Region::BIOME_DATA_LEN] = [0; Region::BIOME_DATA_LEN];
        let mut cached_palette_indexes: AHashMap<NbtString, i64> = AHashMap::new();

        let nbt = Arc::get_mut(&mut self.nbt).ok_or(Error::TriedToAccessArc("chunk_data.nbt"))?;
        is_valid_chunk(&nbt, chunk_coords)?;

        let sections: &mut Vec<NbtCompound> = match nbt
            .list_mut("sections")
            .ok_or(Error::MissingNbtTag("sections"))?
        {
            NbtList::Compound(c) => c,
            _ => return Err(Error::InvalidNbtList("sections")),
        };

        for section in sections {
            let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
            let pending_biomes = match self.pending_biomes.remove(&y) {
                Some(pending_biomes) => pending_biomes,
                None => continue,
            };

            cached_palette_indexes.clear();

            let state = section
                .compound_mut("biomes")
                .ok_or(Error::MissingNbtTag("biomes"))?;

            let state_ptr = state as *mut NbtCompound;
            let palette = unsafe {
                match (*state_ptr)
                    .list_mut("palette")
                    .ok_or(Error::MissingNbtTag("palette"))?
                {
                    NbtList::String(c) => c,
                    _ => return Err(Error::InvalidNbtList("palette")),
                }
            };
            let data = unsafe { (*state_ptr).long_array("data") };

            let data_len = decode_data(&mut old_indexes, get_block_bit_count(palette.len()), data);

            for biome in pending_biomes {
                let palette_index = match cached_palette_indexes.get(&biome.id) {
                    Some(idx) => *idx,
                    None => {
                        let is_in_palette = palette.iter().any(|b| b == biome.id);

                        if !is_in_palette {
                            palette.push(biome.id.clone().to_mutf8string());
                        }

                        let palette_index = palette
                            .iter()
                            .position(|b| b == biome.id)
                            .ok_or(Error::NotInBiomePalette(biome.id.clone()))?
                            as i64;
                        cached_palette_indexes.insert(biome.id, palette_index);
                        palette_index
                    }
                };

                let (x, y, z) = (biome.cell.cell.0, biome.cell.cell.1, biome.cell.cell.2);
                let index = (x
                    + z * BiomeCell::CELL_SIZE
                    + y * BiomeCell::CELL_SIZE * BiomeCell::CELL_SIZE)
                    as usize;

                old_indexes[index] = palette_index;
            }

            clean_palette(&mut old_indexes, data_len, palette);

            if palette.len() == 1 {
                // if theres only 1 palette we can remove the data
                state.remove("data");
                continue;
            }

            encode_data(
                get_biome_bit_count(palette.len()),
                &old_indexes,
                data_len,
                state,
            );
        }

        self.seen_biomes.clear();
        self.dirty_biomes = false;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Instant;

    use super::*;
    use crate::Name;

    #[test]
    fn set_section() -> Result<()> {
        let mut region = Region::default();
        region.set_section(
            (0, 0),
            2,
            Block::try_new(Name::new_namespace("minecraft:beacon"))?,
        )?;
        let beacon = region.get_block(5, 35, 11)?;
        assert_eq!(
            beacon,
            Block::try_new(Name::new_namespace("minecraft:beacon"))?
        );

        Ok(())
    }

    #[test]
    fn full_region_set_section() -> Result<()> {
        let mut region = Region::default();
        let mut sections = Vec::with_capacity(24_576);
        let block = Block::try_new(Name::new_namespace("minecraft:water"))?;

        for x in 0..32 {
            for y in -4..20 {
                for z in 0..32 {
                    sections.push(((x, z), y, block.clone()));
                }
            }
        }

        let instant = Instant::now();
        region.set_sections(sections)?;
        println!("{:?}", instant.elapsed());

        let sample = region.get_block(483, 281, 313)?;
        assert_eq!(sample, block);

        Ok(())
    }
}
