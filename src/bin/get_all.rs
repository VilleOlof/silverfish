use std::time::Instant;

use silverfish::{Region, Result};

fn main() -> Result<()> {
    let region = Region::default();

    let instant = Instant::now();
    let mut blocks = Vec::with_capacity(512 * 384 * 512);
    for x in 0..512 {
        for y in -64..320 {
            for z in 0..512 {
                blocks.push((x, y, z));
            }
        }
    }
    let blocks = region.get_blocks(&blocks)?;
    println!(
        "[{:?}] {} / {}",
        instant.elapsed(),
        blocks.len(),
        512 * 384 * 512
    );

    Ok(())
}
