//! `biome` contains all biome related implementations and functions.  
//! Since the block system is the main focus of this crate,
//! i've just stuffed all biome specific code into this module.  

use crate::{
    BlockWithCoordinate, Error, NbtString, Region, Result,
    data::{decode_data, encode_data},
    get_empty_chunk,
    region::{BiomeBuffer, clean_palette, is_valid_chunk},
};
use ahash::AHashMap;
use fixedbitset::FixedBitSet;
use simdnbt::owned::{NbtCompound, NbtList};
use std::ops::{Range, RangeInclusive};

impl Region {
    pub(crate) const BIOME_DATA_LEN: usize = 64;

    /// Returns the biome at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`crate::to_region_local`].  
    ///
    /// ## Example
    /// ```no_run
    /// let biome = region.get_biome((82, 62, 7))?;
    /// assert_eq!(biome, "minecraft:meadow");
    /// ```
    pub fn get_biome<C: Into<BiomeCell>>(&self, cell: C) -> Result<NbtString> {
        self.get_biomes(vec![cell]).map(|mut b| b.swap_remove(0).id)
    }

    /// Returns the biomes at the specified coordinates *(local to within the region)*.  
    ///
    /// Global coordinates can be converted to region local via [`crate::to_region_local`].  
    ///
    /// ## Example
    /// ```no_run
    /// let biomes = region.get_biomes(vec![(52, 85, 152), (94, -4, 481)])?;
    /// assert_eq!(biomes.len(), 2);
    /// ```
    pub fn get_biomes<C: Into<BiomeCell>>(&self, cells: Vec<C>) -> Result<Vec<BiomeCellWithId>> {
        let mut found_biomes = Vec::with_capacity(cells.len());
        let mut groups = group_cells_into_chunks(cells);

        for chunk_group in groups.iter_mut() {
            let chunk = self
                .chunks
                .get(&chunk_group.coordinate)
                .ok_or(Error::NoChunk(
                    chunk_group.coordinate.0,
                    chunk_group.coordinate.1,
                ))?;

            let sections = chunk
                .list("sections")
                .ok_or(Error::MissingNbtTag("sections"))?
                .compounds()
                .ok_or(Error::InvalidNbtType("sections"))?;

            for section in sections {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let biomes_to_get = match chunk_group.sections.remove(&y) {
                    Some(blocks) => blocks,
                    None => continue,
                };

                let state = section
                    .compound("biomes")
                    .ok_or(Error::MissingNbtTag("biomes"))?;

                let data = state.long_array("data");
                let palette = state
                    .list("palette")
                    .ok_or(Error::MissingNbtTag("palette"))?
                    .strings()
                    .ok_or(Error::InvalidNbtType("palette"))?;

                let mut indexes: [i64; Region::BIOME_DATA_LEN] = [0; Region::BIOME_DATA_LEN];
                decode_data(&mut indexes, get_bit_count(palette.len()), data);

                for cell in biomes_to_get {
                    let (x, y, z) = (cell.cell.0, cell.cell.1, cell.cell.2);
                    let index = (x
                        + z * BiomeCell::CELL_SIZE
                        + y * BiomeCell::CELL_SIZE * BiomeCell::CELL_SIZE)
                        as usize;

                    let palette_index: usize =
                        *indexes.get(index as usize).ok_or(Error::OutOfBounds {
                            len: indexes.len(),
                            index: index as usize,
                        })? as usize;
                    let id = palette.get(palette_index).ok_or(Error::OutOfBounds {
                        len: palette.len(),
                        index: palette_index,
                    })?;

                    found_biomes.push(BiomeCellWithId {
                        cell,
                        id: NbtString::from_mutf8str(Some(id))
                            .ok_or(Error::InvalidNbtType("biome palette id isn't a string"))?,
                    });
                }
            }
        }

        Ok(found_biomes)
    }

    /// Biomes in Minecraft are stored in 4x4x4 cells within each section.  
    ///
    ///
    /// To specify which cell you want to change the biome of, you'll need to specify:  
    /// - The chunk coordinates *(local to the region, 0..=31)*
    /// - The section Y index *(-4..=19)*
    /// - The cell coordinates within the section *(0..=3)*
    ///
    /// You can use [`coordinates_to_cell`] to convert region local coordinates to the needed data.  
    ///
    /// Alternatively, you can just give it the coordinates directly since `(u32, i32, u32)` implements `Into<BiomeCell>`
    ///
    /// ## Example
    /// ```no_run
    /// let _ = region.set_biome(((5, 19), 6, (2, 3)), "minecraft:cherry_grove")
    /// // to actually write the biomes to the NBT
    /// region.write_biomes()?;
    /// ```
    pub fn set_biome<C: Into<BiomeCell>, B: Into<NbtString>>(
        &mut self,
        cell: C,
        biome: B,
    ) -> Option<()> {
        let cell: BiomeCell = cell.into();
        let biome: NbtString = biome.into();

        let index = self.get_biome_index(&cell);
        if !self.seen_biomes.contains(index) {
            self.seen_biomes.insert(index);

            self.pending_biomes
                .entry(cell.chunk)
                .or_default()
                .entry(cell.section)
                .or_default()
                .push(BiomeCellWithId { cell, id: biome });
            return Some(());
        }

        None
    }

    /// Takes all pending biomes changes and writes them to the chunk NBT
    ///
    /// Clears the internal buffer of biomes instantly.  
    /// So if this function fails, do note that any biomes added via [`Self::set_biome`] previous to this call will get cleared.  
    pub fn write_biomes(&mut self) -> Result<()> {
        // reset seen_biomes since we have already reset pending_biomes on the consumer side.
        self.seen_biomes.clear();

        // keep these here to hold onto their memory allocations.
        let mut old_indexes: [i64; Region::BIOME_DATA_LEN] = [0; Region::BIOME_DATA_LEN];
        let mut cached_palette_indexes: AHashMap<NbtString, i64> = AHashMap::new();

        for (chunk_coords, section_map) in self.pending_biomes.iter_mut() {
            // this is the exact same get chunk code as `write_blocks` but it was annoying
            // to get into a function due to the "multiple" mutable references.
            // could do unsafe if were sure
            let chunk = match self.chunks.get_mut(&chunk_coords) {
                Some(chunk) => chunk,
                None if self.config.create_chunk_if_missing => {
                    self.chunks.insert(
                        *chunk_coords,
                        get_empty_chunk(*chunk_coords, self.region_coords),
                    );
                    self.chunks
                        .get_mut(&chunk_coords)
                        .ok_or(Error::NoChunk(chunk_coords.0, chunk_coords.1))?
                }
                None => {
                    return Err(Error::TriedToModifyMissingChunk(
                        chunk_coords.0,
                        chunk_coords.1,
                    ));
                }
            };

            is_valid_chunk(&chunk, *chunk_coords)?;

            let sections: &mut Vec<NbtCompound> = match chunk
                .list_mut("sections")
                .ok_or(Error::MissingNbtTag("sections"))?
            {
                NbtList::Compound(c) => c,
                _ => return Err(Error::InvalidNbtList("sections")),
            };

            for section in sections {
                let y = section.byte("Y").ok_or(Error::MissingNbtTag("Y"))?;
                let pending_biomes = match section_map.remove(&y) {
                    Some(pending_biomes) => pending_biomes,
                    None => continue,
                };

                cached_palette_indexes.clear();

                let state = section
                    .compound_mut("biomes")
                    .ok_or(Error::MissingNbtTag("biomes"))?;

                let state_ptr = state as *mut NbtCompound;
                let palette = unsafe {
                    match (*state_ptr)
                        .list_mut("palette")
                        .ok_or(Error::MissingNbtTag("palette"))?
                    {
                        NbtList::String(c) => c,
                        _ => return Err(Error::InvalidNbtList("palette")),
                    }
                };
                let data = unsafe { (*state_ptr).long_array("data") };

                let data_len = decode_data(&mut old_indexes, get_bit_count(palette.len()), data);

                for biome in pending_biomes {
                    let palette_index = match cached_palette_indexes.get(&biome.id) {
                        Some(idx) => *idx,
                        None => {
                            let is_in_palette = palette.iter().any(|b| b == biome.id);

                            if !is_in_palette {
                                palette.push(biome.id.clone().to_mutf8string());
                            }

                            let palette_index = palette
                                .iter()
                                .position(|b| b == biome.id)
                                .ok_or(Error::NotInBiomePalette(biome.id.clone()))?
                                as i64;
                            cached_palette_indexes.insert(biome.id, palette_index);
                            palette_index
                        }
                    };

                    let (x, y, z) = (biome.cell.cell.0, biome.cell.cell.1, biome.cell.cell.2);
                    let index = (x
                        + z * BiomeCell::CELL_SIZE
                        + y * BiomeCell::CELL_SIZE * BiomeCell::CELL_SIZE)
                        as usize;

                    old_indexes[index] = palette_index;
                }

                clean_palette(&mut old_indexes, data_len, palette);

                if palette.len() == 1 {
                    // if theres only 1 palette we can remove the data
                    state.remove("data");
                    continue;
                }

                encode_data(get_bit_count(palette.len()), &old_indexes, data_len, state);
            }
        }

        self.pending_biomes.clear();

        Ok(())
    }

    /// Due to how the internal buffer is grouped for batching later on.  
    /// You can only define `chunk` and `section` ranges and how many biome cells within each section.  
    ///
    /// Overwrites the already existing internal biome buffer.  
    ///
    /// Useful if you know which areas in your region that you'll modify.  
    ///
    /// ## Example
    /// ```no_run
    /// let mut region = Region::full_empty((0, 0));
    /// region.allocate_block_buffer(0..16, 4..8, 1..3, 32);
    /// ```
    pub fn allocate_biome_buffer(
        &mut self,
        chunks_x: Range<u8>,
        chunk_z: Range<u8>,
        sections: Range<i8>,
        cells_per_section: usize,
    ) {
        let buffer = AHashMap::with_capacity(chunks_x.clone().count() * chunk_z.clone().count());

        for x in chunks_x {
            for z in chunk_z.clone() {
                for y in sections.clone() {
                    self.pending_biomes
                        .entry((x, z))
                        .or_insert_with(|| AHashMap::with_capacity(sections.clone().count()))
                        .entry(y)
                        .or_insert_with(|| Vec::with_capacity(cells_per_section));
                }
            }
        }

        self.set_internal_biome_buffer(buffer);
    }

    /// Sets the internal biome buffer.  
    ///
    /// Overwrites any and all data related to the buffer.  
    pub fn set_internal_biome_buffer(&mut self, buffer: BiomeBuffer) {
        self.pending_biomes = buffer;
        self.seen_blocks.clear();
    }

    /// Returns the index for a biome in the [`Self::seen_biomes`] bitset based of it's cell coordinates  
    pub(crate) fn get_biome_index(&self, cell: &BiomeCell) -> usize {
        let lowest_section_y: i8 = (self.config.world_height.start() / 16) as i8;
        let section_count = (self.config.world_height.clone().count() / 16) as usize;
        let cell_size = BiomeCell::CELL_SIZE as usize;
        let (bx, by, bz) = (
            cell.cell.0 as usize,
            cell.cell.1 as usize,
            cell.cell.2 as usize,
        );

        bz + by * cell_size
            + bx * cell_size * cell_size
            + (cell_size * cell_size * cell_size) * (cell.section - lowest_section_y) as usize
            + (cell_size * cell_size * cell_size * section_count) * cell.chunk.0 as usize
            + (cell_size
                * cell_size
                * cell_size
                * section_count
                * Region::REGION_CHUNK_SIZE as usize)
                * cell.chunk.1 as usize
    }

    /// Returns a [`FixedBitSet`] with a default capacity that holds an entire regions biomes for check  
    pub(crate) fn get_default_biome_bitset(world_height: RangeInclusive<isize>) -> FixedBitSet {
        // (cx * y * cz) * (bw * bw * bw)
        let c = Region::REGION_CHUNK_SIZE as usize;
        let section_count = world_height.count() / 16;
        let b = BiomeCell::CELL_SIZE as usize;
        FixedBitSet::with_capacity((c * section_count * c) * (b * b * b))
    }
}

/// Contains the necessarily information to locate an exact biome cell within a [`Region`].  
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
    const CELL_SIZE: u8 = 4;

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

pub(crate) fn get_bit_count(len: usize) -> u32 {
    match len {
        0 | 1 => 0,
        2 => 1,
        3 | 4 => 2,
        5..=8 => 3,
        9..=16 => 4,
        17..=32 => 5,
        _ => 6,
    }
}

#[derive(Debug)]
pub(crate) struct GetChunkGroup {
    pub coordinate: (u8, u8),
    pub sections: AHashMap<i8, Vec<BiomeCell>>,
}

fn group_cells_into_chunks<C: Into<BiomeCell>>(cells: Vec<C>) -> Vec<GetChunkGroup> {
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
        region.set_biome((5, 17, 148), "minecraft:cherry_grove");

        assert_eq!(region.pending_biomes.len(), 1);
        assert_eq!(region.seen_biomes.count_ones(..), 1);

        Ok(())
    }

    #[test]
    fn set_duplicate_biome() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_biome((248, -42, 21), "minecraft:desert");
        let success = region.set_biome((248, -42, 21), "minecraft:desert");

        assert_eq!(success, None);
        assert_eq!(region.pending_biomes.len(), 1);
        assert_eq!(region.seen_biomes.count_ones(..), 1);

        Ok(())
    }

    #[test]
    fn write_biome() -> Result<()> {
        let mut region = Region::full_empty((0, 0));
        region.set_biome(((0, 0), 4, (0, 0, 1)), "minecraft:swamp");
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
    fn set_all_biome_cells() {
        let mut region = Region::full_empty((0, 0));
        region.allocate_biome_buffer(0..32, 0..32, -4..20, 64);
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

        assert_eq!(region.seen_biomes.count_zeroes(..), 0);
    }

    #[test]
    #[should_panic]
    fn invalid_get_coords() {
        let region = Region::full_empty((0, 0));
        region.get_biome((852, 14, 5212)).unwrap();
    }
}
