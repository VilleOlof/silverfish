//! `error` contains the [`Error`] type for this crate and a shorthand [`Result`] type.  

use crate::{BLOCKS_PER_REGION, NbtString, nbt::Block, region::Region};

pub type Result<T> = std::result::Result<T, Error>;

/// Show the [`std::fmt::Display`] of the error to display even further context & info
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Mca(#[from] mca::McaError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Simdnbt(#[from] simdnbt::Error),

    #[error(
        "Coordinates are outside of regions bounds ({0} || {1} >= {width})",
        width = BLOCKS_PER_REGION
    )]
    CoordinatesOutOfRegionBounds(u32, u32),
    #[error("Chunk coordinates are outside of region bounds ({0} || {1} >= 32")]
    ChunkOutOfRegionBounds(u8, u8),
    #[error("No element at the given index: len is {len} but index is {index}")]
    OutOfBounds { len: usize, index: usize },
    #[error("Nbt value at '{0}' was the wrong nbt data type")]
    InvalidNbtType(&'static str),
    #[error("Nbt value at '{0}' was the wrong nbt list type")]
    InvalidNbtList(&'static str),
    #[error("No Nbt value named '{0}'")]
    MissingNbtTag(&'static str),
    #[error("No section found with the Y index {0}")]
    NoSectionFound(i8),
    #[error("No chunk found at {0} {1}")]
    NoChunk(u8, u8),
    #[error("Tried to modify a missing chunk at {0} {1}")]
    TriedToModifyMissingChunk(u8, u8),
    #[error("Tried to modify a chunk that hasn't been fully generated yet: {chunk:?} = {status}")]
    NotFullyGenerated { chunk: (u8, u8), status: String },
    #[error("Tried to update a chunk with a DataVersion({data_version}) that is older than {min} in chunk {chunk:?}", min = Region::MIN_DATA_VERSION)]
    UnsupportedVersion { chunk: (u8, u8), data_version: i32 },
    #[error("Invalid palette index in data: {0}")]
    InvalidPaletteIndex(i64),
    #[error("No palette that matches {0:?}")]
    NotInBlockPalette(Block),
    #[error("No palette that matches {0:?}")]
    NotInBiomePalette(NbtString),
}
