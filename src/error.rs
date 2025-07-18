use std::io;

use mca::McaError;
use thiserror::Error;

use crate::coordinate::Coordinate;

#[derive(Error, Debug)]
pub enum RustEditError {
    #[error("Failed to convert coordinate to another type")]
    MismatchedCoordinateType(Coordinate),
    #[error("mca failed: {0}")]
    McaError(#[from] McaError),
    #[error("io failed: {0}")]
    IoError(#[from] io::Error),
}
