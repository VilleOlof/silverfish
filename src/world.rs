use crate::{
    error::RustEditError,
    operation::{Operation, OperationData},
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    path: PathBuf,
    operations: Vec<OperationData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Dimension {
    #[default]
    Overworld,
    Nether,
    End,
    Custom(String),
}

impl Dimension {
    pub fn path(&self, world_path: &Path) -> PathBuf {
        let path = match self {
            Dimension::Overworld => world_path.to_path_buf(),
            Dimension::Nether => world_path.join("DIM-1"),
            Dimension::End => world_path.join("DIM1"),
            Dimension::Custom(dim) => world_path.join(dim),
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
    pub fn new<P>(world_path: P) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            path: world_path.as_ref().to_path_buf(),
            operations: vec![],
        }
    }

    pub fn push_op(&mut self, operation: Operation) -> &mut Self {
        self.push_operation_data(OperationData {
            dimension: Dimension::default(),
            operation,
        })
    }

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

        Ok(())
    }
}
