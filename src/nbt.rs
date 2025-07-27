//! `nbt` contains the [`Block`] struct used to set/get blocks and its associated functions and data.  

use crate::error::{Error, Result};
use simdnbt::{
    Mutf8Str, Mutf8String,
    owned::{NbtCompound, NbtTag},
};
use std::{borrow::Cow, collections::BTreeMap, fmt::Debug};

/// A [`Mutf8String`] in disguise. (See it for more info on this string type)
///
/// Wrapper for it since [`Mutf8String`] doesn't implement [`Hash`].  
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtString(pub(crate) Vec<u8>);

// impl partialeq for nbtstring and str/string

impl PartialEq<&str> for NbtString {
    fn eq(&self, other: &&str) -> bool {
        let str = self.to_str();
        &str == other
    }
}

impl PartialEq<NbtString> for &str {
    fn eq(&self, other: &NbtString) -> bool {
        let str = other.to_str();
        &str == self
    }
}

impl PartialEq<String> for NbtString {
    fn eq(&self, other: &String) -> bool {
        let str = self.to_str();
        str == other.as_str()
    }
}

impl PartialEq<NbtString> for String {
    fn eq(&self, other: &NbtString) -> bool {
        let str = other.to_str();
        self.as_str() == str
    }
}

impl Into<Mutf8String> for NbtString {
    fn into(self) -> Mutf8String {
        Self::to_mutf8string(self)
    }
}

impl Into<NbtString> for &str {
    fn into(self) -> NbtString {
        NbtString::from_str(&self).expect("Failed to convert str to NbtString")
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

    pub fn from_str(value: &str) -> Result<Self> {
        NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&value))).ok_or(Error::InvalidNbtType(
            "Failed to convert str to mutf8str & nbtstring",
        ))
    }

    pub fn to_str(&self) -> Cow<'_, str> {
        self.to_mutf8str().to_str()
    }

    pub fn to_string(&self) -> String {
        self.to_mutf8str().to_string()
    }

    pub fn inner(&self) -> &Vec<u8> {
        &self.0
    }
}

/// A Minecraft [Block](https://minecraft.wiki/w/Block), used when setting blocks or when retrieving blocks
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Block {
    pub name: NbtString,
    pub properties: Option<BTreeMap<NbtString, NbtString>>,
}

impl Block {
    /// Tries to create a new block from it's id.  
    ///
    /// Fails if the id is an invalid [`Mutf8String`]  
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```no_run
    /// let beacon = Block::try_new("beacon")?;
    /// ```
    pub fn try_new<B: AsRef<str>>(block: B) -> Result<Self> {
        let name = Self::populate_namespace(&block.as_ref());
        Ok(Block {
            name: NbtString::from_str(&name)?,
            properties: None,
        })
    }

    /// Tries to create a new block from it's id and properties
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```no_run
    /// let conduit = Block::try_new_with_props("conduit", &[("pickles", "4")])?;
    /// ```
    pub fn try_new_with_props<B: AsRef<str>>(
        block: B,
        properties: &[(&str, &str)],
    ) -> Result<Self> {
        let name = Self::populate_namespace(&block.as_ref());
        let mut props = BTreeMap::new();
        for (k, v) in properties {
            let k = NbtString::from_str(&k)?;
            let v = NbtString::from_str(&v)?;
            props.insert(k, v);
        }

        Ok(Block {
            name: NbtString::from_str(&name)?,
            properties: Some(props),
        })
    }

    /// Creates a new block from just an id
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```no_run
    /// let beacon = Block::new("beacon");
    /// ```
    pub fn new<B: AsRef<str>>(block: B) -> Self {
        Self::try_new(block).unwrap()
    }

    /// Creates a new block
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```no_run
    /// let conduit = Block::new_with_props("conduit", [("pickles", "4")]);
    /// ```
    pub fn new_with_props<B: AsRef<str>, const N: usize>(
        block: B,
        properties: [(&str, &str); N],
    ) -> Self {
        Self::try_new_with_props(block, &properties).unwrap()
    }

    /// Populates a namespace to the id if none is given.  
    ///
    /// Defaults to `minecraft:<id>`
    pub(crate) fn populate_namespace(id: &str) -> Cow<'_, str> {
        if !id.contains(":") {
            Cow::Owned(String::from("minecraft:") + &id)
        } else {
            Cow::Borrowed(id)
        }
    }

    /// Converts the NbtCompound to a [`Block`].  
    ///
    /// This should be the actual compound that contains the fields.
    pub fn from_compound(tag: &NbtCompound) -> Result<Self> {
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

    /// Converts [`Block`] to a [`NbtCompound`]  
    ///
    /// Skips writing `properties` if `None` or empty
    pub fn to_compound(self) -> Result<NbtCompound> {
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
        // writes it in format of "<id>" if only id
        // if props, then <id>[<key> = <value>, <key> = <value>, ...]
        // a bit like how minecraft stores the block in snbt but its not snbt
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

/// A custom PartialEq implementation so we dont need to convert NbtCompound to Block  
/// or Block to NbtCompound and can compare them fast
impl PartialEq<&NbtCompound> for &Block {
    fn eq(&self, other: &&NbtCompound) -> bool {
        let name = match other.string("Name") {
            Some(n) => n,
            None => return false,
        };
        if self.name.to_mutf8str() != name {
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
