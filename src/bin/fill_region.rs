use std::time::Instant;

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use silverfish::{Block, Error, Name, Region, Result};

fn main() -> Result<()> {
    let main_instant = Instant::now();
    let mut region = Region::default();
    println!("Took {:?} to create empty region", main_instant.elapsed());

    let loop_instant = Instant::now();
    let block: Block = Name::Namespaced("minecraft:white_concrete".into()).into();
    region
        .allocate_block_buffer(0..32, 0..32, -4..20, 4096)
        .unwrap();
    (0..(512 * 384 * 512))
        .into_par_iter()
        .try_for_each(|index| {
            let x = index / (384 * 512);
            let rem = index % (384 * 512);
            let y = rem / 512;
            let z = rem % 512;

            let y = y as i32 - 64;
            let chunk_x = x as u8 / 16;
            let chunk_z = z as u8 / 16;

            let mut chunk = region.get_chunk_mut(chunk_x, chunk_z)?;
            chunk.set_block(x & 15, y, z & 15, block.clone())?;

            Ok::<(), Error>(())
        })?;

    println!("Took {:?} to set_block", loop_instant.elapsed());

    let write_instant = Instant::now();
    region.write_blocks().inspect_err(|e| println!("{e}"))?;
    println!("Took {:?} to write_blocks", write_instant.elapsed());

    println!("Took {:?} in total", main_instant.elapsed());

    Ok(())
}
