#![doc = include_str!("../readme.md")]
#![feature(str_as_str)]
#![feature(try_find)]

mod biome;
mod config;
mod data;
mod error;
mod get;
mod nbt;
mod nbt_impls;
mod region;
mod set;
mod write;

pub use biome::{BiomeCell, coordinates_to_biome_cell};
pub use config::Config;
pub use error::{Error, Result};
pub use nbt::{Block, Name, NbtString};
pub use region::{BlockWithCoordinate, Region, get_empty_chunk, to_region_local};
