//! `region` contains the core [`Region`] struct used to set/get blocks within the specified Region.  
//!
//! Contains functions for constructing a [`Region`] and writing itself to a specified buffer.  

use crate::{
    BLOCKS_PER_REGION, Coords,
    chunk::ChunkData,
    config::Config,
    error::{Error, Result},
    nbt::Block,
};
use ahash::AHashMap;
use dashmap::{
    DashMap,
    mapref::one::{Ref, RefMut},
};
use mca::{CompressionType, RegionIter, RegionReader, RegionWriter};
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound, NbtList, NbtTag};
use std::{
    fmt::Debug,
    io::{Cursor, Read, Write},
    ops::{Deref, Range},
};

/// An in-memory region to read and write blocks to the chunks within.  
#[derive(Clone)]
pub struct Region {
    /// The chunks within the Region, mapped to their coordinates
    pub chunks: DashMap<(u8, u8), ChunkData>,
    /// Config on how it should handle certain scenarios
    pub(crate) config: Config,
    /// Coordinates for this specific region
    pub region_coords: (i32, i32),
}

/// Just a [`Block`] but with a set of coordinates attached to them.  
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWithCoordinate {
    /// The attached coordinates related to the block.  
    pub coordinates: Coords,
    /// The block itself.  
    pub block: Block,
}

impl Region {
    /// Whatever status the chunks needs to be to allow modification.  
    pub(crate) const REQUIRED_STATUS: &'static str = "minecraft:full";
    /// the minimum dataversion that this crate works with.  
    ///
    /// This is due the massive structural changes in how the nbt is stored that was introduced in `21w39a` & `21w43a`
    pub const MIN_DATA_VERSION: i32 = 2860;

    /// Returns the Region's [`Config`]
    pub fn get_config(&self) -> &Config {
        &self.config
    }

    /// Sets the [`Config`] and re-inits all internal buffers if `world_height` is different.  
    ///
    /// Returns `true` if the `world_height` was different and it did reset all internal buffers.  
    pub fn set_config(&mut self, config: Config) -> Result<bool> {
        let changed_world_height = if self.config.world_height != config.world_height {
            self.set_world_height(config.world_height.clone())?;
            true
        } else {
            false
        };

        self.config = config;

        Ok(changed_world_height)
    }

    /// Updates the world height in the [`Config`].  
    ///
    /// #### Why is `world_height` private in [`Config`] and only mutated through [`Region`] ?
    /// Well, when a region is first constructed it defaults an internal bitset to a certain size.  
    /// for performance reasons, and if you update world_height, we also need to re-init that bitset.
    /// *(this function also clears all internal buffers related to biomes)*.
    /// and a config can only be mutated on a region after the consumer has gotten it.  
    /// So when you get a region, it always defaults to Minecrafts vanilla range of world_height.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # let mut region = Region::default();
    /// region.set_world_height(128..320);
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn set_world_height(&mut self, range: Range<isize>) -> Result<()> {
        // clear all the chunks buffers
        let world_height_count = range.clone().count();
        for x in 0..32 {
            for z in 0..32 {
                let mut chunk = self.get_chunk_mut(x, z)?;
                chunk.pending_blocks = AHashMap::new();
                chunk.pending_biomes = AHashMap::new();
                chunk.seen_blocks = ChunkData::block_bitset(world_height_count);
                chunk.seen_biomes = ChunkData::biome_bitset(world_height_count);
            }
        }

        self.config.world_height = range;

        Ok(())
    }

    /// Creates an empty [`Region`] with no chunks or anything.  
    ///
    /// [`Config::create_chunk_if_missing`] will set to `true` from this  
    pub fn empty(region_coords: (i32, i32)) -> Self {
        let config = Config {
            create_chunk_if_missing: true,
            ..Default::default()
        };

        Self {
            chunks: DashMap::new(),
            region_coords,
            config,
        }
    }

    /// Creates a full [`Region`] with empty chunks in it.  
    pub fn full_empty(region_coords: (i32, i32)) -> Self {
        let mut chunks = AHashMap::new();

        for x in 0..mca::REGION_SIZE as u8 {
            for z in 0..mca::REGION_SIZE as u8 {
                chunks.insert(
                    (x, z),
                    get_empty_chunk((x, z), region_coords, Config::DEFAULT_WORLD_HEIGHT),
                );
            }
        }

        Self::from_nbt(chunks, region_coords)
    }

    /// Creates a new [`Region`] with chunks from `chunks`
    pub fn from_nbt(chunks: AHashMap<(u8, u8), NbtCompound>, region_coords: (i32, i32)) -> Self {
        let config = Config::default();

        let chunks = chunks
            .into_iter()
            .map(|(k, v)| (k, ChunkData::new(v, config.world_height.clone())))
            .collect();

        Self {
            chunks,
            region_coords,
            config,
        }
    }

    /// Creates a [`Region`] from an already existing region
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Region;
    /// # use std::fs::File;
    /// let mut region = Region::from_region(&mut File::open("r.0.0.mca")?, (0, 0))?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn from_region<R: Read>(reader: &mut R, region_coords: (i32, i32)) -> Result<Self> {
        let mut bytes = Vec::with_capacity(4_194_304); // 4 MB, just an average start on the vec to skip a few common re-allocations
        reader.read_to_end(&mut bytes)?;
        let region_reader = RegionReader::new(&bytes)?;

        let mut chunks = AHashMap::new();
        for (i, chunk) in region_reader.iter().enumerate() {
            let chunk = chunk?;
            let chunk = match chunk {
                Some(c) => c.decompress()?,
                None => continue,
            };

            let chunk_nbt = match simdnbt::owned::read(&mut Cursor::new(&chunk))? {
                Nbt::Some(nbt) => nbt.as_compound(),
                Nbt::None => return Err(Error::InvalidNbtType("base_nbt")),
            };
            let (x, z) = RegionIter::get_chunk_coordinate(i);

            chunks.insert((x as u8, z as u8), chunk_nbt);
        }

        Ok(Self::from_nbt(chunks, region_coords))
    }

    /// Writes the region to the specified writer.  
    ///
    /// **Note:** If you haven't called [`Region::write_blocks`] this will most likely  
    /// just return whatever input you gave it initially
    ///
    /// ## Example
    /// ```
    /// # let mut region = silverfish::Region::default();
    /// let mut buf = vec![];
    /// region.write(&mut buf)?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn write<W: Write>(self, writer: &mut W) -> Result<()> {
        let mut region_writer = RegionWriter::new();

        for ((x, z), chunk_data) in self.chunks {
            let mut raw_nbt = vec![];
            let wrapped = Nbt::Some(BaseNbt::new("", chunk_data.nbt));
            wrapped.write(&mut raw_nbt);
            region_writer.push_chunk_with_compression(&raw_nbt, (x, z), CompressionType::Zlib)?;
        }

        region_writer.write(writer)?;

        Ok(())
    }

    /// Returns the chunk nbt data found at the given chunk coordinates.  
    ///
    /// Do note that these chunk coordinates are local to within the region itself.  
    ///
    /// ## Example
    /// ```
    /// # let mut region = silverfish::Region::default();
    /// let chunk = region.get_chunk(5, 17)?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_chunk(&self, x: u8, z: u8) -> Result<Option<Ref<'_, (u8, u8), ChunkData>>> {
        if x >= mca::REGION_SIZE as u8 || z >= mca::REGION_SIZE as u8 {
            return Err(Error::ChunkOutOfRegionBounds(x, z));
        }

        Ok(self.chunks.get(&(x, z)))
    }

    /// Returns the raw [`ChunkData`].  
    #[cfg(test)]
    pub(crate) fn get_raw_chunk(
        &self,
        x: u8,
        z: u8,
    ) -> Result<Option<Ref<'_, (u8, u8), ChunkData>>> {
        Ok(self.chunks.get(&(x, z)))
    }

    /// Returns a mutable reference to a chunk entry within the region.  
    ///
    /// Do note that these chunk coordinates are local to within the region itself.
    ///
    /// ## Example
    /// ```
    /// # let mut region = silverfish::Region::default();
    /// let mut chunk = region.get_chunk_mut(1, 13)?;
    /// let _ = chunk.set_block((6, 124, 14), "anvil")?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get_chunk_mut<'a>(&'a self, x: u8, z: u8) -> Result<RefMut<'a, (u8, u8), ChunkData>> {
        if x >= mca::REGION_SIZE as u8 || z >= mca::REGION_SIZE as u8 {
            return Err(Error::ChunkOutOfRegionBounds(x, z));
        }

        match self.chunks.get_mut(&(x, z)) {
            Some(ch) => return Ok(ch),
            None if self.config.create_chunk_if_missing => {
                self.chunks.insert(
                    (x, z),
                    ChunkData::new(
                        get_empty_chunk(
                            (x, z),
                            self.region_coords,
                            self.config.world_height.clone(),
                        ),
                        self.config.world_height.clone(),
                    ),
                );
                return Ok(self.chunks.get_mut(&(x, z)).unwrap());
            }
            None => return Err(Error::TriedToModifyMissingChunk(x, z)),
        };
    }

    /// Returns if all chunks inside the region has been generated and is [`Region::REQUIRED_STATUS`]
    pub fn is_region_generated(&self) -> Result<bool> {
        for x in 0..mca::REGION_SIZE as u8 {
            for z in 0..mca::REGION_SIZE as u8 {
                let chunk = self.get_chunk(x, z)?;
                match chunk {
                    Some(ch) => match ch.nbt.string("Status") {
                        Some(status) => {
                            if status.to_str() != Region::REQUIRED_STATUS {
                                return Ok(false);
                            }
                        }
                        None => return Ok(false),
                    },
                    None => return Ok(false),
                };
            }
        }

        Ok(true)
    }
}

impl Debug for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Region({}, {})\n  > chunks: {}\n  > {:?}",
            self.region_coords.0,
            self.region_coords.1,
            self.chunks.len(),
            self.config
        )
    }
}

// returns the bit count for whatever palette_len.
// we dont actually need to calculate anything fancy
// palette_len cant be more than 4096 so we can pre set it up
pub(crate) fn get_block_bit_count(len: usize) -> u32 {
    match len {
        0..=16 => 4, // i believe this should be 0..=16 since the old math had a .max(4) at the end, thus always getting 4 at the minimum
        17..=32 => 5,
        33..=64 => 6,
        65..=128 => 7,
        129..=256 => 8,
        257..=512 => 9,
        513..=1024 => 10,
        1025..=2048 => 11,
        2049..=4096 => 12,
        _ => 13,
    }
}

pub(crate) fn get_biome_bit_count(len: usize) -> u32 {
    match len {
        0 | 1 => 0,
        2 => 1,
        3 | 4 => 2,
        5..=8 => 3,
        9..=16 => 4,
        17..=32 => 5,
        _ => 6,
    }
}

/// Generates an empty chunk with plains as the default biome and air in all sections  
///
/// DataVersion is defaulted to [`Region::MIN_DATA_VERSION`]
pub fn get_empty_chunk(
    coords: (u8, u8),
    region_coords: (i32, i32),
    world_height: Range<isize>,
) -> NbtCompound {
    let mut sections: Vec<NbtCompound> =
        Vec::with_capacity(Config::DEFAULT_WORLD_HEIGHT.clone().count() / ChunkData::WIDTH);
    let (section_start, section_end) = (
        (world_height.start / ChunkData::WIDTH as isize) as i8,
        (world_height.end / ChunkData::WIDTH as isize) as i8,
    );

    // one thing would be to move these to world_height.start / 16 and world_height.end / 16
    // but would be a bit annoying to move around the data to get world_height into this function.
    for y in section_start..section_end {
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
        ("DataVersion".into(), NbtTag::Int(Region::MIN_DATA_VERSION)),
        ("sections".into(), NbtTag::List(NbtList::Compound(sections))),
        ("block_entities".into(), NbtTag::List(NbtList::Empty)),
        ("isLightOn".into(), NbtTag::Byte(0)),
        (
            "xPos".into(),
            NbtTag::Int((region_coords.0 * mca::REGION_SIZE as i32) + coords.0 as i32),
        ),
        (
            "zPos".into(),
            NbtTag::Int((region_coords.1 * mca::REGION_SIZE as i32) + coords.1 as i32),
        ),
    ]);

    chunk
}

/// Converts a piece of global world coordinates to coordinates within it's region.  
///
/// ## Example
/// ```
/// # use silverfish::to_region_local;
/// let coords = (-841, -17, 4821);
/// let local_coords = to_region_local(coords);
/// assert_eq!(local_coords, (183, -17, 213))
/// ```
pub fn to_region_local(coords: (i32, i32, i32)) -> Coords {
    (
        (coords.0 as i32 & (BLOCKS_PER_REGION - 1) as i32) as u32,
        coords.1,
        (coords.2 as i32 & (BLOCKS_PER_REGION - 1) as i32) as u32,
    )
        .into()
}

/// Checks the data_version and status of the chunk if it's valid to operate on
pub(crate) fn is_valid_chunk(chunk: &NbtCompound, coordinate: (u8, u8)) -> Result<()> {
    let status = chunk
        .string("Status")
        .ok_or(Error::MissingNbtTag("Status"))?
        .to_str();
    if status != Region::REQUIRED_STATUS {
        return Err(Error::NotFullyGenerated {
            chunk: coordinate,
            status: status.into_owned(),
        });
    }

    let data_version = chunk
        .int("DataVersion")
        .ok_or(Error::MissingNbtTag("DataVersion"))?;
    if data_version < Region::MIN_DATA_VERSION {
        return Err(Error::UnsupportedVersion {
            chunk: coordinate,
            data_version,
        });
    }

    Ok(())
}

/// Removes unused elements from the palette and "cleans" it.  
pub(crate) fn clean_palette<T>(data: &mut [i64], data_len: usize, palette: &mut Vec<T>) {
    let mut palette_count: Vec<i32> = vec![0; palette.len()];
    for index in data.deref() {
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

    for block in 0..data_len {
        data[block] -= palette_offsets[data[block] as usize];
    }
}

impl Default for Region {
    fn default() -> Self {
        Region::full_empty((0, 0))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn same_region_local_coordinates() {
        let coords = (52, -81, 381);
        let local = to_region_local(coords);
        assert_eq!((52, -81, 381), local);
    }

    #[test]
    fn region_local_coordinates() {
        let coords = (851, 85, -481);
        let local = to_region_local(coords);
        assert_eq!((339, 85, 31), local);
    }

    #[test]
    fn empty_chunk() -> Result<()> {
        let chunk = get_empty_chunk((15, 9), (2, -5), Config::DEFAULT_WORLD_HEIGHT);
        let data_version = chunk
            .int("DataVersion")
            .ok_or(Error::MissingNbtTag("DataVersion"))?;
        let x_pos = chunk.int("xPos").ok_or(Error::MissingNbtTag("xPos"))?;
        let z_pos = chunk.int("zPos").ok_or(Error::MissingNbtTag("zPos"))?;
        let sections = chunk
            .list("sections")
            .ok_or(Error::MissingNbtTag("sections"))?
            .compounds()
            .ok_or(Error::InvalidNbtList("!= compounds"))?;

        assert_eq!(data_version, Region::MIN_DATA_VERSION);
        assert_eq!(x_pos, 79);
        assert_eq!(z_pos, -151);
        assert_eq!(sections.len(), 24);

        Ok(())
    }

    #[test]
    fn block_bit_count() {
        assert_eq!(get_block_bit_count(0), 4);
        assert_eq!(get_block_bit_count(58), 6);
        assert_eq!(get_block_bit_count(1754), 11);
        assert_eq!(get_block_bit_count(8572728), 13);
    }

    #[test]
    fn biome_bit_count() {
        assert_eq!(get_biome_bit_count(0), 0);
        assert_eq!(get_biome_bit_count(4), 2);
        assert_eq!(get_biome_bit_count(7), 3);
        assert_eq!(get_biome_bit_count(25), 5);
        assert_eq!(get_biome_bit_count(8572728), 6);
    }

    #[test]
    fn empty_region() -> Result<()> {
        let region = Region::empty((0, 0));
        assert_eq!(region.chunks.len(), 0);
        assert!(region.get_chunk(0, 0)?.is_none());
        assert_eq!(region.region_coords, (0, 0));
        Ok(())
    }

    #[test]
    fn full_empty_region() {
        let region = Region::default();
        assert_eq!(region.chunks.len(), 1024);
    }

    #[test]
    fn empty_from_nbt_region() {
        let chunks = AHashMap::new();
        let region = Region::from_nbt(chunks, (0, 0));
        assert_eq!(region.chunks.len(), 0);
    }

    #[test]
    fn from_nbt_region() -> Result<()> {
        let mut chunks = AHashMap::new();
        chunks.insert(
            (4, 8),
            get_empty_chunk((4, 8), (0, 0), Config::DEFAULT_WORLD_HEIGHT),
        );

        let region = Region::from_nbt(chunks, (0, 0));
        assert_eq!(region.chunks.len(), 1);
        assert_eq!(region.get_raw_chunk(4, 8)?.unwrap().pending_blocks.len(), 0);
        assert_eq!(
            region
                .get_raw_chunk(4, 8)?
                .unwrap()
                .seen_blocks
                .count_ones(..),
            0
        );
        assert_eq!(region.region_coords, (0, 0));

        Ok(())
    }

    const TEST_REGION: &[u8] = include_bytes!("../tests/full_region.mca");

    #[test]
    fn from_region_region() -> Result<()> {
        let mut bytes = BufReader::new(TEST_REGION);
        let region = Region::from_region(&mut bytes, (0, 0))?;
        assert_eq!(region.chunks.len(), 1024);
        Ok(())
    }

    const EMPTY_REGION: &[u8] = include_bytes!("../tests/empty_region.mca");

    #[test]
    fn write_region() -> Result<()> {
        let mut bytes = BufReader::new(EMPTY_REGION);
        let region = Region::from_region(&mut bytes, (0, 0))?;
        let mut new_region_buf = vec![];
        region.write(&mut new_region_buf)?;

        assert_eq!(new_region_buf, EMPTY_REGION);

        Ok(())
    }

    #[test]
    fn get_chunk() -> Result<()> {
        let mut chunks = AHashMap::new();
        chunks.insert(
            (9, 1),
            get_empty_chunk((9, 1), (0, 0), Config::DEFAULT_WORLD_HEIGHT),
        );

        let region = Region::from_nbt(chunks, (0, 0));

        assert!(region.get_chunk(9, 1)?.is_some());
        assert!(region.get_chunk(1, 9)?.is_none());

        Ok(())
    }

    #[test]
    fn fully_generated() -> Result<()> {
        let region = Region::default();
        assert!(region.is_region_generated()?);
        Ok(())
    }
}
