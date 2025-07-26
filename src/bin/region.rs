use std::{fs::File, time::Instant};

use rust_edit::{nbt::Block, region::Region};

fn main() {
    let mut region = Region::full_empty();
    let write_instant = Instant::now();
    region.set_block(2, 80, 2, Block::new("beacon"));
    region.write_blocks();
    println!("took {:?} to set block", write_instant.elapsed());

    // // full region write
    // let write_instant = Instant::now();
    // for x in 0..512 {
    //     for y in -64i32..320i32 {
    //         for z in 0..512 {
    //             region.set_block(x, y, z, Block::new("terracotta"));
    //         }
    //     }
    // }
    // region.write_blocks();
    // println!("took {:?} to set block", write_instant.elapsed());

    let get_instant = Instant::now();
    println!("{:?}", region.get_block(2, 80, 2));
    println!("took {:?} to get block", get_instant.elapsed());

    region.write(&mut File::create("./r.0.0.mca").unwrap());
}
