use std::{
    fs::File,
    io::{Cursor, Read, Write},
    path::PathBuf,
};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound, NbtTag};

use crate::{error::RustEditError, world::World};

impl World {
    /// Returns the root compound tag from the `level.dat` file within a [`World`]  
    ///
    /// See the [minecraft.wiki](https://minecraft.wiki/w/Java_Edition_level_format#level.dat_format) for keys, values & the format.  
    pub fn get_level_dat(&self) -> Result<NbtCompound, RustEditError> {
        let file = File::open(self.get_level_dat_path())?;
        let mut decoder = GzDecoder::new(file);
        let mut bytes = vec![];
        decoder.read_to_end(&mut bytes)?;
        let nbt = simdnbt::owned::read(&mut Cursor::new(&bytes))?;

        let data = nbt
            .compound("Data")
            .ok_or(RustEditError::NbtError(
                "Missing 'Data' in level.dat".into(),
            ))?
            .clone();

        Ok(data)
    }

    /// Sets, and thus overwrites the level.dat file within a [`World`]
    ///
    /// See [`World::get_level_dat`] to get an existing value & more info.  
    pub fn set_level_dat(&self, value: NbtCompound) -> Result<(), RustEditError> {
        let mut file = File::create(self.get_level_dat_path())?;
        let mut encoder = GzEncoder::new(&mut file, Compression::default());

        // wrap it around Data first
        let root = Nbt::Some(BaseNbt::new(
            "",
            NbtCompound::from_values(vec![("Data".into(), NbtTag::Compound(value))]),
        ));

        let mut nbt_data = vec![];
        root.write(&mut nbt_data);
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
        let data_version = level
            .int("DataVersion")
            .ok_or(RustEditError::Other("No DataVersion in level.dat".into()))?;
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
