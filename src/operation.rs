use std::path::PathBuf;

use crate::{
    coordinate::Coordinate,
    nbt::Block,
    world::{Dimension, World},
};

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

impl OperationData {
    #[cfg(not(feature = "spigot"))]
    pub fn region_path(&self, world: &World) -> PathBuf {
        self.dimension.path(&world.path)
    }

    #[cfg(feature = "spigot")]
    pub fn region_path(&self, world: &World) -> PathBuf {
        self.dimension.path(&world.path, &world.world_name)
    }
}
