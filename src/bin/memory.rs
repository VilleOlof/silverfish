use rust_edit::{
    config::FlushConfig, coordinate::Coordinate, nbt::Block, operation::Operation, world::World
};
use std::time::Instant;

fn main() {

    // temporary sloppy argument thingy to test
    // example: cargo r --bin memory

    let mut world = World::memory();

    world.push_op(Operation::Setblock { coordinate: Coordinate::new(50,50,50), block: Block::new("minecraft:bedrock") });
    world.push_op(Operation::Fill { from: Coordinate::new(20,20,20), to: Coordinate::new(-50, 10, -50), block: Block::new("minecraft:glass") });

    let instant = Instant::now();
    world
        .flush(FlushConfig {
            update_lighting: true,
            in_memory: true,
            ..Default::default()
        })
        .unwrap();
    println!("Modified world in {:?}", instant.elapsed());
}
