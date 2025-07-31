//! `biome` contains all biome related implementations and functions.  
//! Since the block system is the main focus of this crate,
//! i've just stuffed all biome specific code into this module.  

use crate::{BlockWithCoordinate, NbtString};
#[cfg(test)]
use crate::{Region, Result};
use ahash::AHashMap;

/// Contains the necessarily information to locate an exact biome cell within a [`Region`](crate::Region).  
///
/// Biomes in Minecraft at the lowest size is `4x4x4`, so this specifies the `chunk`, `section` & `cell` within the section.  
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BiomeCell {
    pub chunk: (u8, u8),
    pub section: i8,
    pub cell: (u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BiomeCellWithId {
    pub cell: BiomeCell,
    pub id: NbtString,
}

impl BiomeCell {
    pub(crate) const CELL_SIZE: u8 = 4;

    /// Creates a new [`BiomeCell`] from the required data.  
    ///
    /// ## Example
    /// ```no_run
    /// let cell = BiomeCell::new((4, 1), -1, (1, 1, 3));
    /// ```
    pub fn new(chunk: (u8, u8), section: i8, cell: (u8, u8, u8)) -> Self {
        assert!(
            chunk.0 < 32 && chunk.1 < 32,
            "Chunk coordinates for BiomeCell it outside region"
        );
        assert!(
            cell.0 < 4 && cell.1 < 4 && cell.2 < 4,
            "Biome 'cell' is outside it's section"
        );

        BiomeCell {
            chunk,
            section,
            cell,
        }
    }

    /// Creates a new [`BiomeCell`] based off **region** local coordinates.  
    pub fn from_coordinates(x: u32, y: i32, z: u32) -> Self {
        coordinates_to_biome_cell(x, y, z)
    }

    // TODO
    pub fn to_coordinates(&self) -> (u32, i32, u32) {
        todo!("hook to like the corner closest to 0,0,0 or whatever it comes up to")
    }
}

impl Into<BiomeCell> for ((u8, u8), i8, (u8, u8, u8)) {
    fn into(self) -> BiomeCell {
        BiomeCell::new(self.0, self.1, self.2)
    }
}

impl Into<BiomeCell> for (u32, i32, u32) {
    fn into(self) -> BiomeCell {
        BiomeCell::from_coordinates(self.0, self.1, self.2)
    }
}

impl Into<BiomeCell> for BlockWithCoordinate {
    fn into(self) -> BiomeCell {
        BiomeCell::from_coordinates(self.coordinates.0, self.coordinates.1, self.coordinates.2)
    }
}

/// Converts a set of region local coordinates to it's appropriate biome cell.  
pub fn coordinates_to_biome_cell(x: u32, y: i32, z: u32) -> BiomeCell {
    assert!(x < 512 && z < 512);

    let chunk_coords = (
        (x as f64 / 16f64).floor() as u8,
        (z as f64 / 16f64).floor() as u8,
    );
    let section = (y as f64 / 16f64).floor() as i8;
    let cell_coords = (
        ((x & 15) / 4) as u8,
        ((y & 15) / 4) as u8,
        ((z & 15) / 4) as u8,
    );

    BiomeCell::new(chunk_coords, section, cell_coords)
}

#[derive(Debug)]
pub(crate) struct GetChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: AHashMap<i8, Vec<BiomeCell>>,
}

pub(crate) fn group_cells_into_chunks<C: Into<BiomeCell>>(cells: Vec<C>) -> Vec<GetChunkGroup> {
    let mut map: AHashMap<(u8, u8), AHashMap<i8, Vec<BiomeCell>>> = AHashMap::new();

    for cell in cells.into_iter() {
        let cell: BiomeCell = cell.into();
        map.entry(cell.chunk)
            .or_default()
            .entry(cell.section)
            .or_default()
            .push(cell);
    }

    let mut chunk_groups = Vec::with_capacity(map.len());
    for (coordinate, section_map) in map {
        chunk_groups.push(GetChunkGroup {
            coordinate,
            sections: section_map,
        });
    }

    chunk_groups
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pre_set_biome() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region
            .set_biome((5, 17, 148), "minecraft:cherry_grove")?
            .unwrap();

        assert_eq!(region.get_raw_chunk(0, 9)?.unwrap().pending_biomes.len(), 1);
        assert_eq!(
            region
                .get_raw_chunk(0, 9)?
                .unwrap()
                .seen_biomes
                .count_ones(..),
            1
        );

        Ok(())
    }

    #[test]
    fn set_duplicate_biome() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region
            .set_biome((248, -42, 21), "minecraft:desert")?
            .unwrap();
        let success = region.set_biome((248, -42, 21), "minecraft:desert")?;

        assert_eq!(success, None);
        assert_eq!(
            region.get_raw_chunk(15, 1)?.unwrap().pending_biomes.len(),
            1
        );
        assert_eq!(
            region
                .get_raw_chunk(15, 1)?
                .unwrap()
                .seen_biomes
                .count_ones(..),
            1
        );

        Ok(())
    }

    #[test]
    fn write_biome() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region
            .set_biome(((0, 0), 4, (0, 0, 1)), "minecraft:swamp")?
            .unwrap();
        region.write_biomes()?;

        let swamp = region.get_biome(((0, 0), 4, (0, 0, 1)))?;
        assert_eq!(swamp, "minecraft:swamp");
        let plains = region.get_biome(((0, 0), 4, (0, 0, 0)))?;
        assert_eq!(plains, "minecraft:plains");

        Ok(())
    }

    #[test]
    fn get_biomes() -> Result<()> {
        let region = Region::full_empty((0, 0));
        let biomes = region.get_biomes(vec![(5, 71, 41), (61, 95, 13), (11, 42, 283)])?;
        assert_eq!(biomes.len(), 3);
        assert!(biomes.iter().all(|b| b.id == "minecraft:plains"));

        Ok(())
    }

    #[test]
    fn get_biome() -> Result<()> {
        let region = Region::full_empty((0, 0));
        let biome = region.get_biome(BiomeCell::new((5, 1), 8, (1, 2, 3)))?;
        assert_eq!(biome, "minecraft:plains");

        Ok(())
    }

    #[test]
    fn set_all_biome_cells() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.allocate_biome_buffer(0..32, 0..32, -4..20, 64)?;
        for cx in 0..32 {
            for sy in -4..20 {
                for cz in 0..32 {
                    for bx in 0..4 {
                        for by in 0..4 {
                            for bz in 0..4 {
                                region
                                    .set_biome(((cx, cz), sy, (bx, by, bz)), "minecraft:plains")
                                    .unwrap();
                            }
                        }
                    }
                }
            }
        }

        for x in 0..32 {
            for z in 0..32 {
                assert_eq!(
                    region
                        .get_raw_chunk(x, z)?
                        .unwrap()
                        .seen_biomes
                        .count_zeroes(..),
                    0
                );
            }
        }

        Ok(())
    }

    #[test]
    #[should_panic]
    fn invalid_get_coords() {
        let region = Region::full_empty((0, 0));
        region.get_biome((852, 14, 5212)).unwrap();
    }
}
