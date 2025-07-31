//! `config` contains the [`Config`] used in [`crate::Region`].  

use std::ops::RangeInclusive;

/// A config used for dictating how [`crate::Region`] should write blocks.  
#[derive(Debug, Clone)]
pub struct Config {
    /// Creates new empty air-filled chunks when chunks are missing.  
    ///
    /// Look at [`silverfish::get_empty_chunk`](crate::get_empty_chunk) for the initial data.  
    pub create_chunk_if_missing: bool,
    /// If it should flag the chunks for Minecraft to re-calculate lighting when first loaded ingame  
    pub update_lighting: bool,

    /// How tall/deep the world is, defaults to `-64..=320`
    ///
    /// Used for properly setting internal buffers.  
    pub(crate) world_height: RangeInclusive<isize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            create_chunk_if_missing: false,
            update_lighting: true,
            world_height: Config::DEFAULT_WORLD_HEIGHT,
        }
    }
}

impl Config {
    /// The default world height in Minecraft (as of `1.17+ (21w06a)`)
    pub const DEFAULT_WORLD_HEIGHT: RangeInclusive<isize> = -64..=320;

    /// Returns the world_height.  
    ///
    /// **Note:** to mutate world_height you either need to pass it a region first or use [`Config::new`].
    pub fn get_world_height(&self) -> &RangeInclusive<isize> {
        &self.world_height
    }

    /// Creates a new [`Config`]
    pub fn new(
        create_chunk_if_missing: bool,
        update_lighting: bool,
        world_height: RangeInclusive<isize>,
    ) -> Self {
        Self {
            create_chunk_if_missing,
            update_lighting,
            world_height,
        }
    }
}
