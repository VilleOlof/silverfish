use mca::RegionReader;

use crate::{
    coordinate::Coordinate,
    error::RustEditError,
    operation::{Operation, OperationData},
};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
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
        // iterate over all operations and actually unpack the data, place all changes
        // and then pack it in and write the data?
        //
        // one big issue im seeing is if lets say a fill operation spans multiple chunks, even regions?
        // for setblocks at least, you could arrange them into a HashMap<(region_x, region_z), OperationData>
        // and batch update a single region with all possible setblocks
        //
        // fill operation (and other operations in the future) that spans multiple chunk/regions
        // you could take the first few chunks in a region etc and operate on them and possibly
        // calculate a operation that would fill out the remaining area and push that into the operations
        // so the main "for op in self.operations" would continue and continue until its all done

        let groups = World::group_by_region(self.operations.iter().collect());

        // depending on how the overlapping issue gets handled, this could even get multithreaded??
        for region in groups {
            let (dimension, region_coords, operations) = (
                region.dimension,
                region.region_coordinate,
                region.operations,
            );
            // also a nit-pick from myself to myself, a bit odd to have region path from the Dimension path but path-wise it makes sense for its grouping
            #[cfg(not(feature = "spigot"))]
            let region_path = dimension.path(&self.path);
            #[cfg(feature = "spigot")]
            let region_path = dimension.path(&self.path, &self.world_name);

            let mut region_buf = vec![];
            let region_data = File::open(&region_path)?.read_to_end(&mut region_buf)?;
            let region = RegionReader::new(&region_buf)?;

            // here group this specific "operations" into a "group_by_chunk"
            // to handle each and every chunk in a single batch
            // (and again as mentioned above we have the issue of overlapping fill operations across chunk/region boundaries)
        }

        Ok(())
    }

    /// Groups operations by their **region [`Coordinate`]** and **[`Dimension`]**
    fn group_by_region(operations: Vec<&OperationData>) -> Vec<RegionGroup> {
        let mut groups: HashMap<(Coordinate, Dimension), Vec<OperationData>> = HashMap::new();

        for data in operations {
            let region_coords = data.operation.get_coordinate().as_region();
            let dimension = data.dimension.clone();

            let entry = groups.get_mut(&(region_coords.clone(), dimension.clone()));
            if let Some(ent) = entry {
                ent.push(data.clone());
            } else {
                groups.insert((region_coords, dimension), vec![data.clone()]);
            }
        }

        groups
            .iter()
            .map(|((c, d), v)| RegionGroup {
                dimension: d.clone(),
                region_coordinate: c.clone(),
                operations: v.clone(),
            })
            .collect()
    }
}

/// A group region with it's coordinate and dimension data
/// along side it's grouped operations
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegionGroup {
    dimension: Dimension,
    region_coordinate: Coordinate,
    operations: Vec<OperationData>,
}
