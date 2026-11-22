//! Serde helper: a map with integer keys, written with string keys.
//!
//! JSON objects only have string keys. serde_json converts integer keys on
//! its own; serde-wasm-bindgen (the browser save / restore) does not, so the
//! saved maps use this helper: `#[serde(with = "crate::sim::str_keys")]`.
//! The JSON text is the same either way: `{ "65536": 99 }`.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<K: Display, V: serde::Serialize, S: Serializer>(m: &BTreeMap<K, V>, s: S) -> Result<S::Ok, S::Error> {
    s.collect_map(m.iter().map(|(k, v)| (k.to_string(), v)))
}

pub fn deserialize<'de, K, V, D>(d: D) -> Result<BTreeMap<K, V>, D::Error>
where K: FromStr + Ord, V: Deserialize<'de>, D: Deserializer<'de> {
    BTreeMap::<String, V>::deserialize(d)?
        .into_iter()
        .map(|(k, v)| k.parse().map(|k| (k, v)).map_err(|_| D::Error::custom(format!("bad map key {k:?}"))))
        .collect()
}
