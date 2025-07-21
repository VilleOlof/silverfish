use fastnbt::Value;
use mca::{CompressionType, PendingChunk, RegionReader, RegionWriter};

use crate::{
    coordinate::Coordinate,
    error::RustEditError,
    nbt::Section,
    operation::{Operation, OperationData, SplitUnit},
};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
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

    pub fn flush(&mut self) -> Result<(), RustEditError> {
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

        for region_group in World::group_operations(operations) {
            // load region
            #[cfg(not(feature = "spigot"))]
            let region_path = region_group.dimension.path(&self.path);
            #[cfg(feature = "spigot")]
            let region_path = region_group.dimension.path(&self.path, &self.world_name);

            let mut region_buf = vec![];
            File::open(&region_path)?.read_to_end(&mut region_buf)?;
            let region = RegionReader::new(&region_buf)?;

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
                let chunk = match chunk {
                    Some(c) => c,
                    None => {
                        // TODO no clue what we do in this, just return, exit everything? skip? warn user? how?
                        eprintln!("Tried to operate on a chunk that hasn't been generated");
                        continue;
                    }
                };
                let chunk_data = chunk.decompress()?;
                let mut chunk_nbt: Value = fastnbt::from_bytes(&chunk_data)?;
                let root = match &mut chunk_nbt {
                    Value::Compound(r) => r,
                    _ => {
                        return Err(RustEditError::WorldError(
                            "No root compound for chunk".into(),
                        ));
                    }
                };

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

                    //   _    _ ______ _____  ______
                    //  | |  | |  ____|  __ \|  ____|
                    //  | |__| | |__  | |__) | |__
                    //  |  __  |  __| |  _  /|  __|
                    //  | |  | | |____| | \ \| |____
                    //  |_|  |_|______|_|  \_\______|

                    // TODO ris HERE RIISSS, KOD HÄR
                    // i havent tested any of this code after the grouping
                    // but i think this should read the region, ... and then save it correctly :)

                    // deconstruct data/palette

                    // modify, run operations
                    // these operations are 100% to be within this exact same section
                    for operation in operations {
                        //
                    }

                    // construct data/palette

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
                                    if is_modified_section(
                                        &modified_section_indexes,
                                        c.deref(),
                                    )? =>
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
                    CompressionType::Zlib,
                )?;
            }

            // for every chunk not in region_group.chunk_operations
            // copy it over to a new RegionWriter (just need to clone data, no deompress/compress for this data)

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
                    let since_the_epoch =
                        start.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
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

            // save region & move onto the next one
            writer.write(&mut File::create(&region_path)?)?;
        }

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
