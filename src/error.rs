use std::io;

use mca::McaError;
use thiserror::Error;

use crate::coordinate::Coordinate;

#[derive(Error, Debug)]
pub enum RustEditError {
    #[error("Failed to convert coordinate to another type")]
    MismatchedCoordinateType(Coordinate),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    // TODO split WorldError into more types, like
    // look at all the errors in World::flush and stuff and split them
    // into more unique ones than just "string"
    #[error("{0}")]
    WorldError(String),
    #[error("mca failed: {0}")]
    McaError(#[from] McaError),
    // #[error("nbt failed: {0}")]
    // NbtError(#[from] fastnbt::error::Error),
    #[error("nbt failed: {0}")]
    NbtError(String),
    #[error("simdnbt failed: {0}")]
    SimdnbtError(#[from] simdnbt::Error),
    #[error("io failed: {0}")]
    IoError(#[from] io::Error),
    #[error("{0}")]
    Other(String),
}
