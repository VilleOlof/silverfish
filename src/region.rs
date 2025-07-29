//! `region` contains the core [`Region`] struct used to set/get blocks within the specified Region.  
//!
//! Contains functions for constructing a [`Region`] and writing itself to a specified buffer.  

use crate::{
    biome::BiomeCellWithId,
    config::Config,
    error::{Error, Result},
    nbt::Block,
};
use ahash::AHashMap;
use fixedbitset::FixedBitSet;
use mca::{CompressionType, RegionIter, RegionReader, RegionWriter};
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound, NbtList, NbtTag};
use std::{
    fmt::Debug,
    io::{Cursor, Read, Write},
    ops::{Deref, RangeInclusive},
};

pub(crate) type BlockBuffer = AHashMap<(u8, u8), AHashMap<i8, Vec<BlockWithCoordinate>>>;
pub(crate) type BiomeBuffer = AHashMap<(u8, u8), AHashMap<i8, Vec<BiomeCellWithId>>>;

/// An in-memory region to read and write blocks to the chunks within.  
#[derive(Clone)]
pub struct Region {
    /// The chunks within the Region, mapped to their coordinates
    pub chunks: AHashMap<(u8, u8), NbtCompound>,
    /// Config on how it should handle certain scenarios
    pub config: Config,
    /// Coordinates for this specific region
    pub region_coords: (i32, i32),

    /// buffered blocks that is about to be written to `chunks`
    pub(crate) pending_blocks: BlockBuffer,
    /// blocks we've already pushed to `pending_blocks` to avoid duplicate coordinate blocks
    pub(crate) seen_blocks: FixedBitSet,

    /// buffered biomes that is about to be written to `chunks`
    pub(crate) pending_biomes: BiomeBuffer,
    /// biomes we've already pushed to `pending_biomes` to avoid duplicate biome cells
    pub(crate) seen_biomes: FixedBitSet,
}

/// Just a [`Block`] but with a set of coordinates attached to them.  
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWithCoordinate {
    pub coordinates: (u32, i32, u32),
    pub block: Block,
}

impl Region {
    /// Whatever status the chunks needs to be to allow modification.  
    pub(crate) const REQUIRED_STATUS: &'static str = "minecraft:full";
    /// the minimum dataversion that this crate works with.  
    ///
    /// This is due the massive structural changes in how the nbt is stored that was introduced in `21w39a` & `21w43a`
    pub const MIN_DATA_VERSION: i32 = 2860;

    // some constants for the FixedBitSet & indexes
    pub(crate) const REGION_X_Z_WIDTH: usize = 512;
    pub(crate) const REGION_Y_MIN: isize = -64;
    pub(crate) const REGION_Y_MAX: isize = 320;
    pub(crate) const REGION_CHUNK_SIZE: u8 = 32;
    pub(crate) const REGION_Y_WIDTH: usize = (Self::REGION_Y_MAX - Self::REGION_Y_MIN) as usize;

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
    /// ```no_run
    /// let mut region = Region::full_empty((0, 0));
    /// region.set_world_height(128..=320);
    /// ```
    pub fn set_world_height(&mut self, range: RangeInclusive<isize>) {
        self.seen_biomes = Self::get_default_biome_bitset(range.clone());
        self.pending_biomes.clear(); // need to reset because we reset bitset
        self.config.world_height = range;
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
            chunks: AHashMap::new(),
            seen_blocks: Self::get_default_block_bitset(),
            seen_biomes: Self::get_default_biome_bitset(config.world_height.clone()),
            pending_blocks: AHashMap::new(),
            pending_biomes: AHashMap::new(),
            region_coords,
            config,
        }
    }

    /// Creates a full [`Region`] with empty chunks in it.  
    pub fn full_empty(region_coords: (i32, i32)) -> Self {
        let mut chunks = AHashMap::new();

        for x in 0..mca::REGION_SIZE as u8 {
            for z in 0..mca::REGION_SIZE as u8 {
                chunks.insert((x, z), get_empty_chunk((x, z), region_coords));
            }
        }

        Self::from_nbt(chunks, region_coords)
    }

    /// Creates a new [`Region`] with chunks from `chunks`
    pub fn from_nbt(chunks: AHashMap<(u8, u8), NbtCompound>, region_coords: (i32, i32)) -> Self {
        let config = Config::default();

        Self {
            chunks,
            seen_blocks: Self::get_default_block_bitset(),
            seen_biomes: Self::get_default_biome_bitset(config.world_height.clone()),
            pending_blocks: AHashMap::new(),
            pending_biomes: AHashMap::new(),
            region_coords,
            config,
        }
    }

    /// Creates a [`Region`] from an already existing region
    ///
    /// ## Example
    /// ```no_run
    /// let mut region = Region::from_region(&mut File::open("r.0.0.mca")?)?;
    /// ```
    pub fn from_region<R: Read>(reader: &mut R, region_coords: (i32, i32)) -> Result<Self> {
        // TODO could look at an average region and with_capacity on that?
        let mut bytes = vec![];
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
    /// ```no_run
    /// let mut buf = vec![];
    /// region.write(&mut buf)?;
    /// ```
    pub fn write<W: Write>(self, writer: &mut W) -> Result<()> {
        let mut region_writer = RegionWriter::new();

        for ((x, z), chunk_nbt) in self.chunks {
            let mut raw_nbt = vec![];
            let wrapped = Nbt::Some(BaseNbt::new("", chunk_nbt));
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
    /// ```no_run
    /// let chunk = region.get_chunk(5, 17)?;
    /// ```
    pub fn get_chunk(&self, x: u8, z: u8) -> Result<Option<&NbtCompound>> {
        if x >= Self::REGION_CHUNK_SIZE || z >= Self::REGION_CHUNK_SIZE {
            return Err(Error::ChunkOutOfRegionBounds(x, z));
        }

        Ok(self.chunks.get(&(x, z)))
    }
}

impl Debug for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Region({}, {})\n  > chunks: {}\n  > buffered blocks: {}\n  > {:?}",
            self.region_coords.0,
            self.region_coords.1,
            self.chunks.len(),
            self.pending_blocks.len(),
            self.config
        )
    }
}

// returns the bit count for whatever palette_len.
// we dont actually need to calculate anything fancy
// palette_len cant be more than 4096 so we can pre set it up
pub(crate) fn get_bit_count(len: usize) -> u32 {
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

/// Generates an empty chunk with plains as the default biome and air in all sections  
///
/// DataVersion is defaulted to [`Region::MIN_DATA_VERSION`]
pub fn get_empty_chunk(coords: (u8, u8), region_coords: (i32, i32)) -> NbtCompound {
    let mut sections: Vec<NbtCompound> = Vec::with_capacity(24);

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
        ("DataVersion".into(), NbtTag::Int(Region::MIN_DATA_VERSION)),
        ("sections".into(), NbtTag::List(NbtList::Compound(sections))),
        ("block_entities".into(), NbtTag::List(NbtList::Empty)),
        ("isLightOn".into(), NbtTag::Byte(0)),
        (
            "xPos".into(),
            NbtTag::Int((region_coords.0 * 32) + coords.0 as i32),
        ),
        (
            "zPos".into(),
            NbtTag::Int((region_coords.1 * 32) + coords.1 as i32),
        ),
    ]);

    chunk
}

/// Converts a piece of global world coordinates to coordinates within it's region.  
///
/// ## Example
/// ```no_run
/// let coords = (-841, -17, 4821);
/// let local_coords = to_region_local(coords);
/// assert_eq!(local_coords, (183, -17, 213))
/// ```
pub fn to_region_local(coords: (i32, i32, i32)) -> (u32, i32, u32) {
    ((coords.0 & 511) as u32, coords.1, (coords.2 & 511) as u32)
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

#[cfg(test)]
mod test {
    use std::io::BufReader;

    use super::*;

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
        let chunk = get_empty_chunk((15, 9), (2, -5));
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
    fn data_bit_count() {
        assert_eq!(get_bit_count(0), 4);
        assert_eq!(get_bit_count(58), 6);
        assert_eq!(get_bit_count(1754), 11);
        assert_eq!(get_bit_count(8572728), 13);
    }

    #[test]
    fn empty_region() {
        let region = Region::empty((0, 0));
        assert_eq!(region.chunks.len(), 0);
        assert_eq!(region.pending_blocks.len(), 0);
        assert_eq!(region.seen_blocks.count_ones(..), 0);
        assert_eq!(region.region_coords, (0, 0));
    }

    #[test]
    fn full_empty_region() {
        let region = Region::full_empty((0, 0));
        assert_eq!(region.chunks.len(), 1024);
    }

    #[test]
    fn empty_from_nbt_region() {
        let chunks = AHashMap::new();
        let region = Region::from_nbt(chunks, (0, 0));
        assert_eq!(region.chunks.len(), 0);
    }

    #[test]
    fn from_nbt_region() {
        let mut chunks = AHashMap::new();
        chunks.insert((4, 8), get_empty_chunk((4, 8), (0, 0)));

        let region = Region::from_nbt(chunks, (0, 0));
        assert_eq!(region.chunks.len(), 1);
        assert_eq!(region.pending_blocks.len(), 0);
        assert_eq!(region.seen_blocks.count_ones(..), 0);
        assert_eq!(region.region_coords, (0, 0));
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
        chunks.insert((9, 1), get_empty_chunk((9, 1), (0, 0)));

        let region = Region::from_nbt(chunks, (0, 0));

        assert!(region.get_chunk(9, 1)?.is_some());
        assert!(region.get_chunk(1, 9)?.is_none());

        Ok(())
    }
}
