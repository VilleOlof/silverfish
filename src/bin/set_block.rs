#[allow(unused_imports)]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
#[allow(unused_imports)]
use silverfish::{Block, Name, Region, Result};
use std::time::Instant;

fn main() -> Result<()> {
    let main_instant = Instant::now();
    let mut region = Region::full_empty((0, 0));
    println!("Took {:?} to create empty region", main_instant.elapsed());

    let set_instant = Instant::now();
    region.set_block((2, 80, 2), Block::new("beacon")).unwrap();
    println!("Took {:?} to set_block", set_instant.elapsed());

    let write_instant = Instant::now();
    region.write_blocks().inspect_err(|e| println!("{e}"))?;
    println!("Took {:?} to write_blocks", write_instant.elapsed());

    let get_instant = Instant::now();
    region
        .get_block((2, 80, 2))
        .inspect_err(|e| println!("{e}"))?;
    println!("Took {:?} to get_block", get_instant.elapsed());

    println!("Took {:?} in total", main_instant.elapsed());

    Ok(())
}
