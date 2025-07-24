use std::collections::HashMap;

use crate::{
    coordinate::Coordinate,
    operation::{Operation, OperationData},
    world::Dimension,
};

/// A group region with it's coordinate and dimension data
/// along side it's grouped operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionGroup {
    pub dimension: Dimension,
    pub region_coordinate: Coordinate,
    pub chunk_operations: Vec<ChunkGroup>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkGroup {
    pub chunk_coordinate: Coordinate,
    pub section_operations: Vec<SectionGroup>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionGroup {
    pub section_idx: isize,
    pub operations: Vec<Operation>,
}

/// Groups operations   
///
/// - first by their **region [`Coordinate`]** and **[`Dimension`]**
/// - then by **chunk [`Coordinate`]**
/// - and lastly by **section index**
pub fn group_operations(operations: Vec<OperationData>) -> Vec<RegionGroup> {
    let mut map: HashMap<
        Dimension,
        HashMap<Coordinate, HashMap<Coordinate, HashMap<isize, Vec<Operation>>>>,
    > = HashMap::new();

    for data in operations {
        let region_coords = data.operation.get_init_coords().as_region();
        let chunk_coords = data.operation.get_init_coords().as_chunk();
        let section_idx = (data.operation.get_init_coords().y() as f64 / 16f64).floor() as isize;

        map.entry(data.dimension)
            .or_default()
            .entry(region_coords)
            .or_default()
            .entry(chunk_coords)
            .or_default()
            .entry(section_idx)
            .or_default()
            .push(data.operation);
    }

    let mut region_groups = vec![];
    for (dimension, region_map) in map {
        for (region_coordinate, chunk_map) in region_map {
            let mut chunk_groups = vec![];

            for (chunk_coordinate, section_map) in chunk_map {
                let mut section_groups = vec![];

                for (section_idx, operations) in section_map {
                    section_groups.push(SectionGroup {
                        section_idx,
                        operations,
                    });
                }

                chunk_groups.push(ChunkGroup {
                    chunk_coordinate,
                    section_operations: section_groups,
                });
            }

            region_groups.push(RegionGroup {
                dimension: dimension.clone(),
                region_coordinate,
                chunk_operations: chunk_groups,
            });
        }
    }

    region_groups
}
