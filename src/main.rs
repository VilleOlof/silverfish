use fastnbt::{LongArray, Value, from_value};
use mca::{RegionReader, RegionWriter};
use rust_edit::nbt::{Block, Section};
use std::{collections::HashMap, io::Read, ops::Deref, path::PathBuf};

#[derive(Debug)]
struct ModifyOperation {
    world_path: PathBuf,
    region: (usize, usize),
    chunk: (usize, usize),
    coordinates: (usize, usize, usize),
    block: Block,
}

impl ModifyOperation {
    fn get_region_path(&self) -> PathBuf {
        let region_file = format!("r.{}.{}.mca", self.region.0, self.region.1);
        self.world_path.join("region").join(region_file)
    }

    fn run(&self) {
        println!("Modifying region: {:?}", self.get_region_path());

        let mut file = std::fs::File::open(self.get_region_path()).unwrap();

        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();

        let region = RegionReader::new(&data).unwrap();
        let mut writer = RegionWriter::new();

        // first we move all the chunks to keep everything the same
        println!("copying chunk data (this takes the longest)");
        for (i, chunk) in region.iter().enumerate() {
            let chunk = chunk.unwrap();
            if let Some(chunk) = chunk {
                let coords = (i % 32, i / 32);
                // skip writing our modification chunk on the copy
                if coords == self.chunk {
                    continue;
                }
                writer
                    .push_chunk_with_compression(
                        &chunk.decompress().unwrap(),
                        (coords.0 as u8, coords.1 as u8),
                        mca::CompressionType::Zlib,
                    )
                    .unwrap();
            }
        }
        println!("copied chunk data");

        let chunk = region
            .get_chunk(self.chunk.0, self.chunk.1)
            .unwrap()
            .unwrap();

        let decompressed = chunk.decompress().unwrap();
        let chunk = self.handle_chunk(decompressed);
        println!("finished chunk handler");

        // then we can write the chunk we modified seperately
        writer
            .push_chunk_with_compression(
                &chunk,
                (self.chunk.0 as u8, self.chunk.1 as u8),
                mca::CompressionType::Zlib,
            )
            .unwrap();

        //TODO: update file in place
        //let mut file = std::fs::File::create(self.get_region_path()).unwrap();
        //writer.write(&mut file).unwrap();

        let mut file = std::fs::File::create("test.mca").unwrap();
        writer.write(&mut file).unwrap();

        println!("success?");
    }

    fn handle_chunk(&self, data: Vec<u8>) -> Vec<u8> {
        println!("starting chunk handler");
        let mut nbt: Value = fastnbt::from_bytes(&data).unwrap();
        let root = match &mut nbt {
            Value::Compound(root) => root,
            _ => panic!("invalid chunk"),
        };

        let section_idx = (self.coordinates.1 as f64 / 16f64).floor() as i8;

        let sections = root.get_mut("sections").unwrap();
        let mut section: Option<Value> = None;
        match &sections {
            Value::List(sections) => {
                for v in sections {
                    match v {
                        Value::Compound(c) if c.get("Y").unwrap() == section_idx => {
                            section = Some(v.clone());
                        }
                        Value::Compound(_) => (),
                        _ => panic!("invalid sections"),
                    }
                }
            }
            _ => panic!("Invalid sections"),
        };

        let section: Section = from_value(&section.unwrap()).unwrap();
        let modified_section = self.modify_section(section);

        let new_sections = match sections {
            Value::List(sections) => {
                let mut new_sections = vec![modified_section];
                for sec in sections {
                    // skip if the one we modified
                    match sec {
                        Value::Compound(c) if c.get("Y").unwrap() == section_idx => continue,
                        _ => (),
                    }

                    new_sections.push(sec.deref().clone());
                }
                new_sections
            }
            _ => panic!("invalid sections"),
        };

        root.insert("sections".into(), Value::List(new_sections));

        fastnbt::to_bytes(&nbt).unwrap()
    }

    fn modify_section(&self, section: Section) -> Value {
        // clear light data so it gets re-computed by the game?
        let mut section = section;

        let mut state = section.block_states;

        let bit_count: u32 = state
            .palette
            .len()
            .next_power_of_two()
            .trailing_zeros()
            .max(4);

        let mut old_indexes: Vec<i64> = Vec::new();

        let mut offset: u32 = 0;
        for data_block in state.data.iter() {
            while (offset * bit_count) + bit_count <= 64 {
                let block = (data_block >> (offset * bit_count)) & ((1 << bit_count) - 1);

                old_indexes.push(block);

                offset += 1
            }
            offset = 0;
        }
        old_indexes.truncate(4096);


        if state.palette.contains(&self.block) {
            println!("Block already exists in palette, skipping palette modification.");
        } else {
            state.palette.push(self.block.clone());
        }

        let (x, y, z) = (
            self.coordinates.0 as i64 % 16,
            self.coordinates.1 as i64 - (section.y * 16) as i64,
            self.coordinates.2 as i64 % 16,
        );

        let index = x + z * 16 + y * 16 * 16;

        old_indexes[index as usize] =
            state.palette.iter().position(|b| b == &self.block).unwrap() as i64;

        // remove unused blocks from the palette
        // ugly but works
        let mut unused_indexes = Vec::new();
        for (idx, _p) in state.palette.iter().enumerate() {
            if old_indexes.contains(&(idx as i64)) {
                continue;
            }

            unused_indexes.push(idx as i64);
        }

        for index in unused_indexes.iter().rev() {
            state.palette.remove(*index as usize);
            for block in old_indexes.iter_mut() {
                if *block > *index {
                    *block -= 1;
                }
            }
        }

        let mut new_blockdata = vec![];
        let bit_count: u32 = state
            .palette
            .len()
            .next_power_of_two()
            .trailing_zeros()
            .max(4);

        let mut offset = 0;
        let mut currrent_long: i64 = 0;
        for block in old_indexes.iter() {
            currrent_long |= block << (offset * bit_count);
            offset += 1;

            if (offset * bit_count) + bit_count > 64 {
                new_blockdata.push(currrent_long);
                currrent_long = 0;
                offset = 0;
            }
        }

        if offset > 0 {
            new_blockdata.push(currrent_long);
        }

        state.data = LongArray::new(new_blockdata);
        section.block_states = state;
        section.to_value()
    }
}

fn main() {
    let op = ModifyOperation {
        world_path: PathBuf::from(""),
        region: (0, 0),
        chunk: (0, 0),
        coordinates: (12, 111, 5),
        block: Block {
            name: String::from("minecraft:bedrock"),
            properties: HashMap::new(),
        },
    };

    println!("{op:?}");

    op.run();
}
