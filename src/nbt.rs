//! `nbt` contains the [`Block`] struct used to set/get blocks and its associated functions and data.  

use crate::error::{Error, Result};
use simdnbt::{
    Mutf8Str, Mutf8String,
    owned::{NbtCompound, NbtTag},
};
use std::{collections::BTreeMap, fmt::Debug};

pub trait NbtConversion {
    fn from_compound(tag: &NbtCompound) -> Result<Self>
    where
        Self: Sized;
    fn to_compound(self) -> Result<NbtCompound>;
}

/// A Minecraft [Block](https://minecraft.wiki/w/Block), used when setting blocks or when retrieving blocks
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

impl NbtConversion for Block {
    fn from_compound(tag: &NbtCompound) -> Result<Self> {
        let name =
            NbtString::from_mutf8str(tag.string("Name")).ok_or(Error::MissingNbtTag("Name"))?;

        let properties = match tag.compound("Properties") {
            // skip calculating if empty
            Some(props) if props.is_empty() => None,
            Some(props) => {
                let mut new_properties = BTreeMap::new();

                for (k, v) in props.iter() {
                    new_properties.insert(
                        NbtString::from_mutf8str(Some(k))
                            .ok_or(Error::InvalidNbtType("Properties > key"))?,
                        NbtString::from_mutf8str(v.string())
                            .ok_or(Error::InvalidNbtType("Properties > value"))?,
                    );
                }
                Some(new_properties)
            }
            None => None,
        };

        Ok(Block { name, properties })
    }

    fn to_compound(self) -> Result<NbtCompound> {
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

/// A custom PartialEq implementation so we dont need to convert NbtCompound to Block  
/// or Block to NbtCompound and can compare them fast
impl PartialEq<&NbtCompound> for &Block {
    fn eq(&self, other: &&NbtCompound) -> bool {
        let name = match other.string("Name") {
            Some(n) => n,
            None => return false,
        };
        if self.name.to_mutf8str().to_str() != name.to_str() {
            return false;
        }

        if let Some(block_props) = &self.properties {
            let props = match other.compound("Properties") {
                Some(props) => props,
                None => return false,
            };

            let mut other_map: BTreeMap<NbtString, NbtString> = BTreeMap::new();

            for (k, v) in props.iter() {
                other_map.insert(
                    // TODO cant really return result from PartialEq but maybe turn these into a default "false" ?
                    NbtString::from_mutf8str(Some(&k)).unwrap(),
                    NbtString::from_mutf8str(v.string()).unwrap(),
                );
            }

            if &other_map != block_props {
                return false;
            }
        } else {
            if other.contains("Properties") {
                return false;
            }
        }

        true
    }
}
