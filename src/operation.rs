use crate::{coordinate::Coordinate, nbt::Block, world::Dimension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationData {
    pub dimension: Dimension,
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Setblock {
        coordinate: Coordinate,
        block: Block,
    },
    Fill {
        from: Coordinate,
        to: Coordinate,
        block: Block,
    },
}
