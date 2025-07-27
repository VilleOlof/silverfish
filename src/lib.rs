#![doc = include_str!("../readme.md")]

mod config;
mod error;
mod get;
mod nbt;
mod region;
mod set;
mod write;

pub use config::Config;
pub use error::{Error, Result};
pub use nbt::{Block, NbtString};
pub use region::{BlockWithCoordinate, Region, get_empty_chunk, to_region_local};

// TODO Next feature-set should probably be set_biome/get_biome somehow
