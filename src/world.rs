use fastnbt::{LongArray, Value};
use mca::{PendingChunk, RegionReader, RegionWriter};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{
    config::FlushConfig,
    coordinate::{Coordinate, CoordinateType},
    error::RustEditError,
    nbt::Section,
    operation::{Operation, OperationData, SplitUnit},
};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Read},
    ops::Deref,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// A Minecraft "World"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub path: PathBuf,
    #[cfg(feature = "spigot")]
    pub world_name: String,
    pub operations: Vec<OperationData>,
}

/// A Minecraft dimension  
///
/// [`Dimension::Custom`] can be used for modded/custom dimensions  
///
/// The string passed to [`Dimension::Custom`] should be the entire path until the folder with the `.mca` files from the [`World`] root path.  
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub enum Dimension {
    #[default]
    Overworld,
    Nether,
    End,
    Custom(String),
}

impl Dimension {
    /// Returns the [`PathBuf`] to the correct region folder depending on the [`Dimension`]
    #[cfg(not(feature = "spigot"))]
    pub fn path(&self, path: &Path) -> PathBuf {
        let path = match self {
            Dimension::Overworld => path.to_path_buf(),
            Dimension::Nether => path.join("DIM-1"),
            Dimension::End => path.join("DIM1"),
            Dimension::Custom(dim) => path.join(dim),
        };

        // append /region to default vanilla dimensions
        // so Custom can be whatever folder within the world
        if let Dimension::Custom(_) = self {
            path
        } else {
            path.join("region")
        }
    }

    /// Returns the [`PathBuf`] to the correct region folder depending on the [`Dimension`]
    #[cfg(feature = "spigot")]
    pub fn path(&self, path: &Path, world_name: &str) -> PathBuf {
        let path = match self {
            Dimension::Overworld => path.to_path_buf(),
            Dimension::Nether => path.join(format!("{world_name}_nether")).join("DIM-1"),
            Dimension::End => path.join(format!("{world_name}_the_end")).join("DIM1"),
            Dimension::Custom(dim) => path.join(dim),
        };

        // append /region to default vanilla dimensions
        // so Custom can be whatever folder within the world
        if let Dimension::Custom(_) = self {
            path
        } else {
            path.join("region")
        }
    }
}

impl World {
    /// Whatever status the chunks needs to be to allow modification.  
    const REQUIRED_STATUS: &'static str = "minecraft:full";
    /// the minimum dataversion that light updating works on.
    /// since "isLightOn" was added in 1.18 (i think)
    pub const MIN_LIGHT_DATA_VERSION: i32 = 2860;

    /// Creates a new World instance to work on
    #[cfg(not(feature = "spigot"))]
    pub fn new<P>(world_path: P) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            path: world_path.as_ref().to_path_buf(),
            operations: vec![],
        }
    }

    /// Creates a new World instance to work on
    #[cfg(feature = "spigot")]
    pub fn new<P, N>(world_path: P, world_name: N) -> Self
    where
        P: AsRef<Path>,
        N: AsRef<str>,
    {
        Self {
            path: world_path.as_ref().to_path_buf(),
            world_name: world_name.as_ref().to_string(),
            operations: vec![],
        }
    }

    pub fn memory() -> Self {
        Self {
            path: PathBuf::new(),
            operations: Vec::new(),
        }
    }

    /// Pushes an [`Operation`] to the current [`World`] to be "flushed" later  
    ///
    /// Creates an [`OperationData`] that operate in [`Dimension::default`]
    pub fn push_op(&mut self, operation: Operation) -> &mut Self {
        self.push_operation_data(OperationData {
            dimension: Dimension::default(),
            operation,
        })
    }

    /// Pushes an [`OperationData`] like [`Self::push_op`]
    ///
    /// but [`OperationData`] includes which dimension to operate on.  
    pub fn push_operation_data(&mut self, data: OperationData) -> &mut Self {
        self.operations.push(data);
        self
    }

    pub fn flush(&mut self, config: FlushConfig) -> Result<(), RustEditError> {
        let instant = Instant::now();
        let mut operations: Vec<OperationData> = vec![];
        for operation in &self.operations {
            match &operation.operation {
                // setblocks cant be spanned across multiple areas so just as
                Operation::Setblock {
                    coordinate: _,
                    block: _,
                } => operations.push(operation.clone()),
                Operation::Fill {
                    from: _,
                    to: _,
                    block: _,
                } => {
                    // resolve fill that spans multiple regions/chunks/sections into sections
                    let mut section: Vec<OperationData> =
                        Operation::split_fill_into(&operation.operation, SplitUnit::Region)?
                            .iter()
                            .map(|r| Operation::split_fill_into(&r, SplitUnit::Chunk))
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .flatten()
                            .map(|c| Operation::split_fill_into(&c, SplitUnit::Section))
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .flatten()
                            .map(|o| OperationData {
                                dimension: operation.dimension.clone(),
                                operation: o,
                            })
                            .collect();
                    operations.append(&mut section);
                }
            }
        }

        let groups = World::group_operations(operations);
        println!("Took {:?} to resolve & group operations", instant.elapsed());

        groups
            .into_par_iter()
            .try_for_each(|region_group| self.handle_region(&config, region_group))?;

        Ok(())
    }

    /// Returns a specific region file from it's coordinates and region folder
    fn get_mca_file(region_folder: PathBuf, coordinate: &Coordinate) -> PathBuf {
        let coords = if coordinate._type != CoordinateType::Region {
            &coordinate.as_region()
        } else {
            coordinate
        };

        region_folder.join(format!("r.{}.{}.mca", coords.x(), coords.z()))
    }

    /// Core function for handling regions, iterating over chunks, sections,  
    /// and running all the operations  
    fn handle_region(
        &self,
        config: &FlushConfig,
        region_group: RegionGroup,
    ) -> Result<(), RustEditError> {
        let instant = Instant::now();
        // load region
        #[cfg(not(feature = "spigot"))]
        let mut region_path = region_group.dimension.path(&self.path);
        #[cfg(feature = "spigot")]
        let mut region_path = region_group.dimension.path(&self.path, &self.world_name);

        
        region_path = World::get_mca_file(region_path, &region_group.region_coordinate);

        let mut region_buf = vec![];
        let region =  if !config.in_memory {
            File::open(&region_path)?.read_to_end(&mut region_buf)?;
            RegionReader::new(region_buf.as_ref())?
        } else {
            let local = RegionWriter::new();
            let mut local_writer = BufWriter::new(Vec::new());
            local.write(&mut local_writer)?;
            region_buf = local_writer.into_inner().unwrap();
            RegionReader::new(&region_buf.as_slice())?
        };

        let mut writer = RegionWriter::new();
        let mut modified_chunk_locals: Vec<(usize, usize)> = vec![];

        for chunk_group in region_group.chunk_operations {
            let (local_chunk_x, local_chunk_z) = {
                let mut local_x = chunk_group.chunk_coordinate.x() % 32;
                let mut local_z = chunk_group.chunk_coordinate.z() % 32;
                if local_x.signum() == -1 {
                    local_x += 32;
                }
                if local_z.signum() == -1 {
                    local_z += 32;
                }

                (local_x as usize, local_z as usize)
            };
            modified_chunk_locals.push((local_chunk_x, local_chunk_z));

            let chunk = region.get_chunk(local_chunk_x, local_chunk_z)?;
            let chunk_data = match chunk {
                Some(c) => {
                    c.decompress()?
                },
                None => {
                    // TODO no clue what we do in this, just return, exit everything? skip? warn user? how?
                    // eprintln!("Tried to operate on a chunk that hasn't been generated");
                    //
                    // im also unsure about this but for now lets create a fake chunk
                    // this adds the possibility of creating chunks without having an input world - ris

                    let mut sections: Vec<Value> = vec![];

                    for y in -4..=19 {
                        sections.push(fastnbt::nbt!({
                            "Y": y as i8,
                            "biomes": {
                                "palette": [
                                    "minecraft:plains"
                                ]
                            },
                            "block_states": {
                                "palette": [
                                    {
                                        "Name": "minecraft:air"
                                    }
                                ]
                            }
                        }));
                    }


                    let chunk = fastnbt::nbt!({
                         "Status": "minecraft:full",
                         "DataVersion": 2860, //1.18
                         "sections": sections,
                         "xPos": local_chunk_x,
                         "zPos": local_chunk_z
                    });

                    fastnbt::to_bytes(&chunk)?
                }
            };
            

            let mut chunk_nbt: Value = fastnbt::from_bytes(&chunk_data)?;
            let root = match &mut chunk_nbt {
                Value::Compound(r) => r,
                _ => {
                    return Err(RustEditError::WorldError(
                        "No root compound for chunk".into(),
                    ));
                }
            };
            // check- status
            let chunk_status = root
                .get("Status")
                .ok_or(RustEditError::WorldError("No Status field in chunk".into()))?;
            if chunk_status != World::REQUIRED_STATUS {
                eprintln!("Tried to operate on a chunk that hasn't been generated");
                continue;
            }

            let data_version = root.get("DataVersion").ok_or(RustEditError::WorldError(
                "No DataVersion field in chunk".into(),
            ))?;
            match data_version {
                Value::Int(data_version) => {
                    if config.update_lighting && *data_version < World::MIN_LIGHT_DATA_VERSION {
                        return Err(RustEditError::WorldError(format!(
                            "Tried to update lighting on a version prior to DataVersion {}",
                            World::MIN_LIGHT_DATA_VERSION
                        )));
                    }
                }
                _ => {
                    return Err(RustEditError::WorldError(
                        "Invalid data type for DataVersion".into(),
                    ));
                }
            }

            if config.update_lighting {
                root.insert("isLightOn".into(), Value::Byte(0));
            }

            let sections = root
                .get_mut("sections")
                .ok_or(RustEditError::WorldError("no sections in chunk".into()))?;

            let mut modified_sections = vec![];
            let modified_section_indexes: Vec<isize> = chunk_group
                .section_operations
                .iter()
                .map(|s| s.section_idx)
                .collect();

            for section_group in chunk_group.section_operations {
                let section_idx = section_group.section_idx;
                let operations = section_group.operations;

                let mut section = Section::get_from_idx(&sections, section_idx)?;

                // deconstruct data/palette
                let mut state = section.block_states;

                if config.update_lighting {
                    // we use "onLightOn" and let Minecraft itself re-calculate lighting when first loaded
                    // so if any section is modified, we delete its blockLight & skyLight data
                    section.block_light = None;
                    section.sky_light = None;
                }

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

                //populate empty sections
                if old_indexes.len() == 0 {
                    old_indexes = vec![0; 4096];
                }

                // modify, run operations
                // these operations are 100% to be within this exact same section
                for operation in operations {
                    match operation {
                        Operation::Setblock { coordinate, block } => {
                            if !state.palette.contains(&block) {
                                state.palette.push(block.clone());
                            }

                            let (x, y, z) = (
                                // broder kan inte matematik 5, lär dig regler gällande kongruens tack
                                coordinate.x() & 15,
                                coordinate.y() & 15,
                                coordinate.z() & 15,
                            );

                            let index = x + z * 16 + y * 16 * 16;

                            old_indexes[index as usize] =
                                state.palette.iter().position(|b| b == &block).unwrap() as i64;
                        }
                        Operation::Fill { from, to, block } => {
                            if !state.palette.contains(&block) {
                                state.palette.push(block.clone());
                            }

                            let (from_x, from_y, from_z) =
                                (from.x() & 15, from.y() & 15, from.z() & 15);

                            let (to_x, to_y, to_z) = (to.x() & 15, to.y() & 15, to.z() & 15);

                            // im lazy
                            let start_x = from_x.min(to_x);
                            let start_y = from_y.min(to_y);
                            let start_z = from_z.min(to_z);

                            let end_x = from_x.max(to_x);
                            let end_y = from_y.max(to_y);
                            let end_z = from_z.max(to_z);

                            for x in start_x..=end_x {
                                for y in start_y..=end_y {
                                    for z in start_z..=end_z {
                                        let index = x + z * 16 + y * 16 * 16;

                                        old_indexes[index as usize] =
                                            state.palette.iter().position(|b| b == &block).unwrap()
                                                as i64;
                                    }
                                }
                            }
                        }
                    }
                }

                // construct data/palette
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

                modified_sections.push(section.to_value());
            }

            // reconstruct "sections"
            let new_sections: Vec<Value> = match sections {
                Value::List(sections) => {
                    let mut new_sections: Vec<Value> = modified_sections;
                    for sect in sections {
                        match sect {
                            // we skip any we have modified since they're already in the vec
                            Value::Compound(c)
                                if is_modified_section(&modified_section_indexes, c.deref())? =>
                            {
                                continue;
                            }
                            _ => (),
                        }

                        new_sections.push(sect.deref().clone());
                    }

                    new_sections
                }
                _ => return Err(RustEditError::WorldError("section isn't a list".into())),
            };

            root.insert("sections".into(), Value::List(new_sections));
            let raw_chunk = fastnbt::to_bytes(&chunk_nbt)?;

            writer.push_chunk_with_compression(
                &raw_chunk,
                (local_chunk_x as u8, local_chunk_z as u8),
                config.chunk_compression.clone(),
            )?;
        }

        // for every chunk not in region_group.chunk_operations
        // copy it over to a new RegionWriter (just need to clone the compressed data)
        if !config.in_memory {
            for (i, chunk) in region.iter().enumerate() {
                let chunk = chunk?;
                let chunk = match chunk {
                    Some(c) => c,
                    None => continue,
                };
                let (local_x, local_z) = (i % 32, i / 32);
    
                // if we have already written it to the writer
                if modified_chunk_locals.contains(&(local_x, local_z)) {
                    continue;
                }
    
                let timestamp = {
                    let start = SystemTime::now();
                    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
                    since_the_epoch.to_be()
                };
    
                // just move over the data to the writer with no modification if we didnt touch it
                writer.push_pending_chunk(PendingChunk::new_compressed(
                    chunk.raw_data.to_vec(),
                    chunk.get_compression_type(),
                    timestamp,
                    (local_x as u8, local_z as u8),
                )?);
            }
        }
        
        let output_file = PathBuf::from("output").join(region_path);

        fs::create_dir_all(&output_file.parent().expect("no parent")).unwrap();

        writer.write(&mut File::create(&output_file)?)?;
        println!(
            "Took {:?} to process r.{}.{}.mca",
            instant.elapsed(),
            region_group.region_coordinate.x(),
            region_group.region_coordinate.z()
        );

        Ok(())
    }

    /// Groups operations   
    ///
    /// - first by their **region [`Coordinate`]** and **[`Dimension`]**
    /// - then by **chunk [`Coordinate`]**
    /// - and lastly by **section index**
    fn group_operations(operations: Vec<OperationData>) -> Vec<RegionGroup> {
        let mut map: HashMap<
            Dimension,
            HashMap<Coordinate, HashMap<Coordinate, HashMap<isize, Vec<Operation>>>>,
        > = HashMap::new();

        for data in operations {
            let region_coords = data.operation.get_init_coords().as_region();
            let chunk_coords = data.operation.get_init_coords().as_chunk();
            let section_idx =
                (data.operation.get_init_coords().y() as f64 / 16f64).floor() as isize;

            map.entry(data.dimension)
                .or_default()
                .entry(region_coords)
                .or_default()
                .entry(chunk_coords)
                .or_default()
                .entry(section_idx)
                .or_default()
                .push(data.operation);
        }

        let mut region_groups = vec![];
        for (dimension, region_map) in map {
            for (region_coordinate, chunk_map) in region_map {
                let mut chunk_groups = vec![];

                for (chunk_coordinate, section_map) in chunk_map {
                    let mut section_groups = vec![];

                    for (section_idx, operations) in section_map {
                        section_groups.push(SectionGroup {
                            section_idx,
                            operations,
                        });
                    }

                    chunk_groups.push(ChunkGroup {
                        chunk_coordinate,
                        section_operations: section_groups,
                    });
                }

                region_groups.push(RegionGroup {
                    dimension: dimension.clone(),
                    region_coordinate,
                    chunk_operations: chunk_groups,
                });
            }
        }

        region_groups
    }
}

/// A group region with it's coordinate and dimension data
/// along side it's grouped operations
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegionGroup {
    dimension: Dimension,
    region_coordinate: Coordinate,
    chunk_operations: Vec<ChunkGroup>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChunkGroup {
    chunk_coordinate: Coordinate,
    section_operations: Vec<SectionGroup>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionGroup {
    section_idx: isize,
    operations: Vec<Operation>,
}

fn is_modified_section(
    mod_indexes: &Vec<isize>,
    v: &HashMap<String, Value>,
) -> Result<bool, RustEditError> {
    let curr_y: i8 = match v
        .get("Y")
        .ok_or(RustEditError::WorldError("No Y in section".into()))?
    {
        Value::Byte(y) => *y,
        _ => return Err(RustEditError::WorldError("Y isn't a byte".into())),
    };

    Ok(mod_indexes.contains(&(curr_y as isize)))
}
