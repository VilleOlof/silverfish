#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    value: (isize, isize, isize),
    pub _type: CoordinateType,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CoordinateType {
    Default,
    Nether,
    Chunk,
    Region,
}

impl Coordinate {
    pub fn new(x: isize, y: isize, z: isize) -> Self {
        Self {
            value: (x, y, z),
            _type: CoordinateType::Default,
        }
    }

    pub fn new_with_type(x: isize, y: isize, z: isize, _type: CoordinateType) -> Self {
        Self {
            value: (x, y, z),
            _type,
        }
    }

    pub fn inner(&self) -> (isize, isize, isize) {
        self.value
    }

    pub fn x(&self) -> isize {
        self.value.0
    }

    pub fn y(&self) -> isize {
        self.value.1
    }

    pub fn z(&self) -> isize {
        self.value.2
    }

    pub fn as_command_format(&self) -> String {
        format!("{} {} {}", self.x(), self.y(), self.z())
    }

    pub fn normalize(&self) -> Self {
        match self._type {
            CoordinateType::Default => self.clone(),
            CoordinateType::Nether => Self::new_with_type(
                Coordinate::mc(self.value.0, 8f64),
                Coordinate::mc(self.value.1, 8f64),
                Coordinate::mc(self.value.2, 8f64),
                CoordinateType::Default,
            ),
            CoordinateType::Chunk => Self::new_with_type(
                Coordinate::mc(self.value.0, 16f64),
                Coordinate::mc(0, 16f64), // //TODO chunk should never have y?
                Coordinate::mc(self.value.2, 16f64),
                CoordinateType::Default,
            ),
            CoordinateType::Region => Self::new_with_type(
                Coordinate::mc(Coordinate::mc(self.value.0, 32f64), 16f64),
                Coordinate::mc(Coordinate::mc(0, 32f64), 16f64), //TODO region should never have y?
                Coordinate::mc(Coordinate::mc(self.value.2, 32f64), 16f64),
                CoordinateType::Default,
            ),
        }
    }

    #[inline(always)]
    pub fn as_overworld(&self) -> Self {
        self.normalize()
    }

    #[inline(always)]
    pub fn as_nether(&self) -> Self {
        let c = self.normalize();
        Self::new_with_type(
            Coordinate::dc(c.value.0, 8f64),
            Coordinate::dc(c.value.1, 8f64),
            Coordinate::dc(c.value.2, 8f64),
            CoordinateType::Nether,
        )
    }

    #[inline(always)]
    pub fn as_chunk(&self) -> Self {
        let c = self.normalize();
        Self::new_with_type(
            Coordinate::dc(c.value.0, 16f64),
            Coordinate::dc(0, 16f64), //TODO chunk should never have y?
            Coordinate::dc(c.value.2, 16f64),
            CoordinateType::Chunk,
        )
    }

    #[inline(always)]
    pub fn as_region(&self) -> Self {
        let c = self.normalize();
        let chunk = c.as_chunk().inner();
        Self::new_with_type(
            Coordinate::dc(chunk.0, 32f64),
            Coordinate::dc(0, 32f64), //TODO region should never have y?
            Coordinate::dc(chunk.2, 32f64),
            CoordinateType::Region,
        )
    }

    #[inline(always)]
    fn dc(v: isize, m: f64) -> isize {
        (v as f64 / m).floor() as isize
    }

    #[inline(always)]
    fn mc(v: isize, m: f64) -> isize {
        (v as f64 * m).floor() as isize
    }
}