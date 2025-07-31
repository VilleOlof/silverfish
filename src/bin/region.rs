#[allow(unused_imports)]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
#[allow(unused_imports)]
use silverfish::{Block, Name, Region, Result};
use std::{fs::File, time::Instant};

fn main() -> Result<()> {
    let mut region = Region::full_empty((0, 0));
    region.set_block(2, 80, 2, Block::new("beacon")).unwrap();
    let write_instant = Instant::now();
    region
        .write_blocks()
        .inspect_err(|e| println!("{e}"))
        .unwrap();
    println!("took {:?} to set block", write_instant.elapsed());

    // fix this shit and split it into some different bins

    // // get all blocks
    // let instant = Instant::now();
    // let mut blocks = Vec::with_capacity(512 * 384 * 512);
    // for x in 0..512 {
    //     for y in -64..320 {
    //         for z in 0..512 {
    //             blocks.push((x, y, z));
    //         }
    //     }
    // }
    // let blocks = region.get_blocks(&blocks)?;
    // println!(
    //     "[{:?}] {} / {}",
    //     instant.elapsed(),
    //     blocks.len(),
    //     512 * 384 * 512
    // );

    // let loop_instant = Instant::now();
    // let block: Block = Name::Namespaced("minecraft:white_concrete".into()).into();
    // region
    //     .allocate_block_buffer(0..32, 0..32, -4..20, 4096)
    //     .unwrap();
    // (0..(512 * 384 * 512)).into_par_iter().for_each(|index| {
    //     let x = index / (384 * 512);
    //     let rem = index % (384 * 512);
    //     let y = rem / 512;
    //     let z = rem % 512;

    //     let y = y as i32 - 64;
    //     let chunk_x = x as u8 / 16;
    //     let chunk_z = z as u8 / 16;

    //     let mut chunk = region.get_chunk_mut(chunk_x, chunk_z).unwrap();

    //     chunk.set_block(x & 15, y, z & 15, block.clone()).unwrap();
    // });

    // println!("took {:?} to loop, writing...", loop_instant.elapsed());
    // let write_instant = Instant::now();
    // region
    //     .write_blocks()
    //     .inspect_err(|e| println!("{e}"))
    //     .unwrap();
    // println!(
    //     "took {:?} to set block, total: {:?}",
    //     write_instant.elapsed(),
    //     loop_instant.elapsed()
    // );

    let get_instant = Instant::now();
    println!(
        "{:?}",
        region
            .get_block(2, 80, 2)
            .inspect_err(|e| println!("{e}"))
            .unwrap()
    );
    println!("took {:?} to get block", get_instant.elapsed());

    region
        .write(&mut File::create("./r.0.0.mca").unwrap())
        .inspect_err(|e| println!("{e}"))?;

    Ok(())
}
