use fastnbt::{ByteArray, LongArray, Value, from_value, to_value};
use mca::{RegionReader, RegionWriter};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Read, ops::Deref, path::PathBuf};

#[derive(Debug)]
struct ModifyOperation {
    world_path: PathBuf,
    region: (usize, usize),
    chunk: (usize, usize),
    coordinates: (usize, usize, usize),
    block: BlockPalette,
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

        // FIXME: possibily of too small palette .trailing_zeros() is only 32bit, while data bits can be upto 64
        let bit_count: u32 = state
            .palette
            .len()
            .next_power_of_two()
            .trailing_zeros()
            .max(4);
        //println!("Bit count for palette: {bit_count}");

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

        //println!("Coordinates: ({}, {}, {})", x, y, z);

        let index = x + z * 16 + y * 16 * 16;

        old_indexes[index as usize] =
            state.palette.iter().position(|b| b == &self.block).unwrap() as i64;

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

        //println!("Output data length: {}", output_data.len());

        state.data = LongArray::new(new_blockdata);
        section.block_states = state;
        section.to_value()
    }
}

#[derive(Debug, Deserialize)]
struct Section {
    #[serde(rename = "SkyLight", default = "byte_array_default")]
    sky_light: ByteArray,
    #[serde(rename = "BlockLight", default = "byte_array_default")]
    block_light: ByteArray,
    #[serde(rename = "Y")]
    y: i8,
    biomes: Biomes,
    block_states: BlockStates,
}

impl Section {
    fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.sky_light.is_empty() {
            map.insert("SkyLight".into(), Value::ByteArray(self.sky_light));
        }
        if !self.block_light.is_empty() {
            map.insert("BlockLight".into(), Value::ByteArray(self.block_light));
        }
        map.insert("Y".into(), Value::Byte(self.y));
        map.insert("biomes".into(), self.biomes.to_value());
        map.insert("block_states".into(), self.block_states.to_value());

        Value::Compound(map)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Biomes {
    #[serde(default = "long_array_default")]
    data: LongArray,
    #[serde(default)]
    palette: Vec<String>,
}

impl Biomes {
    fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.data.is_empty() {
            map.insert("data".into(), Value::LongArray(self.data));
        }
        map.insert("palette".into(), to_value(self.palette).unwrap());

        Value::Compound(map)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct BlockStates {
    #[serde(default = "long_array_default")]
    data: LongArray,
    #[serde(default)]
    palette: Vec<BlockPalette>,
}

impl BlockStates {
    fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.data.is_empty() {
            map.insert("data".into(), Value::LongArray(self.data));
        }
        map.insert(
            "palette".into(),
            to_value(
                self.palette
                    .iter()
                    .map(|p| p.clone().to_value())
                    .collect::<Vec<Value>>(),
            )
            .unwrap(),
        );

        Value::Compound(map)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
struct BlockPalette {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Properties", default)]
    properties: HashMap<String, String>,
}

impl BlockPalette {
    fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.properties.is_empty() {
            map.insert("Properties".into(), self.properties_to_value());
        }
        map.insert("Name".into(), Value::String(self.name));

        Value::Compound(map)
    }

    fn properties_to_value(&self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        for (key, value) in &self.properties {
            map.insert(key.to_string(), Value::String(value.to_string()));
        }

        Value::Compound(map)
    }
}

fn byte_array_default() -> ByteArray {
    ByteArray::new(vec![])
}

fn long_array_default() -> LongArray {
    LongArray::new(vec![])
}

fn main() {
    let op = ModifyOperation {
        world_path: PathBuf::from(""),
        region: (0, 0),
        chunk: (0, 0),
        coordinates: (12, 111, 5),
        block: BlockPalette {
            name: String::from("minecraft:bedrock"),
            properties: HashMap::new(),
        },
    };

    println!("{op:?}");

    op.run();
}
