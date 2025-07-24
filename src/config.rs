#[allow(unused_imports)] // used for doc
use crate::world::World;
use mca::CompressionType;

/// Config for when flushing operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushConfig {
    /// If it should flag the chunks for Minecraft to re-calculate lighting when first loaded ingame  
    ///
    /// See [`World::MIN_LIGHT_DATA_VERSION`] for minimum Minecraft version required for this flag.  
    pub update_lighting: bool,
    /// The [`CompressionType`] to use when saving modified chunks.  
    ///
    /// Unmodified chunks will retain it's [`CompressionType`]
    pub chunk_compression: CompressionType,
}

impl Default for FlushConfig {
    fn default() -> Self {
        Self {
            update_lighting: true,
            chunk_compression: CompressionType::Zlib,
        }
    }
}
