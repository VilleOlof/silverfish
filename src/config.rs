//! `config` contains the [`Config`] used in [`crate::Region`].  

#[derive(Debug, Clone)]
pub struct Config {
    /// Creates new empty air-filled chunks when chunks are missing.  
    ///
    /// Look at [`get_empty_chunk`] for the initial data.  
    pub create_chunk_if_missing: bool,
    /// If it should flag the chunks for Minecraft to re-calculate lighting when first loaded ingame  
    ///
    /// See [`Region::MIN_LIGHT_DATA_VERSION`] for minimum Minecraft version required for this flag.  
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
