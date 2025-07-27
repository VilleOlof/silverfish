use std::{collections::BTreeMap, fmt::Debug};

use simdnbt::{
    Mutf8Str, Mutf8String,
    owned::{NbtCompound, NbtList, NbtTag},
};

use crate::error::RustEditError;

pub trait NbtConversion {
    fn from_compound(tag: &NbtCompound) -> Result<Self, RustEditError>
    where
        Self: Sized;
    fn to_compound(self) -> Result<NbtCompound, RustEditError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub sky_light: Option<Vec<u8>>,
    pub block_light: Option<Vec<u8>>,
    pub y: i8,
    pub biomes: Biomes,
    pub block_states: BlockStates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Biomes {
    pub data: Option<Vec<i64>>,
    pub palette: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockStates {
    pub data: Option<Vec<i64>>,
    pub palette: Vec<Block>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Block {
    pub name: NbtString,
    pub properties: Option<BTreeMap<NbtString, NbtString>>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtString(Vec<u8>);

impl Into<Mutf8String> for NbtString {
    fn into(self) -> Mutf8String {
        Self::to_mutf8string(self)
    }
}

impl NbtString {
    pub fn from_mutf8str(string: Option<&Mutf8Str>) -> Option<Self> {
        let data = string.map(|s| s.as_bytes().to_vec());
        match data {
            Some(d) => Some(Self(d)),
            None => None,
        }
    }

    pub fn to_mutf8string(self) -> Mutf8String {
        Mutf8String::from_vec(self.0)
    }

    pub fn to_mutf8str(&self) -> &Mutf8Str {
        Mutf8Str::from_slice(&self.0)
    }
}

impl NbtConversion for Section {
    fn from_compound(tag: &NbtCompound) -> Result<Self, RustEditError> {
        let sky_light = tag.byte_array("SkyLight").map(|b| b.to_vec());
        let block_light = tag.byte_array("BlockLight").map(|b| b.to_vec());
        let y = tag
            .byte("Y")
            .ok_or(RustEditError::NbtError("Missing 'Y' in section".into()))?;
        let biomes = tag.compound("biomes").ok_or(RustEditError::NbtError(
            "Missing 'biomes' in section".into(),
        ))?;
        let biomes = Biomes::from_compound(&biomes)?;

        let block_states = tag.compound("block_states").ok_or(RustEditError::NbtError(
            "Missing 'block_states' in section".into(),
        ))?;
        let block_states = BlockStates::from_compound(&block_states)?;

        Ok(Section {
            sky_light,
            block_light,
            y,
            biomes,
            block_states,
        })
    }

    fn to_compound(self) -> Result<NbtCompound, RustEditError> {
        let mut tag = NbtCompound::new();
        if let Some(sky_light) = self.sky_light {
            if !sky_light.is_empty() {
                tag.insert("SkyLight", NbtTag::ByteArray(sky_light));
            }
        }
        if let Some(block_light) = self.block_light {
            if !block_light.is_empty() {
                tag.insert("BlockLight", NbtTag::ByteArray(block_light));
            }
        }
        tag.insert("Y", NbtTag::Byte(self.y));

        tag.insert("biomes", self.biomes.to_compound()?);
        tag.insert("block_states", self.block_states.to_compound()?);

        Ok(tag)
    }
}

impl NbtConversion for Biomes {
    fn from_compound(tag: &NbtCompound) -> Result<Self, RustEditError> {
        let data = tag.long_array("data").map(|d| d.to_vec());
        let palette = tag.list("palette").ok_or(RustEditError::NbtError(
            "Missing 'palette' in biomes".into(),
        ))?;
        // TODO simdnbt doesnt expose Mutf8String, only Mutf8Str which is a reference one
        // Mutf8String also handles NBT specific string things so we would want that but uhh it doesnt expose it.
        let palette: Vec<String> = palette
            .strings()
            .map(|s| s.iter().map(|m| m.to_str().to_string()).collect())
            .ok_or(RustEditError::NbtError("Failed to get palette vec".into()))?;

        Ok(Biomes { data, palette })
    }

    fn to_compound(self) -> Result<NbtCompound, RustEditError> {
        let mut tag = NbtCompound::new();
        if let Some(data) = self.data {
            // if palette len is 1, skip writing data
            if self.palette.len() != 1 {
                tag.insert("data", NbtTag::LongArray(data));
            }
        }
        tag.insert(
            "palette",
            NbtTag::List(NbtList::String(
                self.palette
                    .iter()
                    .map(|s| Mutf8Str::from_str(&s).into_owned())
                    .collect(),
            )),
        );

        Ok(tag)
    }
}

impl NbtConversion for BlockStates {
    fn from_compound(tag: &NbtCompound) -> Result<Self, RustEditError> {
        let data = tag.long_array("data").map(|d| d.to_vec());

        let palette = tag.list("palette").ok_or(RustEditError::NbtError(
            "Missing 'palette' in biomes".into(),
        ))?;
        let palette: Vec<Block> = palette
            .compounds()
            .ok_or(RustEditError::NbtError(
                "Invalid palette structure in block states".into(),
            ))?
            .iter()
            .map(|p| Block::from_compound(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BlockStates { data, palette })
    }

    fn to_compound(self) -> Result<NbtCompound, RustEditError> {
        let mut tag = NbtCompound::new();

        if let Some(data) = self.data {
            // if palette len is 1, skip writing data
            if self.palette.len() != 1 {
                tag.insert("data", NbtTag::LongArray(data));
            }
        }
        let palette_nbt: Vec<NbtCompound> = self
            .palette
            .into_iter()
            .map(|b| b.to_compound())
            .collect::<Result<Vec<_>, _>>()?;
        tag.insert("palette", NbtList::Compound(palette_nbt));

        Ok(tag)
    }
}

impl NbtConversion for Block {
    fn from_compound(tag: &NbtCompound) -> Result<Self, RustEditError> {
        let name = NbtString::from_mutf8str(tag.string("Name")).ok_or(RustEditError::NbtError(
            "Missing 'name' in section palette".into(),
        ))?;

        let properties = match tag.compound("Properties") {
            // skip calculating if empty
            Some(props) if props.is_empty() => None,
            Some(props) => {
                let mut new_properties = BTreeMap::new();

                for (k, v) in props.iter() {
                    new_properties.insert(
                        NbtString::from_mutf8str(Some(k)).ok_or(RustEditError::NbtError(
                            "Property key is not a string in section block palette".into(),
                        ))?,
                        NbtString::from_mutf8str(v.string()).ok_or(RustEditError::NbtError(
                            "Property value is not a string in section block palette".into(),
                        ))?,
                    );
                }
                Some(new_properties)
            }
            None => None,
        };

        Ok(Block { name, properties })
    }

    fn to_compound(self) -> Result<NbtCompound, RustEditError> {
        let mut tag = NbtCompound::new();
        tag.insert("Name", NbtTag::String(self.name.into()));
        if let Some(props) = self.properties {
            // skip writing if properties is empty
            if !props.is_empty() {
                let mut props_tag = NbtCompound::new();
                for (k, v) in props {
                    props_tag.insert(k, NbtTag::String(v.into()));
                }
                tag.insert("Properties", props_tag);
            }
        }

        Ok(tag)
    }
}

impl Section {
    pub fn get_from_idx(sections: &NbtList, idx: isize) -> Result<Self, RustEditError> {
        let compound_list = sections.compounds().ok_or(RustEditError::NbtError(
            "Sections is the wrong list data type".into(),
        ))?;

        for c in compound_list {
            let y = c
                .byte("Y")
                .ok_or(RustEditError::WorldError("Missing 'Y' in section".into()))?
                as isize;
            if y == idx {
                let section = Section::from_compound(c)?;
                return Ok(section);
            }
        }

        Err(RustEditError::WorldError(
            "no section found with a valid index".into(),
        ))
    }
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.name.to_mutf8str().to_str(),
            if let Some(props) = &self.properties {
                format!(
                    "[{}]",
                    props
                        .iter()
                        .map(|(k, v)| format!(
                            "{}={}",
                            k.to_mutf8str().to_str(),
                            v.to_mutf8str().to_str()
                        ))
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            } else {
                "".to_owned()
            }
        )
    }
}

impl Block {
    /// Creates a new block from just an id
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    pub fn new<B: AsRef<str>>(block: B) -> Self {
        let name = block.as_ref().to_string();
        Block {
            name: if name.contains(":") {
                NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&name)))
                    .expect("Failed to convert block name to Mutf8Str")
            } else {
                NbtString::from_mutf8str(Some(&Mutf8Str::from_str(
                    &(String::from("minecraft:") + &name),
                )))
                .expect("Failed to convert block name to Mutf8Str")
            },
            properties: None,
        }
    }

    /// Creates a new block
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```
    /// let conduit = Block::new_with_props("conduit", [("pickles", "4")]);
    /// ```
    pub fn new_with_props<B: AsRef<str>, const N: usize>(
        block: B,
        properties: [(&str, &str); N],
    ) -> Self {
        let name = block.as_ref().to_string();
        Block {
            name: if name.contains(":") {
                NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&name)))
                    .expect("Failed to convert block name to Mutf8Str")
            } else {
                NbtString::from_mutf8str(Some(&Mutf8Str::from_str(
                    &(String::from("minecraft:") + &name),
                )))
                .expect("Failed to convert block name to Mutf8Str")
            },
            properties: Some(BTreeMap::from(properties.map(|(k, v)| {
                //
                let k = NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&k)))
                    .expect("Failed to convert block property key to Mutf8Str");
                let v = NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&v)))
                    .expect("Failed to convert block property value to Mutf8Str");
                (k, v)
            }))),
        }
    }
}
