//! Serialisation helpers.
//!
//! JSON object keys must be strings, so a `BTreeMap` with a *tuple* key (a grid
//! cell `(i32,i32)`, a learned transition `(i32,i32,u8)`, an association edge
//! `(u32,u32)`) cannot serialise to JSON directly. `vecmap` represents such a
//! map as a JSON array of `[key, value]` pairs instead — which is exactly what a
//! persistable, inspectable mind needs.

pub mod vecmap {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, s: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Ord,
        V: Serialize,
        S: Serializer,
    {
        let pairs: Vec<(&K, &V)> = map.iter().collect();
        pairs.serialize(s)
    }

    pub fn deserialize<'de, K, V, D>(d: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let pairs: Vec<(K, V)> = Vec::deserialize(d)?;
        Ok(pairs.into_iter().collect())
    }
}
