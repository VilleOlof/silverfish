use rust_edit::{Block, Name, Region, Result};
use std::fs::File;

const BLOCKS_PER_REGION: u32 = 512;

pub fn main() -> Result<()> {
    let mut region = Region::full_empty((0, 0));

    let bedrock = Block::try_new(Name::new_namespace("minecraft:bedrock"))?;
    let dirt = Block::try_new(Name::new_namespace("minecraft:dirt"))?;
    let grass_block = Block::try_new(Name::new_namespace("minecraft:grass_block"))?;

    for x in 0..BLOCKS_PER_REGION {
        for z in 0..BLOCKS_PER_REGION {
            region.set_block(x, 0, z, bedrock.clone());
            region.set_block(x, 1, z, dirt.clone());
            region.set_block(x, 2, z, dirt.clone());
            region.set_block(x, 3, z, grass_block.clone());
        }
    }

    region.write_blocks()?;
    region.write(&mut File::create("r.0.0.mca")?)?;

    Ok(())
}
