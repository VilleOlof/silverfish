//! `write` handles all functions related to actually writing the [`Region`]'s internal block buffer
//! to it's chunks within the [`Region`], handles batching, encoding/decoding section data, etc.  

use crate::{
    Block, Error, Region, Result, get_empty_chunk,
    region::{BlockWithCoordinate, get_bit_count},
};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use std::collections::HashMap;

impl Region {
    /// Takes all pending block writes and applies all the blocks to the actual chunk NBT
    ///
    /// Clears the internal buffered blocks instantly.  
    /// So if this function fails, do note that any blocks sent in via [`Self::set_block`] will get cleared.  
    pub fn write_blocks(&mut self) -> Result<()> {
        let pending_blocks = std::mem::take(&mut self.pending_blocks);
        let mut groups = group_blocks_into_chunks(pending_blocks);
        // reset seen_blocks already here since we swapped pending_blocks with default
        self.seen_blocks = Self::get_default_bitset();

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

            let status = chunk
                .string("Status")
                .ok_or(Error::MissingNbtTag("Status"))?
                .to_str();
            if status != Self::REQUIRED_STATUS {
                return Err(Error::NotFullyGenerated {
                    chunk: chunk_group.coordinate,
                    status: status.into_owned(),
                });
            }

            let data_version = chunk
                .int("DataVersion")
                .ok_or(Error::MissingNbtTag("DataVersion"))?;
            if data_version < Self::MIN_DATA_VERSION {
                return Err(Error::UnsupportedVersion {
                    chunk: chunk_group.coordinate,
                    data_version,
                });
            }

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

                // if no data found we directly skip to a pre-defined zeroed vec
                let mut old_indexes = match data {
                    Some(data) => {
                        let mut old_indexes: Vec<i64> = Vec::with_capacity(4096);

                        let bit_count: u32 = get_bit_count(palette.len());
                        let mut offset: u32 = 0;

                        let mask = (1 << bit_count) - 1;
                        for data_block in data.iter() {
                            while (offset * bit_count) + bit_count <= 64 {
                                let block = (data_block >> (offset * bit_count)) & mask;

                                old_indexes.push(block);

                                offset += 1
                            }
                            offset = 0;
                        }
                        old_indexes.truncate(4096);

                        old_indexes
                    }
                    None => vec![0; 4096],
                };

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
                                .ok_or(Error::NotInPalette(block.block.clone()))?
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

                // construct data/palette
                let mut palette_count: Vec<i32> = vec![0; palette.len()];
                for index in &old_indexes {
                    palette_count[*index as usize] += 1;
                }

                let mut palette_offsets: Vec<i64> = vec![0; palette.len()];

                let mut len = palette.len();
                let mut i = len as i32 - 1;
                while i >= 0 {
                    if palette_count[i as usize] == 0 {
                        palette.remove(i as usize);
                        len -= 1;

                        for j in (i as usize)..palette_count.len() {
                            palette_offsets[j as usize] += 1;
                        }
                    }
                    i -= 1;
                }

                for block in 0..old_indexes.len() {
                    old_indexes[block] -= palette_offsets[old_indexes[block] as usize];
                }

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

                let mut new_blockdata: Vec<i64> = Vec::with_capacity(4096);
                let bit_count: u32 = get_bit_count(palette.len());

                let mut offset = 0;
                let mut currrent_long: i64 = 0;
                for block in old_indexes.iter() {
                    currrent_long |= block << (offset * bit_count);
                    offset += 1;

                    if (offset * bit_count) + bit_count > 64 {
                        new_blockdata.push(currrent_long);
                        currrent_long = 0;
                        offset = 0;
                    }
                }

                if offset > 0 {
                    new_blockdata.push(currrent_long);
                }

                // store back the data, state is &mut to section
                if !state.contains("data") {
                    state.insert("data", NbtTag::LongArray(new_blockdata));
                } else {
                    // this unwrap is 100% to exist due to the above check
                    *state.long_array_mut("data").unwrap() = new_blockdata;
                }
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
