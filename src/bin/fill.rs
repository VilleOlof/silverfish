use rust_edit::{
    config::FlushConfig, coordinate::Coordinate, nbt::Block, operation::Operation, world::World,
};
use std::time::Instant;

fn main() {

    // temporary sloppy argument thingy to test
    // example: cargo r --bin fill -- "." 10,10,10 30,30,30 bedrock
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("no world path given");
    let from: Vec<isize> = args
        .get(2)
        .expect("no from (x,y,z)")
        .split(",")
        .map(|f| f.parse::<isize>().unwrap())
        .collect();
    let to: Vec<isize> = args
        .get(3)
        .expect("no to (x,y,z)")
        .split(",")
        .map(|f| f.parse::<isize>().unwrap())
        .collect();
    let block = args
        .get(4)
        .map(|f| f.clone())
        .unwrap_or(String::from("stone"));

    let mut world = World::new(&path);
    world.push_op(Operation::Fill {
        from: Coordinate::new(from[0], from[1], from[2]),
        to: Coordinate::new(to[0], to[1], to[2]),
        block: Block::new(block),
    });
    let instant = Instant::now();
    world
        .flush(FlushConfig {
            update_lighting: true,
            ..Default::default()
        })
        .unwrap();
    println!("Modified world in {:?}", instant.elapsed());
}
