use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use silverfish::{Block, Name, Region};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_region");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));

    group.bench_function("full_region", |b| {
        b.iter(|| {
            let mut region = Region::default();
            region
                .allocate_block_buffer(0..32, 0..32, -4..20, 4096)
                .unwrap();

            let block: Block = Name::Namespaced("minecraft:white_concrete".into()).into();
            (0..(512 * 384 * 512)).into_par_iter().for_each(|index| {
                let x = index / (384 * 512);
                let rem = index % (384 * 512);
                let y = rem / 512;
                let z = rem % 512;

                let y = y as i32 - 64;
                let chunk_x = x as u8 / 16;
                let chunk_z = z as u8 / 16;

                let mut chunk = region.get_chunk_mut(chunk_x, chunk_z).unwrap();

                chunk
                    .set_block(
                        black_box(x as u32),
                        black_box(y),
                        black_box(z as u32),
                        black_box(block.clone()),
                    )
                    .unwrap();
            });

            region.write_blocks().unwrap();
        })
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
