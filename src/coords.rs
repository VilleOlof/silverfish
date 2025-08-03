/// Coordinates (x, y, z)
///
/// Provides some nice utility functions &
/// implements a few traits for nice conversion
/// between the tuple variant and this.  
///
/// Y is the only signed number since these coords are mostly
/// for region local coordiantes, and y can be negative.  
/// While x and z is always positive.  
#[allow(missing_docs)] // its literally just xyz
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Coords {
    pub x: u32,
    pub y: i32,
    pub z: u32,
}

impl std::fmt::Debug for Coords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, {}, {}", self.x, self.y, self.z)
    }
}

impl From<(u32, i32, u32)> for Coords {
    fn from(value: (u32, i32, u32)) -> Self {
        Self::new(value.0, value.1, value.2)
    }
}

impl From<Coords> for (u32, i32, u32) {
    fn from(value: Coords) -> Self {
        (value.x, value.y, value.z)
    }
}

impl From<&Coords> for (u32, i32, u32) {
    fn from(value: &Coords) -> Self {
        (value.x, value.y, value.z)
    }
}

impl PartialEq<(u32, i32, u32)> for Coords {
    fn eq(&self, other: &(u32, i32, u32)) -> bool {
        &self.as_tuple() == other
    }
}

impl PartialEq<Coords> for (u32, i32, u32) {
    fn eq(&self, other: &Coords) -> bool {
        &other.as_tuple() == self
    }
}

impl Coords {
    /// Wraps xyz into a [`Coords`] struct.  
    pub fn new(x: u32, y: i32, z: u32) -> Self {
        Self { x, y, z }
    }

    /// Returns the coordinates as `(u32, i32, u32)`
    pub fn as_tuple(&self) -> (u32, i32, u32) {
        self.into()
    }
}
