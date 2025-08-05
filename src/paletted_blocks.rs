//! `paletted_blocks` contains *all* the logic related to well, `PalettedBlocks` that holds blocks
//! gathered from [`get_blocks`](crate::Region::get_blocks) & [`get_block`](crate::Region::get_block).  
//! Theses blocks are lazily collected and just holds references until used.  

use crate::{Block, BlockWithCoordinate, Coords, Error, Result};
use fixedbitset::{FixedBitSet, Ones};
use simdnbt::owned::NbtCompound;
use std::{ops::Range, u32};

/// This can become quite memory hungry when used on bigger areas of blocks at a time.  
///
/// Let's say you're collecting blocks from an entire region.  
/// Before you even insert a [`Block`], you'll have already allocated `384 MB` of memory.  
/// This is due to it needing a [`Vec`] that accounts for all the blocks you may or may not insert.  
#[derive(Debug, Clone, PartialEq)]
pub struct PalettedBlocks<'a> {
    /// Stores references to each palette (and the palette's ref count)
    /// So each NbtCompound should point to a palette nbt list.  
    ///
    /// When a palette reaches 0 references, it gets cleaned from the palette.  
    /// But still exists in whatever [`Region`](crate::Region) this comes from.  
    palette: Vec<(&'a [NbtCompound], usize)>,
    /// The first u16 of the u32 is the palette index in "palette".
    /// The second u16 is the palette_index within that palette
    ///
    /// A palette can only have 4096 blocks since its within a section
    /// And there can max be 24,576 palettes within any given region. (given default world_height)  
    /// You would need a world that is 1024+ blocks tall to reach this limitation.  
    /// and thus a u16 is the least to fit both of these since its like 65,535
    blocks: Vec<u32>,
    /// Has the same size as `blocks` but is just bits.  
    /// If one bit set enabled, it means that one block in `blocks` at that index
    /// is a value other than `u32::MAX`
    /// Used to quickly iterate over actual blocks and check how many blocks  
    /// we are dealing with since counting 1s and 0s with a bitset is much much faster than the vec of `blocks`  
    placed_blocks: FixedBitSet,

    /// The lowest y level, used for indexes
    bottom_y: i32,
    /// The width of the area, used for indexes
    width: u32,
}

// Internal `PalettedBlocks` impls
impl<'a> PalettedBlocks<'a> {
    /// Converts a set of coordinates to an index that can be used in  
    /// - [`blocks`](PalettedBlocks::blocks)  
    /// - [`placed_blocks`](PalettedBlocks::placed_blocks)  
    fn to_index(bottom_y: i32, width: u32, coords: Coords) -> u32 {
        let (x, y, z) = coords.into();
        (y - bottom_y) as u32 * width * width + z * width + x
    }

    /// Converts an index back to it's coordinates.  
    fn to_coords(bottom_y: i32, width: u32, index: u32) -> Coords {
        let y = index / width / width;
        let z = (index - y * width * width) / width;
        let x = index - y * width * width - z * width;
        (x, y as i32 + bottom_y, z).into()
    }

    /// Takes a combined `u32` constructed from [`construct_block_val`](PalettedBlocks::construct_block_val)
    /// and seperates it back into it's `palette` index and the `palette_index` within that.  
    fn deconstruct_block_val(val: u32) -> (u16, u16) {
        (
            (val & u16::MAX as u32) as u16,
            ((val >> 16) & u16::MAX as u32) as u16,
        )
    }

    /// Takes in a `palette` index and a `palette_index` in that actual palette  
    /// and merges them into a single `u32`, since both are `u16`.  
    /// They both fit into a single `u32`, taking half each
    fn construct_block_val(palette: u16, palette_index: u16) -> u32 {
        // limits of this part of the paletted block thingy
        // look at docs for `blocks` field for more info.

        assert!(palette_index < 4096 && palette < u16::MAX);
        ((palette_index as u32) << 16) + palette as u32
    }

    /// Shifts all `palette `indexes from `blocks` that points to an element in `palette`
    /// that is after the specified palette `index`
    fn shift_indexes(&mut self, index: u16) {
        for i in self.placed_blocks.ones() {
            let v = self.blocks[i];

            // we need to deconstruct to get the indexes to check against them.
            let (palette, pi) = PalettedBlocks::deconstruct_block_val(v);
            if palette > index {
                // and if we shift it, we of course need to reconstruct it but changed
                self.blocks[i] = PalettedBlocks::construct_block_val(palette - 1, pi)
            }
        }
    }
}

impl<'a> PalettedBlocks<'a> {
    /// Creates a new [`Blocks`] to hold a list of blocks lazily until read.  
    ///
    /// Shouldn't ever be touched outside of [`get_blocks`](crate::Region::get_blocks) functions
    ///
    /// Takes in the `world_height` and the `width` of the area as it's init arguments.  
    /// Uses these to calculate the total area needed to allocate `(size * world_height.count() * size)`
    ///
    /// ***Note:** Worlds taller than 1024 will panic, see it's panic message for more.*
    ///
    /// ## Example
    /// ```
    /// # use silverfish::PalettedBlocks;
    /// // Creates a `PalettedBlocks` region that is exactly one chunk
    /// let blocks = PalettedBlocks::new(-64..320, 16);
    /// ```
    pub fn new(world_height: Range<isize>, width: usize) -> Self {
        let total_world_height = world_height.clone().count();

        if total_world_height >= 1024 {
            unimplemented!(
                "Worlds taller than 1024 blocks are currently not supported due to u16 limitations and me not wanting to take up another GB of your memory at this point.\nMake an issue at 'https://github.com/VilleOlof/silverfish' and i will fix this with either a feature flag or just taking the hit.  "
            );
        }

        Self {
            palette: Vec::with_capacity(4),
            blocks: vec![u32::MAX; width * total_world_height * width],
            placed_blocks: FixedBitSet::with_capacity(width * total_world_height * width),
            bottom_y: world_height.start as i32,
            width: width as u32,
        }
    }

    /// Returns how many *real* blocks exists.  
    pub fn len(&self) -> usize {
        self.placed_blocks.count_ones(..)
    }

    /// Checks if any given [`Block`] is contained within any of the internal palettes.  
    pub fn contains(&self, block: &Block) -> bool {
        for (_, pal_block) in self {
            if &pal_block == block {
                return true;
            }
        }

        false
    }

    /// Converts all the blocks into a list of [`BlockWithCoordinate`]
    pub fn get_all(&self) -> Vec<BlockWithCoordinate> {
        self.into_iter()
            .map(|(coordinates, block)| BlockWithCoordinate {
                coordinates: coordinates,
                block,
            })
            .collect()
    }

    /// Inserts a `palette_index` reference at a specific `palette` index.  
    ///
    /// This function expects that you know what you're doing and that the given `palette` index  
    /// has already been inserted before via [`insert`](PalettedBlocks::insert)  
    pub fn insert_at<C>(&mut self, coords: C, palette: u32, palette_index: u32)
    where
        C: Into<Coords>,
    {
        // increment ref count for palette
        self.palette[palette as usize].1 += 1;

        let block_index = PalettedBlocks::to_index(self.bottom_y, self.width, coords.into());

        // at the actual coordinate and its palette reference number
        self.blocks[block_index as usize] =
            PalettedBlocks::construct_block_val(palette as u16, palette_index as u16);

        // enable the bit for quick access
        self.placed_blocks.set(block_index as usize, true);
    }

    /// Inserts a block coordinate and it's attached palette data into the list.  
    ///
    /// This searches through all palette to check if it's already pushed as a reference.  
    /// Therefore you should try and use the `u32` that returned as it is the `palette` index.  
    /// That can be used in [`insert_at`](PalettedBlocks::insert_at) for quicker insertions later on.  
    ///
    /// Takes in a full on reference to the specified palette list & the block inside it.  
    ///
    /// As the consumer of this crate, you will 99.99% of the time never touch this function nor [`insert_at`](PalettedBlocks::insert_at).  
    /// Since if you use [`get_blocks`](crate::Region::get_blocks) it does all this job for you.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::PalettedBlocks;
    /// let mut blocks = PalettedBlocks::new(-64..320, 16);
    /// let palette = PalettedBlocks::generate_palette(vec!["stone".into()])?;
    ///
    /// let _ = blocks.insert((8, -38, 13), &palette.as_slice(), 0);
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn insert<C>(&mut self, coords: C, palette: &'a [NbtCompound], palette_index: usize) -> u32
    where
        C: Into<Coords>,
    {
        let mut palette_block = None;
        for (idx, palette_entry) in self.palette.iter_mut().enumerate() {
            if palette == palette_entry.0 {
                palette_block = Some((idx as u32, palette_entry));
            }
        }

        let index = match palette_block {
            Some(pb) => {
                // if it already exist we can just increment it ref count
                pb.1.1 += 1;
                // pb.0 is the idx we found the block at
                pb.0
            }
            None => {
                // otherwise insert it with a ref count of 1
                self.palette.push((palette, 1));
                (self.palette.len() - 1) as u32
            }
        };

        // insert the indexes into the blocks to record it
        let block_index = PalettedBlocks::to_index(self.bottom_y, self.width, coords.into());
        self.blocks[block_index as usize] =
            PalettedBlocks::construct_block_val(index as u16, palette_index as u16);
        self.placed_blocks.set(block_index as usize, true);

        index
    }

    /// Inserts only a palette into the [`PalettedBlocks`].  
    ///
    /// Returning whatever index, returns an already existing index if it already is in this [`PalettedBlocks`].  
    pub fn insert_palette_only(&mut self, palette: &'a [NbtCompound]) -> u32 {
        for (idx, palette_entry) in self.palette.iter().enumerate() {
            if palette_entry.0 == palette {
                return idx as u32;
            }
        }

        // this is some spooky stuff because it has a ref count of 0
        // but still lives in the paletted blocks
        // but its chill, because the first insert will bump this with += 1;
        // and then any other remove that checks if it should shift is chill again.
        // and remove cant get to a ref count of 0 because no blocks would be pointing to it
        // so remove would early return before it could even do it.
        self.palette.push((palette, 0));
        // the given palette didnt exist so we return len -1
        (self.palette.len() - 1) as u32
    }

    /// Returns the [`Block`] and the given coordinates.  
    ///
    /// If it exists in this specific [`PalettedBlocks`]  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::PalettedBlocks;
    /// let mut blocks = PalettedBlocks::new(-64..320, 16);
    /// // inserting..
    /// # let palette = PalettedBlocks::generate_palette(vec!["stone".into()])?;
    /// # let palette = palette.as_slice();
    /// # blocks.insert((8, 183, 1), &palette, 0);
    /// let block = blocks.get((8, 183, 1))?;
    /// # assert!(block.is_some());
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn get<C>(&self, coords: C) -> Result<Option<Block>>
    where
        C: Into<Coords>,
    {
        Ok(self
            .get_raw(coords)
            .map(|c| Block::from_compound(c))
            .transpose()?)
    }

    /// Returns the raw [`Block`] [Nbt](NbtCompound) if a block in the list exists at these coordinates.  
    pub fn get_raw<C>(&self, coords: C) -> Option<&NbtCompound>
    where
        C: Into<Coords>,
    {
        let block_index = PalettedBlocks::to_index(self.bottom_y, self.width, coords.into());
        let (palette, palette_index) = match self.blocks.get(block_index as usize) {
            Some(i) if *i == u32::MAX => return None,
            Some(i) => PalettedBlocks::deconstruct_block_val(*i),
            None => return None,
        };
        //println!("{palette:?} {palette_index:?} ({block_index})");

        Some(&self.palette[palette as usize].0[palette_index as usize])
    }

    /// Removes the [`Block`] from this batch of [`PalettedBlocks`]  
    /// and returns it.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::PalettedBlocks;
    /// let mut blocks = PalettedBlocks::new(-64..320, 16);
    /// // insert blocks..
    /// # let palette = PalettedBlocks::generate_palette(vec!["stone".into()])?;
    /// # let palette = palette.as_slice();
    /// # blocks.insert((14, 62, 2), &palette, 0);
    /// let block = blocks.remove((14, 62, 2))?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn remove<C>(&mut self, coords: C) -> Result<Block>
    where
        C: Into<Coords>,
    {
        self.remove_block(coords)
    }

    /// Internal remove function that removes the block coordinate > index entry.  
    /// And it updates the palette ref count and handles the palette indexes & updates them.  
    /// Mostly is it's own thing incase scenarios and just so i can put this yapping docs here.  
    pub(crate) fn remove_block<C>(&mut self, coords: C) -> Result<Block>
    where
        C: Into<Coords>,
    {
        let coords: Coords = coords.into();
        let block_index = PalettedBlocks::to_index(self.bottom_y, self.width, coords);
        if block_index as usize >= self.blocks.len() {
            return Err(Error::OutOfBounds {
                len: self.blocks.len(),
                index: block_index as usize,
            });
        }

        // replace the old value with u32::MAX to mark it as vacant
        let combined_indexes = std::mem::replace(&mut self.blocks[block_index as usize], u32::MAX);
        if combined_indexes == u32::MAX {
            return Err(Error::UnsetPaletteBlock(block_index));
        }

        self.placed_blocks.set(block_index as usize, false);

        let (palette, palette_index) = PalettedBlocks::deconstruct_block_val(combined_indexes);

        // :tf:
        // the thing with this unsafe is that we only modify self.palette
        // in each branch of the following match
        // one removes it and the other one decrements an internal value
        // so they wont ever collide :D
        let palette_ptr = &mut self.palette as *mut Vec<(&[NbtCompound], usize)>;
        let palette_entry = unsafe {
            match (&mut (*palette_ptr)).get_mut(palette as usize) {
                Some(pe) => pe,
                None => return Err(Error::InvalidPaletteIndex(palette as i64)),
            }
        };

        // match against the ref count
        match palette_entry.1 {
            1 => {
                // if it its already on 1 and we are about to remove,
                // we do the cleaning process

                // firstly, remove it from the palette once it has no refs
                self.palette.remove(palette as usize);

                // then shift all existing indexes that was after palette_index
                self.shift_indexes(palette);
            }
            // if its any other than 1, we just decrement its ref
            _ => {
                palette_entry.1 -= 1;
            }
        };

        // both above arms ended into the same result so moved here
        let block = palette_entry
            .0
            .get(palette_index as usize)
            .ok_or(Error::OutOfBounds {
                len: palette_entry.0.len(),
                index: palette_index as usize,
            })?;

        Ok(Block::from_compound(block)?)
    }

    /// Takes in a list of blocks and creating a NBT palette list from it.  
    ///
    /// ## Example
    /// ```
    /// # use silverfish::{PalettedBlocks};
    /// let palette = PalettedBlocks::generate_palette(vec!["stone".into(), "dirt".into()])?;
    /// let palette = palette.as_slice();
    ///
    /// let mut blocks = PalettedBlocks::new(-64..320, 16);
    /// blocks.insert((14, 65, 6), &palette, 1); // inserts a dirt block at 14, 65, 6
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn generate_palette(blocks: Vec<Block>) -> Result<Vec<NbtCompound>> {
        let mut nbt_blocks: Vec<NbtCompound> = vec![];
        for block in blocks {
            nbt_blocks.push(block.to_compound()?);
        }

        Ok(nbt_blocks)
    }
}

/// A struct that holds a ref to [`PalettedBlocks`]  
/// and a iter over [`placed_blocks`](PalettedBlocks::placed_blocks) and it's only enabled bits.  
pub struct PalettedBlocksIntoIter<'a> {
    blocks: &'a PalettedBlocks<'a>,
    placed_iter: Ones<'a>,
}

impl<'a> Iterator for PalettedBlocksIntoIter<'a> {
    type Item = (Coords, Block);

    fn next(&mut self) -> Option<Self::Item> {
        // we can just hijack the iter for enabled bits on the placed_blocks bitset.
        // way way WAYY faster than iterating over `blocks` and checking if its u32::MAX etc.
        let index = match self.placed_iter.next() {
            Some(i) => i,
            None => return None,
        };

        let coords =
            PalettedBlocks::to_coords(self.blocks.bottom_y, self.blocks.width, index as u32);
        let val = self.blocks.blocks[index];

        let (palette, palette_index) = PalettedBlocks::deconstruct_block_val(val);
        let block_nbt = match self
            .blocks
            .palette
            .get(palette as usize)
            .map(|p| p.0.get(palette_index as usize))
        {
            Some(Some(block)) => block,
            _ => return None,
        };

        let block = match Block::from_compound(block_nbt) {
            Ok(b) => b,
            Err(_) => return None,
        };

        return Some((coords, block));
    }
}

/// Iterator only works on a reference [`PalettedBlocks`] due to all the palette references.  
impl<'a> IntoIterator for &'a PalettedBlocks<'a> {
    type Item = (Coords, Block);
    type IntoIter = PalettedBlocksIntoIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        PalettedBlocksIntoIter {
            blocks: &self,
            placed_iter: self.placed_blocks.ones(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Result;

    #[test]
    fn new() {
        let blocks = PalettedBlocks::new(-64..320, 16);
        assert_eq!(blocks.bottom_y, -64);
        assert_eq!(blocks.width, 16);
        assert_eq!(blocks.palette.len(), 0);
        assert_eq!(blocks.placed_blocks.len(), 16 * 384 * 16);
    }

    #[test]
    fn len() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        assert_eq!(blocks.len(), 0);

        let palette = PalettedBlocks::generate_palette(vec!["stone".into()])?;
        let palette = palette.as_slice();
        blocks.insert((2, 3, 4), &palette, 0);
        assert_eq!(blocks.len(), 1);

        blocks.remove((2, 3, 4))?;
        assert_eq!(blocks.len(), 0);

        Ok(())
    }

    #[test]
    fn contains() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);

        let palette = PalettedBlocks::generate_palette(vec!["minecraft:iron_ore".into()])?;
        let palette = palette.as_slice();
        blocks.insert((14, 283, 2), &palette, 0);

        assert!(blocks.contains(&Block::new("minecraft:iron_ore")));
        assert!(!blocks.contains(&Block::new("minecraft:diamond_ore")));

        blocks.remove((14, 283, 2))?;
        assert!(!blocks.contains(&Block::new("minecraft:iron_ore")));

        Ok(())
    }

    #[test]
    fn get_all() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);

        let palette = PalettedBlocks::generate_palette(vec![
            "minecraft:iron_ore".into(),
            "minecraft:coal_ore".into(),
        ])?;
        let palette = palette.as_slice();
        assert_eq!(blocks.get_all().len(), 0);

        blocks.insert((8, 1, 5), &palette, 1);
        assert_eq!(blocks.get_all().len(), 1);
        assert_eq!(blocks.get_all()[0].coordinates, Coords::new(8, 1, 5));

        blocks.insert((13, -52, 1), &palette, 0);
        assert_eq!(blocks.get_all().len(), 2);

        Ok(())
    }

    #[test]
    fn insert_at() -> Result<()> {
        let mut blocks = PalettedBlocks::new(0..16, 8);

        let palette = PalettedBlocks::generate_palette(vec!["custom:spawner".into()])?;
        let palette = palette.as_slice();
        let palette_index = blocks.insert((4, 1, 2), &palette, 0);
        assert_eq!(blocks.len(), 1);

        blocks.insert_at((5, 1, 2), palette_index, 0);
        assert_eq!(blocks.len(), 2);

        Ok(())
    }

    #[test]
    fn insert() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);

        let palette = PalettedBlocks::generate_palette(vec![
            "minecraft:grass_block".into(),
            "minecraft:fern".into(),
        ])?;
        let palette = palette.as_slice();
        blocks.insert((4, 1, 2), &palette, 0);
        assert_eq!(blocks.len(), 1);

        blocks.insert((4, 1, 2), &palette, 1);
        assert_eq!(blocks.len(), 1);

        blocks.insert((13, -42, 9), &palette, 0);
        assert_eq!(blocks.len(), 2);

        Ok(())
    }

    #[test]
    fn insert_fill() -> Result<()> {
        let mut blocks = PalettedBlocks::new(0..64, 16);
        let palette = PalettedBlocks::generate_palette(vec!["minecraft:stone".into()])?;
        let palette = palette.as_slice();

        let pal_index = blocks.insert_palette_only(&palette);
        for x in 0..16 {
            for y in 0..64 {
                for z in 0..16 {
                    blocks.insert_at((x, y, z), pal_index, 0);
                }
            }
        }

        assert_eq!(blocks.len(), 16 * 64 * 16);

        Ok(())
    }

    #[test]
    fn insert_palette() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        let palette = PalettedBlocks::generate_palette(vec!["minecraft:stone".into()])?;
        let palette = palette.as_slice();

        assert_eq!(blocks.palette.len(), 0);
        blocks.insert_palette_only(&palette);
        assert_eq!(blocks.palette.len(), 1);

        blocks.insert_palette_only(&palette);
        assert_eq!(blocks.palette.len(), 1);

        let palette = PalettedBlocks::generate_palette(vec!["minecraft:dirt".into()])?;
        let palette = palette.as_slice();

        blocks.insert_palette_only(&palette);
        assert_eq!(blocks.palette.len(), 2);

        Ok(())
    }

    #[test]
    fn get() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        let palette = PalettedBlocks::generate_palette(vec!["minecraft:grass_block".into()])?;
        let palette = palette.as_slice();
        blocks.insert((4, 1, 2), &palette, 0);

        let block = blocks.get((4, 1, 2))?;
        assert_eq!(block, Some(Block::new("minecraft:grass_block")));

        let block = blocks.get((14, -52, 12))?;
        assert_eq!(block, None);

        Ok(())
    }

    #[test]
    fn remove() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        let palette = PalettedBlocks::generate_palette(vec!["minecraft:grass_block".into()])?;
        let palette = palette.as_slice();

        blocks.insert((5, 1, 5), &palette, 0);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks.palette.len(), 1);

        let _ = blocks.remove((5, 1, 5))?;
        assert_eq!(blocks.len(), 0);
        assert_eq!(blocks.palette.len(), 0);

        blocks.insert((5, 1, 5), &palette, 0);
        assert_eq!(blocks.len(), 1);

        let val = blocks.remove((0, 0, 0));
        assert!(val.is_err());

        Ok(())
    }

    #[test]
    fn generate_palette() -> Result<()> {
        let palette = PalettedBlocks::generate_palette(vec![
            "minecraft:grass_block".into(),
            "minecraft:stone".into(),
        ])?;
        assert_eq!(palette.len(), 2);
        assert_eq!(
            Block::from_compound(&palette[0])?,
            Block::new("minecraft:grass_block")
        );
        Ok(())
    }

    #[test]
    fn iter() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        let palette = PalettedBlocks::generate_palette(vec!["minecraft:grass_block".into()])?;
        let palette = palette.as_slice();

        for x in 0..8 {
            blocks.insert((x, 5, 8), &palette, 0);
        }

        for (idx, (coords, block)) in blocks.into_iter().enumerate() {
            assert!(idx < 8);
            assert!(blocks.contains(&block));
            assert_eq!(coords.z, 8);
        }

        Ok(())
    }

    #[test]
    fn index() {
        let index = PalettedBlocks::to_index(-64, 16, Coords::new(5, 5, 5));
        assert_eq!(index, 17749);

        let index = PalettedBlocks::to_index(-64, 16, Coords::new(0, -58, 15));
        assert_eq!(index, 1776);
    }

    #[test]
    fn block_val() {
        let constructed = PalettedBlocks::construct_block_val(12, 81);
        assert_eq!(constructed, 5_308_428);
        let (de_1, de_2) = PalettedBlocks::deconstruct_block_val(constructed);
        assert_eq!(de_1, 12);
        assert_eq!(de_2, 81);

        assert_eq!(
            PalettedBlocks::construct_block_val(24575, 4095),
            268_394_495
        );
        assert_eq!(PalettedBlocks::construct_block_val(0, 0), 0);
        assert_eq!(
            PalettedBlocks::construct_block_val(0, 1),
            u16::MAX as u32 + 1
        );
    }

    #[test]
    #[should_panic]
    fn panic_block_val() {
        PalettedBlocks::construct_block_val(58282, 8418);
    }

    #[test]
    fn palette_shift() -> Result<()> {
        let mut blocks = PalettedBlocks::new(-64..320, 16);
        let palette_1 = PalettedBlocks::generate_palette(vec!["minecraft:grass_block".into()])?;
        let palette_1 = palette_1.as_slice();

        let palette_2 = PalettedBlocks::generate_palette(vec!["minecraft:stone".into()])?;
        let palette_2 = palette_2.as_slice();

        blocks.insert((5, 283, 8), &palette_1, 0);
        blocks.insert((5, 1, 8), &palette_1, 0);

        let block_1_c = Coords::new(8, 283, 5);
        blocks.insert(block_1_c, &palette_2, 0);
        blocks.insert((8, 1, 5), &palette_2, 0);
        assert_eq!(
            PalettedBlocks::deconstruct_block_val(
                blocks.blocks[PalettedBlocks::to_index(-64, 16, block_1_c) as usize]
            )
            .0,
            1
        );
        blocks.remove((5, 283, 8))?;
        blocks.remove((5, 1, 8))?;
        assert_eq!(
            PalettedBlocks::deconstruct_block_val(
                blocks.blocks[PalettedBlocks::to_index(-64, 16, block_1_c) as usize]
            )
            .0,
            0
        );

        Ok(())
    }

    // TODO, PalettedBlocks got so many tests that it makes me wanna do more tests for other modules as well lmaoo
}
