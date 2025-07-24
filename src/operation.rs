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
    // TODO map section operation, do after setblock/fill
    // probably gonna be one of the most useful operations
    // callback is a nice to mutate list/map of all blocks and their data/coordinate
    // that the callback can modify and then "map" that then gets written to disk
    // should also provide block entity mapping from chunks higher fields?
    // specify from/to or specify a specific chunk? a specific section?
    // Map {
    //     from: Coordinate,
    //     to: Coordinate,
    //     callback: fn(data: BlockData) -> BlockData,
    // },
    // TODO gives the user the entire chunk as nbt so they can do whatever
    // MapChunk {
    //     chunk_x: isize,
    //     chunk_z: isize,
    //     callback: fn(chunk: HashMap<String, Value>) -> HashMap<String, Value>,
    // }
}

#[derive(Debug, Clone)]
pub enum SplitUnit {
    Section,
    Chunk,
    Region,
}

impl SplitUnit {
    #[inline(always)]
    pub fn num<T>(&self) -> T
    where
        T: SplitUnitNum,
    {
        T::from_i32(match self {
            SplitUnit::Section => panic!("Unsupported"),
            SplitUnit::Chunk => 16,
            SplitUnit::Region => 16 * mca::REGION_SIZE as i32,
        })
    }
}

pub trait SplitUnitNum {
    fn from_i32(n: i32) -> Self;
}
impl SplitUnitNum for isize {
    fn from_i32(n: i32) -> Self {
        n as isize
    }
}
impl SplitUnitNum for f64 {
    fn from_i32(n: i32) -> Self {
        n as f64
    }
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

impl Operation {
    pub fn get_init_coords(&self) -> Coordinate {
        (match self {
            Self::Setblock {
                coordinate,
                block: _,
            } => coordinate,
            Self::Fill {
                from,
                to: _,
                block: _,
            } => from,
        })
        .clone()
    }
}
