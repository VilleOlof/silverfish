//! `config` contains the [`Config`] used in [`crate::Region`].  

/// A config used for dictating how [`crate::Region`] should write blocks.  
#[derive(Debug, Clone)]
pub struct Config {
    /// Creates new empty air-filled chunks when chunks are missing.  
    ///
    /// Look at [`crate::get_empty_chunk`] for the initial data.  
    pub create_chunk_if_missing: bool,
    /// If it should flag the chunks for Minecraft to re-calculate lighting when first loaded ingame  
    pub update_lighting: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            create_chunk_if_missing: false,
            update_lighting: true,
        }
    }
}
