use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use silverfish::{Error, Region, Result};
use std::fs::File;

fn main() -> Result<()> {
    let mut region = Region::default();

    (0..32).collect::<Vec<u8>>().par_iter().try_for_each(|z| {
        let mut chunk = region.get_chunk_mut(0, *z)?;
        chunk.set_block(0, 0, 0, "furnace").unwrap();

        Ok::<(), Error>(())
    })?;

    region.write_blocks()?;
    region.write(&mut File::create("r.0.0.mca")?)?;

    Ok(())
}
