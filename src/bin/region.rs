use std::{fs::File, time::Instant};

use rust_edit::{Block, Region};

fn main() {
    let mut region = Region::full_empty((0, 0));
    region.set_block(2, 80, 2, Block::new("beacon"));
    let write_instant = Instant::now();
    region
        .write_blocks()
        .inspect_err(|e| println!("{e}"))
        .unwrap();
    println!("took {:?} to set block", write_instant.elapsed());

    // // full region write
    // let loop_instant = Instant::now();
    // region.allocate_block_buffer(4096);
    // for x in 0..512 {
    //     for y in -64i32..320i32 {
    //         for z in 0..512 {
    //             region.set_block(x, y, z, Block::new("terracotta"));
    //         }
    //     }
    // }
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
        .inspect_err(|e| println!("{e}"))
        .unwrap();
}
