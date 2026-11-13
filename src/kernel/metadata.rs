//! Strongly-typed `Metadata` map enforcing Moyasar's 30-key, 40-char, 500-char limits.

use core::fmt;
use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::kernel::error::MoyasarError;

/// Maximum number of metadata entries permitted by Moyasar.
pub const METADATA_MAX_KEYS: usize = 30;
/// Maximum character length for a metadata key.
pub const METADATA_KEY_MAX_LEN: usize = 40;
/// Maximum character length for a metadata value.
pub const METADATA_VALUE_MAX_LEN: usize = 500;

/// Validated key/value metadata map for Moyasar resources.
#[derive(Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Metadata(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for Metadata {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = BTreeMap::<String, String>::deserialize(deserializer)?;
        let mut meta = Self::new();
        for (k, v) in raw {
            meta.insert(&k, &v).map_err(de::Error::custom)?;
        }
        Ok(meta)
    }
}

impl fmt::Debug for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metadata")
            .field("keys_count", &self.0.len())
            .finish()
    }
}

impl Metadata {
    /// Creates an empty `Metadata` map.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts or updates a key/value pair after enforcing Moyasar limits.
    pub fn insert(&mut self, key: &str, value: &str) -> Result<(), MoyasarError> {
        if self.0.len() >= METADATA_MAX_KEYS && !self.0.contains_key(key) {
            return Err(MoyasarError::MetadataTooManyKeys);
        }
        if key.chars().count() > METADATA_KEY_MAX_LEN {
            return Err(MoyasarError::MetadataKeyTooLong);
        }
        if value.chars().count() > METADATA_VALUE_MAX_LEN {
            return Err(MoyasarError::MetadataValueTooLong);
        }
        self.0.insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    /// Marks a key to be unset on update payloads by setting its value to `""`.
    pub fn unset(&mut self, key: &str) -> Result<(), MoyasarError> {
        self.insert(key, "")
    }

    /// Retrieves a metadata value by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Removes a key locally from the map.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.0.remove(key)
    }

    /// Returns the number of entries in the map.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_key_count_and_length_limits() {
        let mut meta = Metadata::new();
        for i in 0..30 {
            meta.insert(&format!("k_{i}"), "val").expect("within 30 keys");
        }
        assert!(matches!(
            meta.insert("k_30", "val"),
            Err(MoyasarError::MetadataTooManyKeys)
        ));

        let long_key = "a".repeat(41);
        assert!(matches!(
            Metadata::new().insert(&long_key, "v"),
            Err(MoyasarError::MetadataKeyTooLong)
        ));

        let long_val = "b".repeat(501);
        assert!(matches!(
            Metadata::new().insert("k", &long_val),
            Err(MoyasarError::MetadataValueTooLong)
        ));
    }

    #[test]
    fn empty_string_unset_serializes_cleanly() {
        let mut meta = Metadata::new();
        meta.unset("order_id").expect("unset valid");
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["order_id"], "");
    }

    #[test]
    fn deserialize_enforces_metadata_limits() {
        let valid: Metadata = serde_json::from_str(r#"{"order_id":"123"}"#).unwrap();
        assert_eq!(valid.get("order_id"), Some("123"));

        let mut map = BTreeMap::new();
        for i in 0..31 {
            map.insert(format!("k_{i}"), "val".to_owned());
        }
        let json_31 = serde_json::to_string(&map).unwrap();
        assert!(serde_json::from_str::<Metadata>(&json_31).is_err());

        let long_key_json = format!(r#"{{"{}":"v"}}"#, "a".repeat(41));
        assert!(serde_json::from_str::<Metadata>(&long_key_json).is_err());

        let long_val_json = format!(r#"{{"k":"{}"}}"#, "b".repeat(501));
        assert!(serde_json::from_str::<Metadata>(&long_val_json).is_err());
    }
}
