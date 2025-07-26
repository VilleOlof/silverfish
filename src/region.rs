use crate::nbt::{Block, NbtConversion};
use mca::{CompressionType, RegionIter, RegionReader, RegionWriter};
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound, NbtList, NbtTag};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read, Write},
};

// so ive tested filling an entire region with 1 single block
//  (best case scenario for palette cache)
// and got a throughput of `1,936,204` blocks per second*
// on my machine with optimized compiler flags

/// An in-memory region to read and write blocks to the chunks within.  
#[derive(Debug, Clone)]
pub struct Region {
    pub chunks: HashMap<(u8, u8), NbtCompound>,
    pending_blocks: HashMap<(u32, i32, u32), Block>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub create_chunk_if_missing: bool,
    pub update_lighting: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            create_chunk_if_missing: false,
            update_lighting: true,
        }
    }
}

impl Region {
    /// Whatever status the chunks needs to be to allow modification.  
    const REQUIRED_STATUS: &'static str = "minecraft:full";
    /// the minimum dataversion that light updating works on.
    /// since "isLightOn" was added in 1.18 (i think)
    pub const MIN_LIGHT_DATA_VERSION: i32 = 2860;

    /// Creates an empty [`Region`] with no chunks or anything.  
    ///
    /// [`Config::create_chunk_if_missing`] will set to `true` from this  
    #[inline(always)]
    pub fn empty() -> Self {
        Self {
            chunks: HashMap::new(),
            pending_blocks: HashMap::new(),
            config: Config {
                create_chunk_if_missing: true,
                ..Default::default()
            },
        }
    }

    /// Creates a full [`Region`] with empty chunks in it.  
    #[inline(always)]
    pub fn full_empty() -> Self {
        let mut chunks = HashMap::new();

        for x in 0..mca::REGION_SIZE as u8 {
            for z in 0..mca::REGION_SIZE as u8 {
                chunks.insert((x, z), get_empty_chunk((x, z)));
            }
        }

        Self::from_nbt(chunks)
    }

    /// Creates a new [`Region`] with chunks from `chunks`
    #[inline(always)]
    pub fn from_nbt(chunks: HashMap<(u8, u8), NbtCompound>) -> Self {
        Self {
            chunks,
            pending_blocks: HashMap::new(),
            config: Config::default(),
        }
    }

    /// Creates a [`Region`] from an already existing region
    ///
    /// ## Example
    /// ```rust
    /// let region = Region::from_region(&mut File::open("r.0.0.mca").unwrap());
    /// ```
    pub fn from_region<R: Read>(reader: &mut R) -> Self {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes).unwrap();
        let region_reader = RegionReader::new(&bytes).unwrap();

        let mut chunks = HashMap::new();
        for (i, chunk) in region_reader.iter().enumerate() {
            let chunk = chunk.unwrap();
            let chunk = match chunk {
                Some(c) => c.decompress().unwrap(),
                None => continue,
            };

            let chunk_nbt = simdnbt::owned::read(&mut Cursor::new(&chunk))
                .unwrap()
                .unwrap()
                .as_compound();
            let (x, z) = RegionIter::get_chunk_coordinate(i);

            chunks.insert((x as u8, z as u8), chunk_nbt);
        }

        Self::from_nbt(chunks)
    }

    /// Set a block at the specified coordinates *(local to within the region)*.  
    ///
    /// Returns true if there was previously a block at this coordinate in the buffer.  
    ///
    /// **Note:** This doesn't actually set the block but writes it to an internal buffer.  
    ///
    /// To actually write the changes to the `chunks`, call [`Region::write_blocks`]
    ///
    /// ## Example
    /// ```rust
    /// region.set_block(5, 97, 385, Block::new("dirt"));
    /// // and to actually write the changes to the NBT
    /// region.write_blocks();
    /// ```
    #[inline(always)]
    pub fn set_block(&mut self, x: u32, y: i32, z: u32, block: Block) -> bool {
        self.pending_blocks.insert((x, y, z), block).is_some()
    }

    /// Returns the block at the specified coordinates *(local to within the region)*.  
    ///
    /// ## Example
    /// ```rust
    /// let block = region.get_block(5, 97, 385);
    /// assert_eq!(block, Block::new("dirt"));
    /// ```
    pub fn get_block(&self, x: u32, y: i32, z: u32) -> Block {
        assert!(x < 512 && z < 512);

        let (chunk_x, chunk_z) = (
            (x as f64 / 16f64).floor() as u8,
            (z as f64 / 16f64).floor() as u8,
        );
        let section_y = (y as f64 / 16f64).floor() as i8;
        let index = (x & 15) + ((z & 15) * 16) + ((y & 15) as u32 * 16 * 16);

        let chunk = self.chunks.get(&(chunk_x, chunk_z)).unwrap();
        let sections = chunk.list("sections").unwrap().compounds().unwrap();
        let section = sections
            .iter()
            .find(|s| s.byte("Y").unwrap() == section_y)
            .unwrap();

        let state = section.compound("block_states").unwrap();

        let data = state
            .long_array("data")
            .map(|d| d.to_vec())
            .unwrap_or(vec![0; 4096]);
        let palette = state.list("palette").unwrap().compounds().unwrap();

        let bit_count: u32 = get_bit_count(palette.len());

        let mut indexes: Vec<i64> = Vec::with_capacity(4096);

        let mut offset: u32 = 0;
        for data_block in data.iter() {
            while (offset * bit_count) + bit_count <= 64 {
                let block = (data_block >> (offset * bit_count)) & ((1 << bit_count) - 1);

                indexes.push(block);

                offset += 1
            }
            offset = 0;
        }

        let palette_index = indexes[index as usize] as usize;
        let block = palette.get(palette_index).unwrap();
        return Block::from_compound(block).unwrap();
    }

    /// Takes all pending block writes and applies all the blocks to the actual chunk NBT
    pub fn write_blocks(&mut self) {
        let groups = group_blocks_into_chunks(self.pending_blocks.clone());

        for chunk_group in groups {
            let chunk = match self.chunks.get_mut(&chunk_group.coordinate) {
                Some(chunk) => chunk,
                None if self.config.create_chunk_if_missing => {
                    self.chunks.insert(
                        chunk_group.coordinate,
                        get_empty_chunk(chunk_group.coordinate),
                    );
                    self.chunks.get_mut(&chunk_group.coordinate).unwrap()
                }
                None => panic!("tried to modify missing chunk"),
            };

            let status = chunk.string("Status").unwrap().to_str();
            if status != Self::REQUIRED_STATUS {
                panic!("Tried to modify a chunk that isn't fully generated")
            }

            let data_version = chunk.int("DataVersion").unwrap();
            if self.config.create_chunk_if_missing && data_version < Self::MIN_LIGHT_DATA_VERSION {
                panic!(
                    "Tried to update lighting on a DataVersion prior to {}",
                    Self::MIN_LIGHT_DATA_VERSION
                )
            }

            if self.config.update_lighting {
                *chunk.byte_mut("isLightOn").unwrap() = 0;
            }

            // we do a little bit of unsafe :tf:
            let chunk_ptr = chunk as *mut NbtCompound;
            let sections: &mut Vec<NbtCompound> = unsafe {
                match (*chunk_ptr).list_mut("sections").unwrap() {
                    NbtList::Compound(c) => c,
                    _ => panic!("Invalid NBT list"),
                }
            };

            let block_entities: &mut Vec<NbtCompound> = unsafe {
                match (*chunk_ptr).list_mut("block_entities").unwrap() {
                    NbtList::Compound(c) => c,
                    NbtList::Empty => &mut vec![],
                    _ => {
                        panic!("Invalid NBT list")
                    }
                }
            };
            // a little cache so we can find the index directly and remove it instead of looking up the coords everytime
            let mut block_entity_cache: HashMap<(i32, i32, i32), bool> = HashMap::new();
            for be in block_entities.iter() {
                let x = be.int("x").unwrap() & 15;
                let y = be.int("y").unwrap() & 15;
                let z = be.int("z").unwrap() & 15;

                block_entity_cache.insert((x, y, z), false);
            }

            for section in sections.iter_mut() {
                let y = section.byte("Y").unwrap();
                let pending_blocks = match chunk_group.sections.get(&y) {
                    Some(pending_blocks) => pending_blocks,
                    None => continue,
                };

                if self.config.update_lighting {
                    section.remove("BlockLight");
                    section.remove("SkyLight");
                }

                let state = section.compound_mut("block_states").unwrap();

                // more unsafe :D
                let state_ptr = state as *mut NbtCompound;
                let palette = unsafe {
                    match (*state_ptr).list_mut("palette").unwrap() {
                        NbtList::Compound(c) => c,
                        _ => panic!("Invalid nbt list"),
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

                let palette = match state.list_mut("palette").unwrap() {
                    NbtList::Compound(c) => c,
                    _ => panic!("Invalid nbt list"),
                };

                // this *should* check for bad files
                for idx in old_indexes.iter_mut() {
                    if *idx < 0 || *idx >= palette.len() as i64 {
                        panic!("Invalid block index in data: {}", idx);
                    }
                }

                let mut cached_palette_indexes: HashMap<&Block, i64> = HashMap::new();
                for (block_coords, block) in pending_blocks {
                    let is_in_palette = palette.iter().any(|c| block == c);

                    if !is_in_palette {
                        let block_nbt = block.clone().to_compound().unwrap();
                        palette.push(block_nbt);
                    }
                    let palette_index = match cached_palette_indexes.get(block) {
                        Some(idx) => *idx,
                        None => {
                            let palette_index =
                                palette.iter().position(|c| block == c).unwrap() as i64;
                            cached_palette_indexes.insert(block, palette_index);
                            palette_index
                        }
                    };

                    let (x, y, z) = (
                        block_coords.0 & 15,
                        block_coords.1 & 15,
                        block_coords.2 & 15,
                    );
                    let index = x + z * 16 + y as u32 * 16 * 16;

                    old_indexes[index as usize] = palette_index;

                    // if blocke entity at these coords, mark for deletion
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

                // this function may be faster but "something" is wrong with it
                // broder kan inte programmering, lär dig programmering tack q:^)
                /*
                let mut palette_offsets: Vec<i64> = vec![0; palette.len()];
                
                let mut len = palette.len();
                let mut i = len - 1;
                while i != 0 {
                    if palette_count[i] == 0 {
                        palette.remove(i);
                        len -= 1;
                        
                        for j in i..len {
                            palette_offsets[j as usize] += 1;
                        }
                    }
                    i -= 1;
                }
                
                for block in 0..old_indexes.len() {
                    old_indexes[block] -= palette_offsets[old_indexes[block] as usize];
                }
                */

                let mut unused_indexes = Vec::new();
                for (idx, _p) in palette.iter().enumerate() {
                    if old_indexes.contains(&(idx as i64)) {
                        continue;
                    }

                    unused_indexes.push(idx as i64);
                }

                for index in unused_indexes.iter().rev() {
                    palette.remove(*index as usize);
                    for block in old_indexes.iter_mut() {
                        if *block > *index {
                            *block -= 1;
                        }
                    }
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
                    *state.long_array_mut("data").unwrap() = new_blockdata;
                }
            }
        }
    }

    /// Writes the region to the specified writer.  
    ///
    /// **Note:** If you haven't called [`Region::write_blocks`] this will most likely  
    /// just return whatever input you gave it initially
    pub fn write<W: Write>(self, writer: &mut W) {
        let mut region_writer = RegionWriter::new();

        for ((x, z), chunk_nbt) in self.chunks {
            let mut raw_nbt = vec![];
            let wrapped = Nbt::Some(BaseNbt::new("", chunk_nbt));
            wrapped.write(&mut raw_nbt);
            region_writer
                .push_chunk_with_compression(&raw_nbt, (x, z), CompressionType::Zlib)
                .unwrap();
        }

        region_writer.write(writer).unwrap();
    }
}

struct ChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: HashMap<i8, HashMap<(u32, i32, u32), Block>>,
}

/// Groups a list of blocks into their own sections and chunks within a region  
fn group_blocks_into_chunks(blocks: HashMap<(u32, i32, u32), Block>) -> Vec<ChunkGroup> {
    let mut map: HashMap<(u8, u8), HashMap<i8, HashMap<(u32, i32, u32), Block>>> = HashMap::new();

    for ((b_x, b_y, b_z), block) in blocks {
        let (chunk_x, chunk_z) = (
            (b_x as f64 / 16f64).floor() as u8,
            (b_z as f64 / 16f64).floor() as u8,
        );
        let section_y = (b_y as f64 / 16f64).floor() as i8;

        map.entry((chunk_x, chunk_z))
            .or_default()
            .entry(section_y)
            .or_default()
            .insert((b_x, b_y, b_z), block);
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

/// A custom PartialEq implementation so we dont need to convert NbtCompound to Block  
/// or Block to NbtCompound and can compare them fast
impl PartialEq<&NbtCompound> for &Block {
    fn eq(&self, other: &&NbtCompound) -> bool {
        let name = match other.string("Name") {
            Some(n) => n,
            None => return false,
        };
        if self.name != name.to_str() {
            return false;
        }

        if let Some(block_props) = &self.properties {
            let props = match other.compound("Properties") {
                Some(props) => props,
                None => return false,
            };

            let mut other_map: BTreeMap<String, String> = BTreeMap::new();

            for (k, v) in props.iter() {
                other_map.insert(
                    k.to_str().to_string(),
                    v.string().unwrap().to_str().to_string(),
                );
            }

            if &other_map != block_props {
                return false;
            }
        } else {
            if other.contains("Properties") {
                return false;
            }
        }

        true
    }
}

// returns the bit count for whatever palette_len.
// we dont actually need to calculate anything fancy
// palette_len cant be more than 4096 so we can pre set it up
#[inline(always)]
fn get_bit_count(len: usize) -> u32 {
    match len {
        0 => 0,
        1..=16 => 4,
        17..=32 => 5,
        33..=64 => 6,
        65..=128 => 7,
        129..=256 => 8,
        257..=512 => 9,
        513..=1024 => 10,
        1025..=2048 => 11,
        2049..=4096 => 12,
        _ => panic!("invalid palette len"),
    }
}

/// Generates an empty chunk with plains as the default biome and air in all sections  
///
/// DataVersion is defaulted to [`Region::MIN_LIGHT_DATA_VERSION`]
pub fn get_empty_chunk(coords: (u8, u8)) -> NbtCompound {
    let mut sections: Vec<NbtCompound> = vec![];

    for y in -4..=19 {
        let biomes = NbtCompound::from_values(vec![(
            "palette".into(),
            NbtTag::List(NbtList::String(vec!["minecraft:plains".into()])),
        )]);
        let block_states = NbtCompound::from_values(vec![(
            "palette".into(),
            NbtTag::List(NbtList::Compound(vec![NbtCompound::from_values(vec![(
                "Name".into(),
                NbtTag::String("minecraft:air".into()),
            )])])),
        )]);

        sections.push(NbtCompound::from_values(vec![
            ("Y".into(), NbtTag::Byte(y)),
            ("biomes".into(), NbtTag::Compound(biomes)),
            ("block_states".into(), NbtTag::Compound(block_states)),
        ]));
    }

    let chunk = NbtCompound::from_values(vec![
        (
            "Status".into(),
            NbtTag::String(Region::REQUIRED_STATUS.into()),
        ),
        (
            "DataVersion".into(),
            NbtTag::Int(Region::MIN_LIGHT_DATA_VERSION),
        ),
        ("sections".into(), NbtTag::List(NbtList::Compound(sections))),
        ("block_entities".into(), NbtTag::List(NbtList::Empty)),
        ("isLightOn".into(), NbtTag::Byte(0)),
        ("xPos".into(), NbtTag::Int(coords.0 as i32)),
        ("zPos".into(), NbtTag::Int(coords.1 as i32)),
    ]);

    chunk
}
