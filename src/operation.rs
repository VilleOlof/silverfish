use std::path::PathBuf;

use crate::{
    coordinate::Coordinate,
    error::RustEditError,
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
    fn num<T>(&self) -> T
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

trait SplitUnitNum {
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

    pub fn split_fill_into(
        operation: &Operation,
        unit: SplitUnit,
    ) -> Result<Vec<Operation>, RustEditError> {
        if let SplitUnit::Section = unit {
            return Operation::split_fill_into_sections(operation);
        }

        let (from, to, block) = match operation {
            Self::Fill { from, to, block } => (from, to, block),
            _ => {
                return Err(RustEditError::InvalidOperation(
                    "Tried to map a non-fill operation into unit areas".into(),
                ));
            }
        };

        let (a_from, a_to) = match unit {
            SplitUnit::Chunk => (from.as_chunk(), to.as_chunk()),
            SplitUnit::Region => (from.as_region(), to.as_region()),
            _ => panic!("Shouldn't be possible to reach this"),
        };

        // if the same area, we just return it as no mapping is needed
        if a_from == a_to {
            return Ok(vec![operation.clone()]);
        }
        let mut operations = vec![];

        let start_corner = Coordinate::new_with_type(
            a_from.x() * unit.num::<isize>(),
            a_from.y() * unit.num::<isize>(),
            a_from.z() * unit.num::<isize>(),
            a_from._type,
        );

        let (mut x, mut z) = (from.x(), from.z());
        #[rustfmt::skip]
        let (delta_x_sign, delta_z_sign) = (
            (to.x() - x).signum(),
            (to.z() - z).signum()
        );
        #[rustfmt::skip]
        let (normalized_delta_x_sign, normalized_delta_z_sign) = (
            ((delta_x_sign + 1) / 2), 
            ((delta_z_sign + 1) / 2)
        );

        let (mut to_x, mut to_z) = (
            start_corner.x() + (unit.num::<isize>() - 1) * normalized_delta_x_sign,
            start_corner.z() + (unit.num::<isize>() - 1) * normalized_delta_z_sign,
        );
        #[rustfmt::skip]
        let (same_x_area, same_z_area) = (
            (to_x as f64 / unit.num::<f64>()).floor() as isize == (to.x() as f64 / unit.num::<f64>()).floor() as isize,
            (to_z as f64 / unit.num::<f64>()).floor() as isize == (to.z() as f64 / unit.num::<f64>()).floor() as isize,
        );

        if same_z_area {
            to_z = to.z();
        }
        if same_x_area {
            to_x = to.x();
        }

        while (to.x() - x).signum() == delta_x_sign {
            while (to.z() - z).signum() == delta_z_sign {
                operations.push(Operation::Fill {
                    from: Coordinate::new(x, from.y(), z),
                    to: Coordinate::new(to_x, to.y(), to_z),
                    block: block.clone(),
                });

                z = ((z as f64 / unit.num::<f64>()).floor() as isize + delta_z_sign)
                    * unit.num::<isize>()
                    + (unit.num::<isize>() - 1) * (1 - normalized_delta_z_sign);
                to_z = ((to_z as f64 / unit.num::<f64>()).floor() as isize + delta_z_sign)
                    * unit.num::<isize>()
                    + (unit.num::<isize>() - 1) * normalized_delta_z_sign;

                // edge case?
                if z == to_z {
                    operations.push(Operation::Fill {
                        from: Coordinate::new(x, from.y(), z),
                        to: Coordinate::new(to_x, to.y(), to_z),
                        block: block.clone(),
                    });
                }

                to_z = if ((start_corner.z() - to_z).abs() < (start_corner.z() - to.z()).abs())
                    && !same_z_area
                {
                    to_z
                } else {
                    to.z()
                };
            }

            z = from.z();
            to_z = start_corner.z() + (unit.num::<isize>() - 1) * normalized_delta_z_sign;
            if same_z_area {
                to_z = to.z();
            }

            x = ((x as f64 / unit.num::<f64>()).floor() as isize + delta_x_sign)
                * unit.num::<isize>()
                + (unit.num::<isize>() - 1) * (1 - normalized_delta_x_sign);
            let potential_x = ((to_x as f64 / unit.num::<f64>()).floor() as isize + delta_x_sign)
                * unit.num::<isize>()
                + (unit.num::<isize>() - 1) * normalized_delta_x_sign;
            let new_to_x: isize = if (start_corner.x() - potential_x).abs()
                < (start_corner.x() - to.x()).abs()
                && !same_x_area
            {
                potential_x
            } else {
                to.x()
            };

            if new_to_x == to_x {
                break;
            } else {
                to_x = new_to_x;
            }
        }

        Ok(operations)
    }

    fn split_fill_into_sections(operation: &Operation) -> Result<Vec<Operation>, RustEditError> {
        let (from, to, block) = match operation {
            Self::Fill { from, to, block } => (from, to, block),
            _ => {
                return Err(RustEditError::InvalidOperation(
                    "Tried to map a non-fill operation into unit areas".into(),
                ));
            }
        };

        let (sect_i_from, sect_i_to) = (
            (from.y() as f64 / 16f64).floor() as isize,
            (to.y() as f64 / 16f64).floor() as isize
        );

        // if same section, just return
        if sect_i_from == sect_i_to {
            return Ok(vec![operation.clone()]);
        }
        // how many unique sections there is
        let section_len = (sect_i_from + 1).abs_diff(sect_i_to);
        let mut ops = Vec::with_capacity(section_len);

        // swap the "from" & "to" to which is higher for top
        let (top, bottom) = if from.y() > to.y() {
            (from, to)
        } else {
            (to, from)
        };

        // first top section
        ops.push(Operation::Fill {
            from: Coordinate::new(top.x(), top.y(), top.z()),
            to: Coordinate::new(bottom.x(), top.y() - (top.y() % 16), bottom.z()),
            block: block.clone()
        });

        let (high_sect, low_sect) = if sect_i_from > sect_i_to {
            (sect_i_from, sect_i_to)
        }
        else {
            (sect_i_to, sect_i_from)
        };
        // if theres any more inbetween full section, we fill them here
        for idx in (low_sect + 1)..high_sect {
            ops.push(Operation::Fill {
                from: Coordinate::new(top.x(), idx * 16 + 15, top.z()),
                to: Coordinate::new(bottom.x(), idx * 16, bottom.z()),
                block: block.clone() 
            });
        }
        // last bottom section
        ops.push(Operation::Fill {
            from: Coordinate::new(top.x(), bottom.y() + (15 - (bottom.y() % 16)), top.z()),
            to: Coordinate::new(bottom.x(), bottom.y(), bottom.z()),
            block: block.clone()
        });
        
        Ok(ops)
    }
}