use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rust_edit::{nbt::Block, region::Region};

const PATH: &'static str = "./region/r.0.0.mca";
pub fn criterion_benchmark(c: &mut Criterion) {
    let mut region = Region::from_path(PATH);
    c.bench_function("region 1x (new)", |b| {
        b.iter(|| {
            region.set_block(0, 0, 0, black_box(Block::new("beacon")));
            region.write_blocks();
        })
    });
    c.bench_function("region 16x (new)", |b| {
        b.iter(|| {
            let colors = [
                "white",
                "brown",
                "gray",
                "light_gray",
                "black",
                "red",
                "orange",
                "magenta",
                "purple",
                "pink",
                "cyan",
                "blue",
                "light_blue",
                "green",
                "lime",
            ];

            for (i, col) in colors.iter().enumerate() {
                let block = format!("{col}_wool");
                region.set_block(i as u32, 0, 0, black_box(Block::new(block)));
            }

            region.write_blocks();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
