use thiserror::Error;

use crate::coordinate::Coordinate;

#[derive(Error, Debug)]
pub enum RustEditError {
    #[error("Failed to convert coordinate to another type")]
    MismatchedCoordinateType(Coordinate),
}
