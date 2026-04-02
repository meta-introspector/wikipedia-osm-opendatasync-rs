use super::property::Property;
use crate::wikidata::WikidataId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};

/// Collection of entities indexed by their WikidataId
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCollection(
    #[serde(
        serialize_with = "serialize_entity_collection",
        deserialize_with = "deserialize_entity_collection"
    )]
    pub BTreeMap<WikidataId, Entity>,
);

impl EntityCollection {
    /// Create a new empty collection
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Create from a vector of entities
    ///
    /// Note: Prefer using the `From<Vec<Entity>>` trait implementation instead:
    /// ```rust,ignore
    /// let collection: EntityCollection = entities.into();
    /// ```
    pub fn from_vec(entities: Vec<Entity>) -> Self {
        entities.into()
    }

    /// Get an iterator over entity references
    pub fn values(&self) -> std::collections::btree_map::Values<'_, WikidataId, Entity> {
        self.0.values()
    }

    /// Get a mutable iterator over entity references
    pub fn values_mut(&mut self) -> std::collections::btree_map::ValuesMut<'_, WikidataId, Entity> {
        self.0.values_mut()
    }
}

impl Default for EntityCollection {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for EntityCollection {
    type Target = BTreeMap<WikidataId, Entity>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EntityCollection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for EntityCollection {
    type Item = (WikidataId, Entity);
    type IntoIter = std::collections::btree_map::IntoIter<WikidataId, Entity>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a EntityCollection {
    type Item = (&'a WikidataId, &'a Entity);
    type IntoIter = std::collections::btree_map::Iter<'a, WikidataId, Entity>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut EntityCollection {
    type Item = (&'a WikidataId, &'a mut Entity);
    type IntoIter = std::collections::btree_map::IterMut<'a, WikidataId, Entity>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl From<Vec<Entity>> for EntityCollection {
    fn from(entities: Vec<Entity>) -> Self {
        let map = entities
            .into_iter()
            .map(|entity| (entity.id.clone(), entity))
            .collect();
        Self(map)
    }
}

impl From<BTreeMap<WikidataId, Entity>> for EntityCollection {
    fn from(map: BTreeMap<WikidataId, Entity>) -> Self {
        Self(map)
    }
}

/// Strongly typed intermediary representation of a Wikidata entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: WikidataId,
    pub entity_type: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    #[serde(
        serialize_with = "serialize_properties",
        deserialize_with = "deserialize_properties"
    )]
    pub properties: BTreeMap<WikidataId, Property>,
    pub sitelinks: HashMap<String, String>, // site -> title
}

impl Entity {
    /// Create a new Entity with minimal required fields
    pub fn new(id: WikidataId, entity_type: String) -> Self {
        Self {
            id,
            entity_type,
            label: None,
            description: None,
            aliases: Vec::new(),
            properties: BTreeMap::new(),
            sitelinks: HashMap::new(),
        }
    }

    /// Get all property IDs used in this entity
    pub fn get_property_ids(&self) -> BTreeSet<WikidataId> {
        self.properties.keys().cloned().collect()
    }

    /// Get all WikidataIds referenced in the property values
    pub fn get_referenced_ids(&self) -> BTreeSet<WikidataId> {
        let mut ids = BTreeSet::new();
        for property in self.properties.values() {
            property.collect_referenced_ids(&mut ids);
        }
        ids
    }

    /// Apply resolved labels from a label map to this entity
    pub fn apply_resolved_labels_from_map(&mut self, label_map: &BTreeMap<String, WikidataId>) {
        // Update property IDs in the properties BTreeMap
        let mut updated_properties = BTreeMap::new();

        for (property_id, property) in std::mem::take(&mut self.properties) {
            let id_str = property_id.to_string();
            let updated_id = if let Some(resolved_id) = label_map.get(&id_str) {
                resolved_id.clone()
            } else {
                property_id
            };

            updated_properties.insert(updated_id, property);
        }

        self.properties = updated_properties;

        // Update WikidataIds in property values
        for property in self.properties.values_mut() {
            property.apply_resolved_labels_from_map(label_map);
        }
    }

    /// Check if this entity should be cached
    ///
    /// Returns true if:
    /// - It's a Property (P-ID) - always cache properties since labels are all we need
    /// - It's an Item (Q-ID) with properties - only cache complete items
    ///
    /// Returns false if:
    /// - It's an Item (Q-ID) without properties - partial fetch, don't cache
    pub fn should_cache(&self) -> bool {
        if self.id.is_property() {
            // Properties (P-IDs) are metadata - labels-only is complete, always cache
            true
        } else {
            // Items (Q-IDs) should only be cached if they have properties
            // Empty properties means it was a labels-only fetch, which is partial
            !self.properties.is_empty()
        }
    }
}

/// Custom serialization for properties BTreeMap to convert WikidataId keys to strings
fn serialize_properties<S>(
    properties: &BTreeMap<WikidataId, Property>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(properties.len()))?;
    for (key, value) in properties {
        map.serialize_entry(&key.to_string(), value)?;
    }
    map.end()
}

/// Custom deserialization for properties BTreeMap to convert strings back to WikidataId keys
fn deserialize_properties<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<WikidataId, Property>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Deserialize, Error};
    let string_map: HashMap<String, Property> = HashMap::deserialize(deserializer)?;
    let mut properties = BTreeMap::new();

    for (key_str, value) in string_map {
        let key = WikidataId::try_from(key_str.as_str())
            .map_err(|e| D::Error::custom(format!("Invalid WikidataId: {}", e)))?;
        properties.insert(key, value);
    }

    Ok(properties)
}

/// Custom serialization for EntityCollection to convert WikidataId keys to strings
fn serialize_entity_collection<S>(
    collection: &BTreeMap<WikidataId, Entity>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(collection.len()))?;
    for (key, value) in collection {
        map.serialize_entry(&key.to_string(), value)?;
    }
    map.end()
}

/// Custom deserialization for EntityCollection to convert strings back to WikidataId keys
fn deserialize_entity_collection<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<WikidataId, Entity>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Deserialize, Error};
    let string_map: HashMap<String, Entity> = HashMap::deserialize(deserializer)?;
    let mut collection = BTreeMap::new();

    for (key_str, value) in string_map {
        let key = WikidataId::try_from(key_str.as_str())
            .map_err(|e| D::Error::custom(format!("Invalid WikidataId: {}", e)))?;
        collection.insert(key, value);
    }

    Ok(collection)
}

/// Implement Cacheable trait for Entity
impl crate::Cacheable for Entity {
    fn global_id(&self) -> crate::GlobalId {
        crate::GlobalId::Wikidata(self.id.clone())
    }

    fn cache_key(&self) -> String {
        self.id.to_string()
    }

    fn should_cache(&self) -> bool {
        // Use the Entity's should_cache method
        self.should_cache()
    }
}
