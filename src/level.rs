use std::{collections::HashMap, fs::File, io::Write, path::PathBuf};

use fastnbt::Value;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};

use crate::{error::RustEditError, world::World};

impl World {
    /// Returns the root compound tag from the `level.dat` file within a [`World`]  
    ///
    /// See the [minecraft.wiki](https://minecraft.wiki/w/Java_Edition_level_format#level.dat_format) for keys, values & the format.  
    pub fn get_level_dat(&self) -> Result<HashMap<String, Value>, RustEditError> {
        let file = File::open(self.get_level_dat_path())?;
        let decoder = GzDecoder::new(file);
        let nbt: Value = fastnbt::from_reader(decoder)?;

        let root = match nbt {
            Value::Compound(mut c) => {
                if let Some(Value::Compound(root)) = c.remove("Data") {
                    root
                } else {
                    return Err(RustEditError::WorldError(
                        "Missing 'Data' tag in level.dat".into(),
                    ));
                }
            }
            _ => {
                return Err(RustEditError::WorldError(
                    "Missing root in level.dat".into(),
                ));
            }
        };

        Ok(root)
    }

    /// Sets, and thus overwrites the level.dat file within a [`World`]
    ///
    /// See [`World::get_level_dat`] to get an existing value & more info.  
    pub fn set_level_dat(&self, value: HashMap<String, Value>) -> Result<(), RustEditError> {
        let mut file = File::create(self.get_level_dat_path())?;
        let mut encoder = GzEncoder::new(&mut file, Compression::default());

        // wrap it around Data first
        let mut root = HashMap::new();
        root.insert("Data", Value::Compound(value));

        let mut nbt_data = fastnbt::to_bytes(&root)?;
        encoder.write_all(&mut nbt_data)?;

        Ok(())
    }

    /// Returns the entire path to the [`World`]'s `level.dat` file
    fn get_level_dat_path(&self) -> PathBuf {
        self.path.join("level.dat")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const WORLD_PATH: &'static str = "test_world";

    #[test]
    fn get_level_dat() -> Result<(), RustEditError> {
        let level = World::new(&WORLD_PATH).get_level_dat()?;
        let data_version = match level
            .get("DataVersion")
            .ok_or(RustEditError::Other("No DataVersion in level.dat".into()))?
        {
            Value::Int(dv) => *dv,
            _ => {
                return Err(RustEditError::Other(
                    "Invalid DataVersion in level.dat".into(),
                ));
            }
        };
        Ok(assert!(data_version > 0))
    }

    #[test]
    fn set_level_dat() -> Result<(), RustEditError> {
        let world = World::new(&WORLD_PATH);
        let level = world.get_level_dat()?;
        world.set_level_dat(level)?;
        Ok(())
    }
}
