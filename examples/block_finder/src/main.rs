use silverfish::{Name, Region, Result};
use std::{env::args, fs::File};

// block_finder <region_file> <block_id>
fn main() -> Result<()> {
    let args = args().collect::<Vec<String>>();
    let region = args.get(1).expect("No region file given");
    let block_id = args.get(2).expect("No block_id file given");
    let block_name = Name::new_id(block_id.as_str()).into_namespaced();

    // we expect the region file to follow this format "r.x.x.mca"
    let region_coordinates = region.split('.').collect::<Vec<&str>>();
    let region_x = region_coordinates.get(1).unwrap().parse::<i32>().unwrap();
    let region_z = region_coordinates.get(2).unwrap().parse::<i32>().unwrap();

    let region = Region::from_region(&mut File::open(region)?, (region_x, region_z))?;

    // searches through all chunks, one at a time.
    let mut found = None;
    'outer: for x in 0..32 {
        for z in 0..32 {
            let mut blocks_to_search = Vec::with_capacity(98_304);

            for cx in 0..16 {
                for y in -64..320 {
                    for cz in 0..16 {
                        blocks_to_search.push(((x * 16) + cx, y, (z * 16) + cz));
                    }
                }
            }

            let blocks = region.get_blocks(&blocks_to_search)?;

            for block in blocks {
                if block.block.name == block_name {
                    found = Some(block);
                    break 'outer;
                }
            }
        }

        println!("{}/1024 chunks searched", (x + 1) * 32);
    }

    match found {
        Some(block) => {
            println!(
                "Found block matching {:?} at {:?}",
                block.block, block.coordinates
            );
        }
        None => println!("Found no block matching {block_name:?}"),
    }

    Ok(())
}
