//! `region` contains the core [`Region`] struct used to set/get blocks within the specified Region.  
//!
//! Contains functions for constructing a [`Region`] and writing itself to a specified buffer.  

use crate::{
    config::Config,
    error::{Error, Result},
    nbt::Block,
};
use fixedbitset::FixedBitSet;
use mca::{CompressionType, RegionIter, RegionReader, RegionWriter};
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound, NbtList, NbtTag};
use std::{
    collections::HashMap,
    fmt::Debug,
    io::{Cursor, Read, Write},
};

// so ive tested filling an entire region with 1 single block
//  (best case scenario for palette cache)
// and got a throughput of `3,564,564` blocks per second*
// on my machine with optimized compiler flags

/// An in-memory region to read and write blocks to the chunks within.  
#[derive(Clone)]
pub struct Region {
    /// The chunks within the Region, mapped to their coordinates
    pub chunks: HashMap<(u8, u8), NbtCompound>,
    /// Config on how it should handle certain scenarios
    pub config: Config,
    /// Coordinates for this specific region
    pub region_coords: (i32, i32),

    /// buffered blocks that is about to be written to `chunks`
    pub(crate) pending_blocks: Vec<BlockWithCoordinate>,
    /// blocks we've already pushed to `pending_blocks` to avoid duplicate coordinate blocks
    pub(crate) seen_blocks: FixedBitSet,
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

    /// Creates an empty [`Region`] with no chunks or anything.  
    ///
    /// [`Config::create_chunk_if_missing`] will set to `true` from this  
    #[inline(always)]
    pub fn empty(region_coords: (i32, i32)) -> Self {
        Self {
            chunks: HashMap::new(),
            pending_blocks: vec![],
            seen_blocks: Self::get_default_bitset(),
            region_coords,
            config: Config {
                create_chunk_if_missing: true,
                ..Default::default()
            },
        }
    }

    /// Creates a full [`Region`] with empty chunks in it.  
    #[inline(always)]
    pub fn full_empty(region_coords: (i32, i32)) -> Self {
        let mut chunks = HashMap::new();

        for x in 0..mca::REGION_SIZE as u8 {
            for z in 0..mca::REGION_SIZE as u8 {
                chunks.insert((x, z), get_empty_chunk((x, z), region_coords));
            }
        }

        Self::from_nbt(chunks, region_coords)
    }

    /// Creates a new [`Region`] with chunks from `chunks`
    #[inline(always)]
    pub fn from_nbt(chunks: HashMap<(u8, u8), NbtCompound>, region_coords: (i32, i32)) -> Self {
        Self {
            chunks,
            pending_blocks: vec![],
            seen_blocks: Self::get_default_bitset(),
            config: Config::default(),
            region_coords,
        }
    }

    /// Creates a [`Region`] from an already existing region
    ///
    /// ## Example
    /// ```no_run
    /// let mut region = Region::from_region(&mut File::open("r.0.0.mca")?)?;
    /// ```
    pub fn from_region<R: Read>(reader: &mut R, region_coords: (i32, i32)) -> Result<Self> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        let region_reader = RegionReader::new(&bytes)?;

        let mut chunks = HashMap::new();
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
#[inline(always)]
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
