use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
};

use fastnbt::{ByteArray, LongArray, Value, to_value};
use serde::{Deserialize, Serialize};

use crate::error::RustEditError;

#[derive(Debug, Deserialize, Clone)]
pub struct Section {
    #[serde(rename = "SkyLight", default = "byte_array_default")]
    pub sky_light: ByteArray,
    #[serde(rename = "BlockLight", default = "byte_array_default")]
    pub block_light: ByteArray,
    #[serde(rename = "Y")]
    pub y: i8,
    pub biomes: Biomes,
    pub block_states: BlockStates,
}

impl Section {
    pub fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.sky_light.is_empty() {
            map.insert("SkyLight".into(), Value::ByteArray(self.sky_light));
        }
        if !self.block_light.is_empty() {
            map.insert("BlockLight".into(), Value::ByteArray(self.block_light));
        }
        map.insert("Y".into(), Value::Byte(self.y));
        map.insert("biomes".into(), self.biomes.to_value());
        map.insert("block_states".into(), self.block_states.to_value());

        Value::Compound(map)
    }

    pub fn get_from_idx(sections: &Value, idx: isize) -> Result<Self, RustEditError> {
        match sections {
            Value::List(sects) => {
                for (idddx, s_v) in sects.iter().enumerate() {
                    match s_v {
                        Value::Compound(c)
                            if c.get("Y").ok_or(RustEditError::WorldError(
                                "No Y value in section".into(),
                            ))? == idx =>
                        {
                            let section = fastnbt::from_value(s_v)?;
                            return Ok(section);
                        }
                        Value::Compound(_) => (),
                        _ => {
                            return Err(RustEditError::WorldError(
                                format!("section {} isn't a compound", idddx).into(),
                            ));
                        }
                    }
                }
            }
            _ => return Err(RustEditError::WorldError("sections isn't a list".into())),
        }

        Err(RustEditError::WorldError(
            "no section found with a valid index".into(),
        ))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Biomes {
    #[serde(default = "long_array_default")]
    pub data: LongArray,
    #[serde(default)]
    pub palette: Vec<String>,
}

impl Biomes {
    pub fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.data.is_empty() {
            map.insert("data".into(), Value::LongArray(self.data));
        }
        map.insert("palette".into(), to_value(self.palette).unwrap());

        Value::Compound(map)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlockStates {
    #[serde(default = "long_array_default")]
    pub data: LongArray,
    #[serde(default)]
    pub palette: Vec<Block>,
}

impl BlockStates {
    pub fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if !self.data.is_empty() {
            map.insert("data".into(), Value::LongArray(self.data));
        }
        map.insert(
            "palette".into(),
            to_value(
                self.palette
                    .iter()
                    .map(|p| p.clone().to_value())
                    .collect::<Vec<Value>>(),
            )
            .unwrap(),
        );

        Value::Compound(map)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Hash)]
pub struct Block {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Properties", default)]
    pub properties: Option<BTreeMap<String, String>>,
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.name,
            if let Some(props) = &self.properties {
                &format!(
                    "[{}]",
                    props
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            } else {
                ""
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
                name
            } else {
                String::from("minecraft:") + &name
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
                name
            } else {
                String::from("minecraft:") + &name
            },
            properties: Some(BTreeMap::from(
                properties.map(|(k, v)| (k.to_string(), v.to_string())),
            )),
        }
    }

    pub fn to_value(self) -> Value {
        let mut map = HashMap::<String, Value>::new();

        if let Some(props) = self.properties {
            if !props.is_empty() {
                map.insert("Properties".into(), Block::properties_to_value(props));
            }
        }
        map.insert("Name".into(), Value::String(self.name));

        Value::Compound(map)
    }

    pub fn properties_to_value(props: BTreeMap<String, String>) -> Value {
        let mut map = HashMap::<String, Value>::new();

        for (key, value) in &props {
            map.insert(key.to_string(), Value::String(value.to_string()));
        }

        Value::Compound(map)
    }
}

fn byte_array_default() -> ByteArray {
    ByteArray::new(vec![])
}

fn long_array_default() -> LongArray {
    LongArray::new(vec![])
}
