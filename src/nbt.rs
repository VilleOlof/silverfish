//! `nbt` contains the [`Block`] struct used to set/get blocks and its associated functions and data.  

use crate::error::{Error, Result};
use simdnbt::{
    Mutf8Str, Mutf8String,
    owned::{NbtCompound, NbtTag},
};
use std::{borrow::Cow, collections::BTreeMap, hash::Hash};

/// A Minecraft [Block](https://minecraft.wiki/w/Block), used when setting blocks or when retrieving blocks
#[derive(Clone, PartialEq, Eq)]
pub struct Block {
    /// The id of the block, look at [`Name`] for more info.  
    pub name: Name,
    /// Optional block properties attached to the blocok.  
    pub properties: Option<BTreeMap<NbtString, NbtString>>,
}

/// A [`Mutf8String`] in disguise. (See it for more info on this string type)
///
/// Wrapper for it since [`Mutf8String`] doesn't implement [`Hash`].  
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtString(pub(crate) Vec<u8>);

/// A block name, an enum to decide if it contains a namespace or not.  
///
/// If you know that your blocks already contain a namespace, you can safetely construct a [`Name::Namespaced`].  
///
/// Otherwise if you don't know, construct a [`Name::Id`] and  
/// let it auto-translate it to a namespaced on if it doesn't already contain a namespace
/// The to namespace translation only happens once the name is actually writtent to NBT.  
#[derive(Clone, Eq, Hash)]
pub enum Name {
    /// A block name which has a namespace at the start `<namespace>:<id>`
    Namespaced(NbtString),
    /// A block name that may or may not have a namespace `<namespace?>:<id>`
    Id(NbtString),
}

impl NbtString {
    /// Creates a new [`NbtString`] from maybe an [`Mutf8Str`]
    pub fn from_mutf8str(string: Option<&Mutf8Str>) -> Option<Self> {
        let data = string.map(|s| s.as_bytes().to_vec());
        match data {
            Some(d) => Some(Self(d)),
            None => None,
        }
    }

    /// Creates a new [`Mutf8String`] from the [`NbtString]
    pub fn to_mutf8string(self) -> Mutf8String {
        Mutf8String::from_vec(self.0)
    }

    /// Creates a new [`Mutf8Str`] from the [`NbtString]
    pub fn to_mutf8str(&self) -> &Mutf8Str {
        Mutf8Str::from_slice(&self.0)
    }

    /// Creates a new [`NbtString`] from [`str`]
    pub fn from_str(value: &str) -> Result<Self> {
        NbtString::from_mutf8str(Some(&Mutf8Str::from_str(&value))).ok_or(Error::InvalidNbtType(
            "Failed to convert str to mutf8str & nbtstring",
        ))
    }

    /// Get the [`NbtString`] as a `Cow<'_, str>`
    pub fn to_str(&self) -> Cow<'_, str> {
        self.to_mutf8str().to_str()
    }

    /// Creates a new [`String`] from the [`NbtString`]
    pub fn to_string(&self) -> String {
        self.to_mutf8str().to_string()
    }

    /// Returns the inner [`Vec<u8>`] that represents this [`NbtString`]
    pub fn inner(&self) -> &Vec<u8> {
        &self.0
    }
}

impl Block {
    /// Tries to create a new block from it's id.  
    ///
    /// Fails if the id is an invalid [`Mutf8String`]  
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Block;
    /// let beacon = Block::try_new("beacon")?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn try_new<B: Into<Name>>(block: B) -> Result<Self> {
        Ok(Block {
            name: block.into(),
            properties: None,
        })
    }

    /// Tries to create a new block from it's id and properties
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Block;
    /// let conduit = Block::try_new_with_props("conduit", &[("pickles", "4")])?;
    /// # Ok::<(), silverfish::Error>(())
    /// ```
    pub fn try_new_with_props<B: Into<Name>>(
        block: B,
        properties: &[(&str, &str)],
    ) -> Result<Self> {
        let mut props = BTreeMap::new();
        for (k, v) in properties {
            let k = NbtString::from_str(&k)?;
            let v = NbtString::from_str(&v)?;
            props.insert(k, v);
        }

        Ok(Block {
            name: block.into(),
            properties: Some(props),
        })
    }

    /// Creates a new block from just an id
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Block;
    /// let beacon = Block::new("beacon");
    /// ```
    pub fn new<B: Into<Name>>(block: B) -> Self {
        Self::try_new(block).unwrap()
    }

    /// Creates a new block
    ///
    /// Auto populates into minecraft namespace if no namespace was given
    ///
    /// ## Example
    /// ```
    /// # use silverfish::Block;
    /// let conduit = Block::new_with_props("conduit", [("pickles", "4")]);
    /// ```
    pub fn new_with_props<B: Into<Name>, const N: usize>(
        block: B,
        properties: [(&str, &str); N],
    ) -> Self {
        Self::try_new_with_props(block, &properties).unwrap()
    }

    /// Populates a namespace to the id if none is given.  
    ///
    /// Defaults to `minecraft:<id>`
    pub(crate) fn populate_namespace(id: &str) -> Cow<'_, str> {
        // we first check if its just a minecraft namespace since its :
        // is on the 9th index and we can easily skip any further iterations.
        if let Some(maybe_colon) = id.chars().nth(9) {
            if maybe_colon == ':' {
                return Cow::Borrowed(id);
            }
        }

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

        Ok(Block {
            name: Name::Namespaced(name),
            properties,
        })
    }

    /// Converts [`Block`] to a [`NbtCompound`]  
    ///
    /// Skips writing `properties` if `None` or empty
    pub fn to_compound(self) -> Result<NbtCompound> {
        let mut tag = NbtCompound::new();
        tag.insert("Name", NbtTag::String(self.name.into_namespaced().into()));
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

impl Name {
    /// Creates a new [`Name`] from a **namespaced** id.  
    ///
    /// Only use if you're sure that your block contains a namespace.  
    pub fn new_namespace<S: Into<NbtString>>(value: S) -> Self {
        Name::Namespaced(value.into())
    }

    /// Creates a new [`Name`] that may or may not contain a namespace.  
    pub fn new_id<S: Into<NbtString>>(value: S) -> Self {
        Name::Id(value.into())
    }

    /// Converts the [`Name`] into a 100% guaranteed namespaced variant.  
    pub fn into_namespaced(self) -> Self {
        match self {
            Name::Namespaced(n) => Name::Namespaced(n),
            Name::Id(n) => Name::Namespaced(
                NbtString::from_str(&Block::populate_namespace(&n.to_str())).unwrap(),
            ),
        }
    }

    /// Converts the [`Name`] into a guaranteed namespaced variant, but it may be owned or borrowed.  
    pub fn into_cow_namespaced(&self) -> Cow<'_, NbtString> {
        match self {
            Name::Namespaced(n) => Cow::Borrowed(n),
            Name::Id(n) => {
                Cow::Owned(NbtString::from_str(&Block::populate_namespace(&n.to_str())).unwrap())
            }
        }
    }

    /// Converts [`Name`] into [`str`]
    pub fn to_str(&self) -> Cow<'_, str> {
        match self {
            Name::Namespaced(n) => n.to_str(),
            Name::Id(n) => n.to_str(),
        }
    }

    /// Extracts the internal [`NbtString`] value
    pub fn to_nbt_string(self) -> NbtString {
        match self {
            Name::Namespaced(n) => n,
            Name::Id(n) => n,
        }
    }

    /// A reference to the internal [`NbtString`] value
    pub fn as_nbt_string(&self) -> &NbtString {
        match self {
            Name::Namespaced(n) => &n,
            Name::Id(n) => &n,
        }
    }

    /// Converts [`Name`] into [`Mutf8Str`]
    pub fn to_mutf8str(&self) -> &Mutf8Str {
        match self {
            Name::Namespaced(n) => n.to_mutf8str(),
            Name::Id(n) => n.to_mutf8str(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_block() -> Result<()> {
        let block = Block::try_new("minecraft:air")?;
        assert_eq!(block.name, "minecraft:air");
        assert_eq!(block.properties, None);
        Ok(())
    }

    #[test]
    fn no_namespace_block() -> Result<()> {
        let block = Block::try_new("furnace")?;
        assert_eq!(block.name, "furnace");
        Ok(())
    }

    #[test]
    fn new_block_props() -> Result<()> {
        let block = Block::try_new_with_props("sea_pickle", &[("waterlogged", "true")])?;
        assert!(block.properties.is_some());
        assert_eq!(block.properties.clone().unwrap().len(), 1);
        assert_eq!(
            block
                .properties
                .unwrap()
                .get(&NbtString::from_str("waterlogged")?)
                .map(|b| b.to_string()),
            Some(String::from("true"))
        );
        Ok(())
    }

    #[test]
    fn nbt_string() -> Result<()> {
        let nbt_string = NbtString::from_str("arentyouexcited")?;
        assert!(nbt_string.inner().len() > 0);
        assert_eq!(nbt_string, "arentyouexcited");
        assert_eq!(nbt_string.to_string(), String::from("arentyouexcited"));
        Ok(())
    }

    #[test]
    fn simple_nbt_block_compare() -> Result<()> {
        let block = Block::try_new("minecraft:terracotta")?;
        let nbt = NbtCompound::from_values(vec![(
            "Name".into(),
            NbtTag::String("minecraft:terracotta".into()),
        )]);
        assert!(&block == &nbt);

        Ok(())
    }

    #[test]
    fn complex_nbt_block_compare() -> Result<()> {
        let block = Block::try_new_with_props("minecraft:furnace", &[("lit", "true")])?;
        let nbt = NbtCompound::from_values(vec![
            ("Name".into(), NbtTag::String("minecraft:furnace".into())),
            (
                "Properties".into(),
                NbtTag::Compound(NbtCompound::from_values(vec![(
                    "lit".into(),
                    NbtTag::String("true".into()),
                )])),
            ),
        ]);
        assert!(&block == &nbt);

        Ok(())
    }

    #[test]
    fn block_to_nbt() -> Result<()> {
        let block = Block::try_new("minecraft:redstone_block")?;
        let block_nbt = block.to_compound()?;
        let ref_nbt = NbtCompound::from_values(vec![(
            "Name".into(),
            NbtTag::String("minecraft:redstone_block".into()),
        )]);

        assert!(block_nbt == ref_nbt);

        Ok(())
    }

    #[test]
    fn nbt_to_block() -> Result<()> {
        let nbt = NbtCompound::from_values(vec![
            (
                "Name".into(),
                NbtTag::String("minecraft:mangrove_roots".into()),
            ),
            (
                "Properties".into(),
                NbtTag::Compound(NbtCompound::from_values(vec![(
                    "waterlogged".into(),
                    NbtTag::String("true".into()),
                )])),
            ),
        ]);
        let block = Block::from_compound(&nbt)?;

        assert!(&block == &nbt);

        Ok(())
    }

    #[test]
    fn populate_namespace() {
        let id = Block::populate_namespace("lime_concrete");
        assert_eq!(id, "minecraft:lime_concrete")
    }

    #[test]
    fn dont_populate_namespace() {
        let id = Block::populate_namespace("custom:lime_concrete");
        assert_eq!(id, "custom:lime_concrete")
    }

    #[test]
    fn compare_block_against_nbt() -> Result<()> {
        let block = Block::try_new("minecraft:beacon")?;
        let nbt = NbtCompound::from_values(vec![(
            "Name".into(),
            NbtTag::String("minecraft:beacon".into()),
        )]);

        assert!(&block == &nbt);

        Ok(())
    }

    #[test]
    fn compare_namespaced_name() {
        let name_1 = Name::new_namespace("minecraft:pink_concrete");
        let name_2 = Name::new_namespace("minecraft:pink_concrete");
        assert_eq!(name_1, name_2)
    }

    #[test]
    fn compare_diff_name() {
        let name_1 = Name::new_namespace("minecraft:lime_wool");
        let name_2 = Name::new_id("lime_wool");
        assert_ne!(name_1, name_2)
    }

    #[test]
    fn compare_into_namespaced() {
        let name_1 = Name::new_namespace("minecraft:white_stained_glass");
        let name_2 = Name::new_id("white_stained_glass");
        assert_eq!(name_1, name_2.into_namespaced())
    }
}
