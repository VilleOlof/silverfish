//! `write` handles all functions related to actually writing the [`Region`]'s internal block buffer
//! to it's chunks within the [`Region`], handles batching, encoding/decoding section data, etc.  

use crate::{
    Block, Error, Region, Result,
    data::{decode_data, encode_data},
    get_empty_chunk,
    region::{BlockWithCoordinate, clean_palette, get_bit_count, is_valid_chunk},
};
use simdnbt::owned::{NbtCompound, NbtList};
use std::collections::HashMap;

impl Region {
    pub(crate) const BLOCK_DATA_LEN: usize = 4096;

    /// Takes all pending block writes and applies all the blocks to the actual chunk NBT
    ///
    /// Clears the internal buffered blocks instantly.  
    /// So if this function fails, do note that any blocks sent in via [`Self::set_block`] will get cleared.  
    pub fn write_blocks(&mut self) -> Result<()> {
        let pending_blocks = std::mem::take(&mut self.pending_blocks);
        let mut groups = group_blocks_into_chunks(pending_blocks);
        // reset seen_blocks already here since we swapped pending_blocks with default
        self.seen_blocks.clear();

        // theres probably some way to convert this into a rayon par_iter or something
        // so this can be heavily faster, the only annoying thing is since this is all mutable reference to self.chunks
        // we cant Mutex: Self or self.chunks and lock because the entire "thread" needs to use a mutable ref constantly
        for chunk_group in groups.iter_mut() {
            let chunk = match self.chunks.get_mut(&chunk_group.coordinate) {
                Some(chunk) => chunk,
                None if self.config.create_chunk_if_missing => {
                    self.chunks.insert(
                        chunk_group.coordinate,
                        get_empty_chunk(chunk_group.coordinate, self.region_coords),
                    );
                    self.chunks
                        .get_mut(&chunk_group.coordinate)
                        .ok_or(Error::NoChunk(
                            chunk_group.coordinate.0,
                            chunk_group.coordinate.1,
                        ))?
                }
                None => {
                    return Err(Error::TriedToModifyMissingChunk(
                        chunk_group.coordinate.0,
                        chunk_group.coordinate.1,
                    ));
                }
            };

            is_valid_chunk(&chunk, chunk_group.coordinate)?;

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
                    _ => return Err(Error::InvalidNbtList("sections")),
                }
            };
            // a little cache so we can find the index directly and remove it instead of looking up the coords everytime
            let mut block_entity_cache: HashMap<(i32, i32, i32), bool> = HashMap::new();
            for be in block_entities.iter() {
                let x = be.int("x").ok_or(Error::MissingNbtTag("x"))? & 15;
                let y = be.int("y").ok_or(Error::MissingNbtTag("y"))? & 15;
                let z = be.int("z").ok_or(Error::MissingNbtTag("z"))? & 15;

                block_entity_cache.insert((x, y, z), false);
            }

            for section in sections.iter_mut() {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let pending_blocks = match chunk_group.sections.remove(&y) {
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

                let mut old_indexes =
                    decode_data(Region::BLOCK_DATA_LEN, get_bit_count(palette.len()), data);

                // this *should* check for bad files
                for idx in old_indexes.iter_mut() {
                    if *idx < 0 || *idx >= palette.len() as i64 {
                        return Err(Error::InvalidPaletteIndex(*idx));
                    }
                }

                let mut cached_palette_indexes: HashMap<&Block, i64> = HashMap::new();
                for block in &pending_blocks {
                    let is_in_palette = palette.iter().any(|c| &block.block == c);

                    if !is_in_palette {
                        // this is the only .clone() in this entire code and i hate it but i must have it grrr
                        let block_nbt = block.block.clone().to_compound()?;
                        palette.push(block_nbt);
                    }
                    let palette_index = match cached_palette_indexes.get(&block.block) {
                        Some(idx) => *idx,
                        None => {
                            let palette_index = palette
                                .iter()
                                .position(|c| &block.block == c)
                                .ok_or(Error::NotInBlockPalette(block.block.clone()))?
                                as i64;
                            cached_palette_indexes.insert(&block.block, palette_index);
                            palette_index
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

                clean_palette(&mut old_indexes, palette);

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

                if palette.len() == 1 {
                    // if theres only 1 palette we can remove the data
                    state.remove("data");
                    continue;
                }

                encode_data(
                    Region::BLOCK_DATA_LEN,
                    get_bit_count(palette.len()),
                    old_indexes,
                    state,
                );
            }
        }

        Ok(())
    }
}

pub(crate) struct ChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: HashMap<i8, Vec<BlockWithCoordinate>>,
}

/// Groups a list of blocks into their own sections and chunks within a region  
fn group_blocks_into_chunks(blocks: Vec<BlockWithCoordinate>) -> Vec<ChunkGroup> {
    let mut map: HashMap<(u8, u8), HashMap<i8, Vec<BlockWithCoordinate>>> = HashMap::new();

    for block in blocks {
        let (chunk_x, chunk_z) = (
            (block.coordinates.0 as f64 / 16f64).floor() as u8,
            (block.coordinates.2 as f64 / 16f64).floor() as u8,
        );
        let section_y = (block.coordinates.1 as f64 / 16f64).floor() as i8;

        map.entry((chunk_x, chunk_z))
            .or_default()
            .entry(section_y)
            .or_default()
            .push(block);
    }

    let mut chunk_groups = vec![];
    for ((chunk_x, chunk_z), section_map) in map {
        chunk_groups.push(ChunkGroup {
            coordinate: (chunk_x, chunk_z),
            sections: section_map,
        });
    }

    chunk_groups
}
