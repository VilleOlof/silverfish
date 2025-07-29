//! `write` handles all functions related to actually writing the [`Region`]'s internal block buffer
//! to it's chunks within the [`Region`], handles batching, encoding/decoding section data, etc.  

use crate::{
    Block, Error, Region, Result,
    data::{decode_data, encode_data},
    get_empty_chunk,
    region::{clean_palette, get_bit_count, is_valid_chunk},
};
use ahash::AHashMap;
use simdnbt::owned::{NbtCompound, NbtList};

impl Region {
    pub(crate) const BLOCK_DATA_LEN: usize = 4096;

    /// Takes all pending block writes and applies all the blocks to the actual chunk NBT
    ///
    /// Clears the internal buffered blocks instantly.  
    /// So if this function fails, do note that any blocks sent in via [`Self::set_block`] will get cleared.  
    pub fn write_blocks(&mut self) -> Result<()> {
        // reset seen_blocks already here since we consume pending_blocks
        self.seen_blocks.clear();

        // we keep these here since we re-use these to hold onto their memory allocations.
        let mut old_indexes: [i64; Region::BLOCK_DATA_LEN] = [0; Region::BLOCK_DATA_LEN];
        let mut cached_palette_indexes: AHashMap<Block, i64> = AHashMap::with_capacity(4);
        let mut block_entity_cache: AHashMap<(i32, i32, i32), bool> = AHashMap::new();

        // theres probably some way to convert this into a rayon par_iter or something
        // so this can be heavily faster, the only annoying thing is since this is all mutable reference to self.chunks
        // we cant Mutex: Self or self.chunks and lock because the entire "thread" needs to use a mutable ref constantly
        for (chunk_coords, section_map) in self.pending_blocks.iter_mut() {
            let chunk = match self.chunks.get_mut(&chunk_coords) {
                Some(chunk) => chunk,
                None if self.config.create_chunk_if_missing => {
                    self.chunks.insert(
                        *chunk_coords,
                        get_empty_chunk(*chunk_coords, self.region_coords),
                    );
                    self.chunks
                        .get_mut(&chunk_coords)
                        .ok_or(Error::NoChunk(chunk_coords.0, chunk_coords.1))?
                }
                None => {
                    return Err(Error::TriedToModifyMissingChunk(
                        chunk_coords.0,
                        chunk_coords.1,
                    ));
                }
            };

            is_valid_chunk(&chunk, *chunk_coords)?;

            // clear heightmaps if they exist since they can become outdated after this
            if let Some(height_maps) = chunk.compound_mut("Heightmaps") {
                height_maps.clear();
            };

            if self.config.update_lighting {
                *chunk
                    .byte_mut("isLightOn")
                    .ok_or(Error::MissingNbtTag("isLightOn"))? = 0;
            }

            // we do a little bit of unsafe :tf:
            let chunk_ptr = chunk as *mut NbtCompound;
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
                let x = be.int("x").ok_or(Error::MissingNbtTag("x"))? & 15;
                let y = be.int("y").ok_or(Error::MissingNbtTag("y"))? & 15;
                let z = be.int("z").ok_or(Error::MissingNbtTag("z"))? & 15;

                block_entity_cache.insert((x, y, z), false);
            }

            for section in sections.iter_mut() {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let pending_blocks = match section_map.remove(&y) {
                    Some(pending_blocks) => pending_blocks,
                    None => continue,
                };

                if self.config.update_lighting {
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

                let data_len = decode_data(&mut old_indexes, get_bit_count(palette.len()), data);

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
                        block.coordinates.0 & 15,
                        block.coordinates.1 & 15,
                        block.coordinates.2 & 15,
                    );
                    let index = (x + z * 16 + y as u32 * 16 * 16) as usize;

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
                    let x = be.int("x").unwrap() & 15;
                    let y = be.int("y").unwrap() & 15;
                    let z = be.int("z").unwrap() & 15;

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

                encode_data(get_bit_count(palette.len()), &old_indexes, data_len, state);
            }
        }

        self.pending_blocks.clear();

        Ok(())
    }

    /// Set a single section (16\*16\*16) to a single [`Block`].  
    ///
    /// Writes the changes directly to the NBT.  
    ///
    /// ## Example
    /// ```no_run
    /// let mut region = Region::full_empty((0, 0));
    /// region.set_section((13, 15), 1, "minecraft:stone")?;
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
    /// ## Example
    /// ```no_run
    /// let mut region = Region::full_empty((0, 0));
    /// region.set_sections(vec![((5, 12), 6, "dirt"), ((14, 5), -1, "stone")])?;
    /// ```
    pub fn set_sections<B: Into<Block>>(&mut self, sections: Vec<((u8, u8), i8, B)>) -> Result<()> {
        for (chunk_coords, section_y, block) in sections {
            assert!(chunk_coords.0 < 32 && chunk_coords.1 < 32);

            // again, this part is just copied but hard to extrapolate
            let chunk = match self.chunks.get_mut(&chunk_coords) {
                Some(chunk) => chunk,
                None if self.config.create_chunk_if_missing => {
                    self.chunks.insert(
                        chunk_coords,
                        get_empty_chunk(chunk_coords, self.region_coords),
                    );
                    self.chunks
                        .get_mut(&chunk_coords)
                        .ok_or(Error::NoChunk(chunk_coords.0, chunk_coords.1))?
                }
                None => {
                    return Err(Error::TriedToModifyMissingChunk(
                        chunk_coords.0,
                        chunk_coords.1,
                    ));
                }
            };

            is_valid_chunk(&chunk, chunk_coords)?;

            // clear heightmaps if they exist since they can become outdated after this
            if let Some(height_maps) = chunk.compound_mut("Heightmaps") {
                height_maps.clear();
            };

            if self.config.update_lighting {
                *chunk
                    .byte_mut("isLightOn")
                    .ok_or(Error::MissingNbtTag("isLightOn"))? = 0;
            }

            let chunk_ptr = chunk as *mut NbtCompound;
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
                    Ok::<bool, Error>(s.byte("Y").ok_or(Error::MissingNbtTag("Y"))? == section_y)
                })?
                .ok_or(Error::MissingNbtTag("couldn't find section"))?;

            if self.config.update_lighting {
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
            let block: Block = block.into();
            palette.push(block.to_compound()?);

            assert_eq!(palette.len(), 1);

            // TODO most block entity things doesnt have tests for them
            // and im unsure if this actually works since i suck at math :)
            for i in 0..block_entities.len() {
                let x = block_entities[i]
                    .int("x")
                    .ok_or(Error::MissingNbtTag("x"))?
                    & 15;
                let y = block_entities[i]
                    .int("y")
                    .ok_or(Error::MissingNbtTag("y"))?
                    & 15;
                let z = block_entities[i]
                    .int("z")
                    .ok_or(Error::MissingNbtTag("z"))?
                    & 15;

                // check if x y z is within
                if (chunk_coords.0..chunk_coords.0 + 16).contains(&(x as u8))
                    || ((section_y * 16)..(section_y * 16) + 16).contains(&(y as i8))
                    || (chunk_coords.1..chunk_coords.1 + 16).contains(&(z as u8))
                {
                    block_entities.remove(i);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::Name;

    use super::*;

    #[test]
    fn set_section() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
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
}
