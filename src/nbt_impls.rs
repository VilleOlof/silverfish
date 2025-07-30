use crate::{Block, Name, NbtString};
use simdnbt::{Mutf8Str, Mutf8String, owned::NbtCompound};
use std::{borrow::Cow, collections::BTreeMap, fmt::Debug, hash::Hash};

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

impl Into<Block> for Name {
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

impl Into<Name> for NbtString {
    fn into(self) -> Name {
        Name::Id(self)
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
