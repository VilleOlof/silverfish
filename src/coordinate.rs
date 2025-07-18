#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    value: (isize, isize, isize),
    _type: CoordinateType,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CoordinateType {
    Default,
    Nether,
    Chunk,
    Region,
}

impl Coordinate {
    pub fn new(x: isize, y: isize, z: isize, _type: CoordinateType) -> Self {
        Self {
            value: (x, y, z),
            _type,
        }
    }

    pub fn inner(&self) -> (isize, isize, isize) {
        self.value
    }

    pub fn normalize(&self) -> Self {
        match self._type {
            CoordinateType::Default => self.clone(),
            CoordinateType::Nether => Self::new(
                Coordinate::mc(self.value.0, 8f64),
                Coordinate::mc(self.value.1, 8f64),
                Coordinate::mc(self.value.2, 8f64),
                CoordinateType::Default,
            ),
            CoordinateType::Chunk => Self::new(
                Coordinate::mc(self.value.0, 16f64),
                Coordinate::mc(self.value.1, 16f64),
                Coordinate::mc(self.value.2, 16f64),
                CoordinateType::Default,
            ),
            CoordinateType::Region => Self::new(
                Coordinate::mc(Coordinate::mc(self.value.0, 32f64), 16f64),
                Coordinate::mc(Coordinate::mc(self.value.1, 32f64), 16f64),
                Coordinate::mc(Coordinate::mc(self.value.2, 32f64), 16f64),
                CoordinateType::Default,
            ),
        }
    }

    pub fn as_overworld(&self) -> Self {
        self.clone()
    }

    pub fn as_nether(&self) -> Self {
        let c = self.normalize();
        Self::new(
            Coordinate::dc(c.value.0, 8f64),
            Coordinate::dc(c.value.1, 8f64),
            Coordinate::dc(c.value.2, 8f64),
            CoordinateType::Nether,
        )
    }

    pub fn as_chunk(&self) -> Self {
        let c = self.normalize();
        Self::new(
            Coordinate::dc(c.value.0, 16f64),
            Coordinate::dc(c.value.1, 16f64),
            Coordinate::dc(c.value.2, 16f64),
            CoordinateType::Chunk,
        )
    }

    pub fn as_region(&self) -> Self {
        let c = self.normalize();
        let chunk = c.as_chunk().inner();
        Self::new(
            Coordinate::dc(chunk.0, 32f64),
            Coordinate::dc(chunk.1, 32f64),
            Coordinate::dc(chunk.2, 32f64),
            CoordinateType::Region,
        )
    }

    fn dc(v: isize, m: f64) -> isize {
        (v as f64 / m).floor() as isize
    }

    fn mc(v: isize, m: f64) -> isize {
        (v as f64 * m).floor() as isize
    }
}
