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
    #[error("{0}")]
    WorldError(String),
    #[error("mca failed: {0}")]
    McaError(#[from] McaError),
    #[error("nbt failed: {0}")]
    NbtError(#[from] fastnbt::error::Error),
    #[error("io failed: {0}")]
    IoError(#[from] io::Error),
}
