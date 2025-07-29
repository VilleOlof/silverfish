use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rust_edit::{Block, Region};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_region");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(600)); // 10 minutes or so, this is a juicy one

    group.bench_function("full_region", |b| {
        b.iter(|| {
            let mut region = Region::full_empty((0, 0));
            region.allocate_block_buffer(0..32, 0..32, -4..20, 4096);

            for x in 0..512 {
                for y in -64i32..320i32 {
                    for z in 0..512 {
                        region.set_block(
                            black_box(x),
                            black_box(y),
                            black_box(z),
                            black_box(Block::new("white_concrete")),
                        );
                    }
                }
            }

            region.write_blocks().unwrap();
        })
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
