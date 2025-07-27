//! Easily set/get blocks to Minecraft regions

mod config;
mod error;
mod get;
mod nbt;
mod region;
mod set;
mod write;

pub use config::Config;
pub use error::{Error, Result};
pub use nbt::{Block, NbtConversion, NbtString};
pub use region::{Region, get_empty_chunk};
