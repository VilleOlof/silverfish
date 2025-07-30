use silverfish::{Region, Result};
use std::fs::File;

const BLOCKS_PER_REGION: u32 = 512;

pub fn main() -> Result<()> {
    let mut region = Region::full_empty((0, 0));

    for x in 0..BLOCKS_PER_REGION {
        for z in 0..BLOCKS_PER_REGION {
            region.set_block(x, 0, z, "minecraft:bedrock");
            region.set_block(x, 1, z, "minecraft:dirt");
            region.set_block(x, 2, z, "minecraft:dirt");
            region.set_block(x, 3, z, "minecraft:grass_block");
        }
    }

    region.write_blocks()?;
    region.write(&mut File::create("r.0.0.mca")?)?;

    Ok(())
}
