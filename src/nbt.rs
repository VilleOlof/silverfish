//! `nbt` contains the [`Block`] struct used to set/get blocks and its associated functions and data.  

use crate::error::{Error, Result};
use simdnbt::{
    Mutf8Str, Mutf8String,
    owned::{NbtCompound, NbtTag},
};
use std::{borrow::Cow, collections::BTreeMap, fmt::Debug, hash::Hash};

/// A [`Mutf8String`] in disguise. (See it for more info on this string type)
///
/// Wrapper for it since [`Mutf8String`] doesn't implement [`Hash`].  
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NbtString(pub(crate) Vec<u8>);

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
#[derive(Clone, PartialEq, Eq)]
pub struct Block {
    pub name: Name,
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
    /// ```no_run
    /// let conduit = Block::try_new_with_props("conduit", &[("pickles", "4")])?;
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
    /// ```no_run
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
    /// ```no_run
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

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // writes it in format of "<id>" if only id
        // if props, then <id>[<key> = <value>, <key> = <value>, ...]
        // a bit like how minecraft stores the block in snbt but its not snbt
        write!(
            f,
            "{}{}",
            self.name.to_str(),
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

impl Into<Block> for &str {
    fn into(self) -> Block {
        Block::new(self)
    }
}

impl Into<Block> for String {
    fn into(self) -> Block {
        Block::new(self)
    }
}

impl Hash for Block {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.name.as_nbt_string().0);
        if let Some(props) = &self.properties {
            for (k, v) in props {
                state.write(&k.0);
                state.write(&v.0);
            }
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
        if self.name.into_cow_namespaced().to_mutf8str() != name {
            return false;
        }

        if let Some(block_props) = &self.properties {
            let props = match other.compound("Properties") {
                Some(props) => props,
                None => return false,
            };

            let mut other_map: BTreeMap<NbtString, NbtString> = BTreeMap::new();

            for (k, v) in props.iter() {
                let k = match NbtString::from_mutf8str(Some(&k)) {
                    Some(k) => k,
                    None => return false,
                };
                let v = match NbtString::from_mutf8str(v.string()) {
                    Some(v) => v,
                    None => return false,
                };

                other_map.insert(k, v);
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

impl Debug for NbtString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

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

impl PartialEq<NbtString> for Mutf8String {
    fn eq(&self, other: &NbtString) -> bool {
        let str = other.to_mutf8str();
        self.as_str() == str
    }
}

impl PartialEq<NbtString> for &Mutf8String {
    fn eq(&self, other: &NbtString) -> bool {
        let str = other.to_mutf8str();
        self.as_str() == str
    }
}

impl PartialEq<NbtString> for &Mutf8Str {
    fn eq(&self, other: &NbtString) -> bool {
        let str = other.to_mutf8str();
        self == &str
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

impl Into<NbtString> for String {
    fn into(self) -> NbtString {
        NbtString::from_str(&self).expect("Failed to convert string to NbtString")
    }
}

impl Into<NbtString> for &Mutf8Str {
    fn into(self) -> NbtString {
        NbtString::from_mutf8str(Some(self)).expect("Failed to convert mutf8str to NbtString")
    }
}

impl Into<NbtString> for Mutf8String {
    fn into(self) -> NbtString {
        NbtString::from_mutf8str(Some(self.as_str()))
            .expect("Failed to convert mutf8string to NbtString")
    }
}

/// A block name, an enum to decide if it contains a namespace or not.  
///
/// If you know that your blocks already contain a namespace, you can safetely construct a [`Name::Namespaced`].  
///
/// Otherwise if you don't know, construct a [`Name::Id`] and  
/// let it auto-translate it to a namespaced on if it doesn't already contain a namespace
/// The to namespace translation only happens once the name is actually writtent to NBT.  
#[derive(Clone, Eq, Hash)]
pub enum Name {
    Namespaced(NbtString),
    Id(NbtString),
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

impl Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Name::into_namespaced(self.clone()).to_str())
    }
}

impl Into<Mutf8String> for Name {
    fn into(self) -> Mutf8String {
        match self {
            Name::Namespaced(n) => n.to_mutf8string(),
            Name::Id(n) => n.to_mutf8string(),
        }
    }
}

impl Into<Name> for String {
    fn into(self) -> Name {
        Name::Id(self.into())
    }
}

impl Into<Name> for &str {
    fn into(self) -> Name {
        Name::Id(self.into())
    }
}

impl PartialEq<Name> for &str {
    fn eq(&self, other: &Name) -> bool {
        other.to_str() == *Cow::Borrowed(self)
    }
}

impl PartialEq<Name> for Name {
    fn eq(&self, other: &Name) -> bool {
        other.to_str().as_str() == self.to_str().as_str()
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.to_str() == *Cow::Borrowed(other)
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
