#![doc = include_str!("../readme.md")]
#![feature(str_as_str)]
#![feature(try_find)]

mod biome;
mod chunk;
mod config;
mod data;
mod error;
mod get;
mod nbt;
mod nbt_impls;
mod region;
mod set;
mod write;

pub use biome::{BiomeCell, BiomeCellWithId, coordinates_to_biome_cell};
pub use chunk::ChunkData;
pub use config::Config;
pub use error::{Error, Result};
pub use nbt::{Block, Name, NbtString};
pub use region::{BlockWithCoordinate, Region, get_empty_chunk, to_region_local};
pub const BLOCKS_PER_REGION: u32 = 512;

// re-export `RefMut` under "dashmap"
// since it's really the only type from dashmap the user may want
// that is tied to a function ("get_mut_chunk").
pub mod dashmap {
    pub use dashmap::mapref::one::RefMut;
}
