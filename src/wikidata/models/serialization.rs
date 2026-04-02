use super::{Entity, EntityCollection, property::Property};
use crate::wikidata::EntityValue;
use crate::{Error, wikidata::WikidataId};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;

/// Configuration for CSV serialization
pub struct CsvConfig {
    pub resolve_property_headers: Option<Vec<WikidataId>>,
    pub resolve_data_values: Option<Vec<WikidataId>>,
    pub keep_qids: bool,
    pub keep_filename: bool,
    pub language: String,
}

/// Configuration for denormalized CSV serialization
pub struct CsvDenormalizedConfig {
    pub resolve_property_headers: Option<Vec<WikidataId>>,
    pub resolve_data_values: Option<Vec<WikidataId>>,
    pub keep_qids: bool,
    pub language: String,
}

impl Default for CsvDenormalizedConfig {
    fn default() -> Self {
        Self {
            resolve_property_headers: None,
            resolve_data_values: None,
            keep_qids: false,
            language: "en".to_string(),
        }
    }
}

/// Configuration for JSON serialization
#[derive(Default, Clone)]
pub struct JsonConfig {
    pub only_values: bool,
    pub preserve_qualifiers_for: Vec<WikidataId>,
}

impl Default for CsvConfig {
    fn default() -> Self {
        Self {
            resolve_property_headers: None,
            resolve_data_values: None,
            keep_qids: false,
            keep_filename: false,
            language: "en".to_string(),
        }
    }
}

/// Simplified entity collection for --only-values JSON serialization
#[derive(Debug, Serialize)]
pub struct SimplifiedEntityCollection(
    #[serde(serialize_with = "serialize_simplified_entity_collection")]
    BTreeMap<WikidataId, SimplifiedEntity>,
);

impl SimplifiedEntityCollection {
    /// Create a SimplifiedEntityCollection from an EntityCollection with config
    pub fn from_collection(entities: &EntityCollection, config: &JsonConfig) -> Self {
        let map = entities
            .iter()
            .map(|(key, entity)| (key.clone(), SimplifiedEntity::from_entity(entity, config)))
            .collect();
        Self(map)
    }
}

/// Simplified entity for --only-values JSON serialization
#[derive(Debug, Serialize)]
pub struct SimplifiedEntity {
    pub id: WikidataId,
    pub entity_type: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    #[serde(serialize_with = "serialize_simplified_properties")]
    pub properties: BTreeMap<WikidataId, SimplifiedProperty>,
    pub sitelinks: HashMap<String, String>,
}

impl SimplifiedEntity {
    /// Create a SimplifiedEntity from an Entity with config
    pub fn from_entity(entity: &Entity, config: &JsonConfig) -> Self {
        let properties = entity
            .properties
            .iter()
            .map(|(key, prop)| {
                (
                    key.clone(),
                    SimplifiedProperty::from_property(prop, key, config),
                )
            })
            .collect();

        Self {
            id: entity.id.clone(),
            entity_type: entity.entity_type.clone(),
            label: entity.label.clone(),
            description: entity.description.clone(),
            aliases: entity.aliases.clone(),
            properties,
            sitelinks: entity.sitelinks.clone(),
        }
    }
}

/// Simplified property for --only-values JSON serialization
#[derive(Debug)]
pub struct SimplifiedProperty {
    pub statements: Vec<SimplifiedStatement>,
}

/// Simplified statement that may optionally include qualifiers
#[derive(Debug)]
pub struct SimplifiedStatement {
    pub value: EntityValue,
    pub qualifiers: Option<BTreeMap<String, Vec<EntityValue>>>,
}

impl Serialize for SimplifiedStatement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // When we have qualifiers, serialize as an object with unwrapped value and qualifiers
        if let Some(ref qualifiers) = self.qualifiers {
            let mut map = serializer.serialize_map(Some(2))?;

            // Serialize the value field with unwrapping (same logic as SimplifiedProperty)
            map.serialize_entry("value", &UnwrappedValue(&self.value))?;

            // Serialize qualifiers
            map.serialize_entry("qualifiers", &SerializedQualifiersMap(qualifiers))?;

            map.end()
        } else {
            // This shouldn't happen in SimplifiedProperty::serialize, but handle it anyway
            self.value.serialize(serializer)
        }
    }
}

/// Wrapper for unwrapping a single EntityValue during serialization
struct UnwrappedValue<'a>(&'a EntityValue);

impl<'a> Serialize for UnwrappedValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.0 {
            EntityValue::String(s) => s.serialize(serializer),
            EntityValue::MonolingualText { text, .. } => text.serialize(serializer),
            EntityValue::CommonsMediaPageId(pageid_info) => {
                if pageid_info.filename.is_none() {
                    pageid_info.pageid.serialize(serializer)
                } else {
                    pageid_info.serialize(serializer)
                }
            }
            _ => self.0.serialize(serializer),
        }
    }
}

/// Wrapper for serializing the qualifiers map
struct SerializedQualifiersMap<'a>(&'a BTreeMap<String, Vec<EntityValue>>);

impl<'a> Serialize for SerializedQualifiersMap<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, values) in self.0 {
            map.serialize_entry(key, &SerializedValueArray(values))?;
        }
        map.end()
    }
}

/// Wrapper for serializing an array of EntityValues with unwrapping
struct SerializedValueArray<'a>(&'a Vec<EntityValue>);

impl<'a> Serialize for SerializedValueArray<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            // Apply same unwrapping logic as SimplifiedProperty
            match value {
                EntityValue::String(s) => seq.serialize_element(s)?,
                EntityValue::MonolingualText { text, .. } => seq.serialize_element(text)?,
                EntityValue::CommonsMediaPageId(pageid_info) => {
                    if pageid_info.filename.is_none() {
                        seq.serialize_element(&pageid_info.pageid)?;
                    } else {
                        seq.serialize_element(pageid_info)?;
                    }
                }
                _ => seq.serialize_element(value)?,
            }
        }
        seq.end()
    }
}

impl SimplifiedProperty {
    /// Create a SimplifiedProperty from a Property
    /// If property_id is in preserve_qualifiers_for list, qualifiers will be included
    pub fn from_property(
        property: &Property,
        property_id: &WikidataId,
        config: &JsonConfig,
    ) -> Self {
        let preserve_qualifiers = config
            .preserve_qualifiers_for
            .iter()
            .any(|id| id == property_id);

        let statements = property
            .statements
            .iter()
            .map(|stmt| {
                let qualifiers = if preserve_qualifiers {
                    // Convert Vec<Qualifier> to BTreeMap<String, Vec<EntityValue>>
                    // Group by property label (use label if present, fallback to ID string)
                    // Always create the map even if empty to ensure consistent format
                    let mut qualifiers_map: BTreeMap<String, Vec<EntityValue>> = BTreeMap::new();
                    for qualifier in &stmt.qualifiers {
                        let key = qualifier.property.to_string();
                        qualifiers_map
                            .entry(key)
                            .or_default()
                            .push(qualifier.value.clone());
                    }
                    Some(qualifiers_map)
                } else {
                    None
                };

                SimplifiedStatement {
                    value: stmt.value.clone(),
                    qualifiers,
                }
            })
            .collect();

        Self { statements }
    }
}

impl Serialize for SimplifiedProperty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.statements.len()))?;
        for stmt in &self.statements {
            // If qualifiers are present, serialize as object with value and qualifiers
            if stmt.qualifiers.is_some() {
                seq.serialize_element(stmt)?;
            } else {
                // Otherwise, serialize just the value (with special handling for certain types)
                match &stmt.value {
                    EntityValue::String(s) => seq.serialize_element(s)?,
                    EntityValue::MonolingualText { text, .. } => seq.serialize_element(text)?,
                    EntityValue::CommonsMediaPageId(pageid_info) => {
                        // When filename is None, serialize as just the pageid (int)
                        // When filename is Some, serialize as object with both fields
                        if pageid_info.filename.is_none() {
                            seq.serialize_element(&pageid_info.pageid)?;
                        } else {
                            seq.serialize_element(pageid_info)?;
                        }
                    }
                    _ => seq.serialize_element(&stmt.value)?,
                }
            }
        }
        seq.end()
    }
}

/// Custom serialization for simplified properties BTreeMap to convert WikidataId keys to strings
fn serialize_simplified_properties<S>(
    properties: &BTreeMap<WikidataId, SimplifiedProperty>,
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

/// Custom serialization for SimplifiedEntityCollection to convert WikidataId keys to strings
fn serialize_simplified_entity_collection<S>(
    collection: &BTreeMap<WikidataId, SimplifiedEntity>,
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

/// Serialize entities to CSV format
pub async fn serialize_entities_to_csv<W: Write>(
    entities: &EntityCollection,
    writer: W,
    config: CsvConfig,
) -> Result<(), Error> {
    if entities.is_empty() {
        return Ok(());
    }

    // Collect all unique property IDs across all entities
    let mut all_property_ids: BTreeSet<WikidataId> = BTreeSet::new();
    for (_id, entity) in entities.iter() {
        all_property_ids.extend(entity.get_property_ids());
    }

    // Property and data value labels are now resolved via LabelStore before calling this function
    let property_labels: BTreeMap<WikidataId, String> = BTreeMap::new();

    // Determine which properties have resolvable values (for keep_qids)
    let mut properties_with_resolvable_ids: BTreeSet<WikidataId> = BTreeSet::new();
    if config.keep_qids && config.resolve_data_values.is_some() {
        for entity in entities.values() {
            for (property_id, property) in &entity.properties {
                if (config.resolve_data_values.as_ref().unwrap().is_empty()
                    || config
                        .resolve_data_values
                        .as_ref()
                        .unwrap()
                        .contains(property_id))
                    && let Some(statement) = property.get_primary_statement()
                    && statement.value.contains_resolvable_id()
                {
                    properties_with_resolvable_ids.insert(property_id.clone());
                }
            }
        }
    }

    // Determine which properties have CommonsMediaPageId values (for keep_filename)
    let mut properties_with_commons_media: BTreeSet<WikidataId> = BTreeSet::new();
    if config.keep_filename && config.resolve_data_values.is_some() {
        for entity in entities.values() {
            for (property_id, property) in &entity.properties {
                if (config.resolve_data_values.as_ref().unwrap().is_empty()
                    || config
                        .resolve_data_values
                        .as_ref()
                        .unwrap()
                        .contains(property_id))
                    && let Some(statement) = property.get_primary_statement()
                    && matches!(statement.value, EntityValue::CommonsMediaPageId(_))
                {
                    properties_with_commons_media.insert(property_id.clone());
                }
            }
        }
    }

    // Build CSV header
    let mut csv_writer = csv::Writer::from_writer(writer);
    let mut header = vec![
        "id".to_string(),
        "type".to_string(),
        "label".to_string(),
        "description".to_string(),
    ];

    // Add property columns
    for property_id in &all_property_ids {
        let column_name = property_labels
            .get(property_id)
            .cloned()
            .unwrap_or_else(|| property_id.to_string());

        let should_resolve_data = config
            .resolve_data_values
            .as_ref()
            .map(|ids| ids.is_empty() || ids.contains(property_id))
            .unwrap_or(false);

        let has_resolvable_ids = properties_with_resolvable_ids.contains(property_id);
        let has_commons_media = properties_with_commons_media.contains(property_id);

        if config.keep_qids && should_resolve_data && has_resolvable_ids {
            header.push(format!("{}-qid", column_name));
            header.push(column_name);
        } else if config.keep_filename && should_resolve_data && has_commons_media {
            header.push(format!("{}-filename", column_name));
            header.push(column_name);
        } else {
            header.push(column_name);
        }
    }

    csv_writer.write_record(&header)?;

    // Write entity data
    for entity in entities.values() {
        let mut record = vec![
            entity.id.to_string(),
            entity.entity_type.clone(),
            entity.label.as_deref().unwrap_or("").to_string(),
            entity.description.as_deref().unwrap_or("").to_string(),
        ];

        // Property values
        for property_id in &all_property_ids {
            let should_resolve_data = config
                .resolve_data_values
                .as_ref()
                .map(|ids| ids.is_empty() || ids.contains(property_id))
                .unwrap_or(false);

            let has_resolvable_ids = properties_with_resolvable_ids.contains(property_id);
            let has_commons_media = properties_with_commons_media.contains(property_id);

            let (primary_value, resolved_value) =
                if let Some(property) = entity.properties.get(property_id) {
                    if let Some(statement) = property.get_primary_statement() {
                        match &statement.value {
                            // WikidataItem: primary is QID, resolved is label
                            crate::wikidata::EntityValue::WikidataItem(id) => {
                                (id.id_string(), statement.value.display_value())
                            }
                            // CommonsMediaPageId: primary is filename, resolved is pageid
                            crate::wikidata::EntityValue::CommonsMediaPageId(pageid_info) => {
                                let primary = pageid_info.filename.clone().unwrap_or_default();
                                let resolved = pageid_info.pageid.to_string();
                                (primary, resolved)
                            }
                            // CommonsMedia (unresolved): show empty for resolved
                            crate::wikidata::EntityValue::CommonsMedia(_) => {
                                ("".to_string(), "".to_string())
                            }
                            // Everything else: same for both
                            _ => {
                                let val = statement.value.display_value();
                                (val.clone(), val)
                            }
                        }
                    } else {
                        ("".to_string(), "".to_string())
                    }
                } else {
                    ("".to_string(), "".to_string())
                };

            if (config.keep_qids && should_resolve_data && has_resolvable_ids)
                || (config.keep_filename && should_resolve_data && has_commons_media)
            {
                record.push(primary_value);
                record.push(resolved_value);
            } else {
                let value = if should_resolve_data {
                    resolved_value
                } else {
                    primary_value
                };
                record.push(value);
            }
        }

        csv_writer.write_record(&record)?;
    }

    csv_writer.flush()?;
    Ok(())
}

/// Serialize entities to denormalized CSV format
///
/// Each entity becomes a row with columns for id, type, label, description, and each property.
/// Properties with multiple values will only show the primary value (preferred or first normal).
/// This format is useful for data analysis in spreadsheet applications.
pub async fn serialize_entities_to_csv_denormalized<W: Write>(
    entities: &EntityCollection,
    writer: W,
    config: CsvDenormalizedConfig,
    client: &crate::wikidata::WikidataClient,
) -> Result<(), Error> {
    use crate::wikidata::GetEntitiesQuery;

    if entities.is_empty() {
        return Ok(());
    }

    // Collect all unique property IDs across all entities
    let mut all_property_ids: BTreeSet<WikidataId> = BTreeSet::new();
    for entity in entities.values() {
        all_property_ids.extend(entity.get_property_ids());
    }

    // Resolve property labels for headers if requested
    let mut property_labels: BTreeMap<WikidataId, String> = BTreeMap::new();
    if let Some(headers_to_resolve) = &config.resolve_property_headers {
        let properties_to_fetch: Vec<WikidataId> = if headers_to_resolve.is_empty() {
            all_property_ids.iter().cloned().collect()
        } else {
            headers_to_resolve
                .iter()
                .filter(|prop| all_property_ids.contains(prop))
                .cloned()
                .collect()
        };

        if !properties_to_fetch.is_empty() {
            let query = GetEntitiesQuery::builder()
                .ids(properties_to_fetch.clone())
                .languages(vec![config.language.clone()])
                .props(vec!["labels".to_string()])
                .build()
                .map_err(|e| Error::InvalidInput(format!("Failed to build query: {}", e)))?;

            let label_entities = client.get_entities(&query).await?;

            for (property_id, entity) in label_entities {
                if let Some(label) = entity.label {
                    property_labels.insert(property_id, label);
                }
            }
        }
    }

    // Determine which properties to resolve data values for
    let data_properties_to_resolve: Vec<WikidataId> =
        if let Some(data_props) = &config.resolve_data_values {
            if data_props.is_empty() {
                all_property_ids.iter().cloned().collect()
            } else {
                data_props
                    .iter()
                    .filter(|prop| all_property_ids.contains(prop))
                    .cloned()
                    .collect()
            }
        } else {
            Vec::new()
        };

    // Collect all WikidataItem values that need resolution
    let mut data_value_labels: BTreeMap<WikidataId, String> = BTreeMap::new();
    let mut properties_with_wikidata_ids: BTreeSet<WikidataId> = BTreeSet::new();

    if !data_properties_to_resolve.is_empty() {
        let mut values_to_resolve: BTreeSet<WikidataId> = BTreeSet::new();

        for entity in entities.values() {
            for property_id in &data_properties_to_resolve {
                if let Some(property) = entity.properties.get(property_id)
                    && let Some(statement) = property.get_primary_statement()
                    && let EntityValue::WikidataItem(ref id) = statement.value
                {
                    values_to_resolve.insert(id.clone());
                    properties_with_wikidata_ids.insert(property_id.clone());
                }
            }
        }

        if !values_to_resolve.is_empty() {
            let query = GetEntitiesQuery::builder()
                .ids(
                    values_to_resolve
                        .iter()
                        .cloned()
                        .collect::<Vec<WikidataId>>(),
                )
                .languages(vec![config.language.clone()])
                .props(vec!["labels".to_string()])
                .build()
                .map_err(|e| Error::InvalidInput(format!("Failed to build query: {}", e)))?;

            let label_entities = client.get_entities(&query).await?;

            for (value_id, entity) in label_entities {
                if let Some(label) = entity.label {
                    data_value_labels.insert(value_id, label);
                }
            }
        }
    }

    // Build CSV header
    let mut csv_writer = csv::Writer::from_writer(writer);
    let mut header = vec![
        "id".to_string(),
        "type".to_string(),
        format!("label_{}", config.language),
        format!("description_{}", config.language),
    ];

    for property_id in &all_property_ids {
        let column_name = property_labels
            .get(property_id)
            .cloned()
            .unwrap_or_else(|| property_id.to_string());

        let should_resolve_data = data_properties_to_resolve.contains(property_id);
        let has_wikidata_ids = properties_with_wikidata_ids.contains(property_id);

        if config.keep_qids && should_resolve_data && has_wikidata_ids {
            header.push(format!("{}-qid", column_name));
            header.push(column_name);
        } else {
            header.push(column_name);
        }
    }

    csv_writer.write_record(&header)?;

    // Write entity data
    for entity in entities.values() {
        let mut record = vec![
            entity.id.to_string(),
            entity.entity_type.clone(),
            entity.label.as_deref().unwrap_or("").to_string(),
            entity.description.as_deref().unwrap_or("").to_string(),
        ];

        for property_id in &all_property_ids {
            let should_resolve_data = data_properties_to_resolve.contains(property_id);
            let has_wikidata_ids = properties_with_wikidata_ids.contains(property_id);

            let (qid_value, resolved_value) =
                if let Some(property) = entity.properties.get(property_id) {
                    if let Some(statement) = property.get_primary_statement() {
                        match &statement.value {
                            EntityValue::WikidataItem(id) => {
                                let qid = id.id_string();
                                let resolved = data_value_labels
                                    .get(id)
                                    .cloned()
                                    .unwrap_or_else(|| id.to_string());
                                (qid, resolved)
                            }
                            _ => {
                                let value = statement.value.display_value();
                                (value.clone(), value)
                            }
                        }
                    } else {
                        ("".to_string(), "".to_string())
                    }
                } else {
                    ("".to_string(), "".to_string())
                };

            if config.keep_qids && should_resolve_data && has_wikidata_ids {
                record.push(qid_value);
                record.push(resolved_value);
            } else {
                let value = if should_resolve_data {
                    resolved_value
                } else {
                    qid_value
                };
                record.push(value);
            }
        }

        csv_writer.write_record(&record)?;
    }

    csv_writer.flush()?;
    Ok(())
}

/// Serialize entities to JSON format
pub fn serialize_entities_to_json<W: Write>(
    entities: &EntityCollection,
    writer: W,
    config: JsonConfig,
) -> Result<(), Error> {
    if config.only_values {
        // Use custom serialization that prunes non-value fields
        // Config is passed through to conditionally preserve qualifiers for specific properties
        let simplified = SimplifiedEntityCollection::from_collection(entities, &config);
        serde_json::to_writer_pretty(writer, &simplified)?;
    } else {
        serde_json::to_writer_pretty(writer, entities)?;
    }
    Ok(())
}

/// A flattened representation for CSV serialization
#[derive(Debug, Serialize)]
pub struct FlatEntity {
    pub id: String,
    pub entity_type: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub aliases: String, // Comma-separated
    #[serde(flatten)]
    pub properties: HashMap<String, String>, // property_id -> value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wikidata::WikidataId;

    fn create_test_entity_collection() -> EntityCollection {
        let mut entity = Entity::new(WikidataId::qid(42), "item".to_string());
        entity.label = Some("Test Entity".to_string());
        entity.description = Some("Test Description".to_string());

        // Add a property with a string value
        let mut property = Property::new();
        property.add_statement(super::super::property::Statement::new(EntityValue::String(
            "test value".to_string(),
        )));
        entity.properties.insert(WikidataId::pid(31), property);

        // Add a property with a WikidataItem value
        let mut property2 = Property::new();
        property2.add_statement(super::super::property::Statement::new(
            EntityValue::WikidataItem(WikidataId::qid(5)),
        ));
        entity.properties.insert(WikidataId::pid(279), property2);

        EntityCollection::from_vec(vec![entity])
    }

    #[tokio::test]
    async fn test_serialize_entities_to_csv_denormalized_basic() {
        let entities = create_test_entity_collection();
        let client = crate::wikidata::WikidataClient::new();

        let config = CsvDenormalizedConfig::default();
        let mut buffer = Vec::new();
        serialize_entities_to_csv_denormalized(&entities, &mut buffer, config, &client)
            .await
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("id,type,label_en,description_en"));
        assert!(output.contains("Q42"));
        assert!(output.contains("Test Entity"));
        assert!(output.contains("test value"));
    }

    #[tokio::test]
    async fn test_serialize_entities_to_csv_denormalized_with_wikidata_item_values() {
        let entities = create_test_entity_collection();
        let client = crate::wikidata::WikidataClient::new();

        let config = CsvDenormalizedConfig::default();
        let mut buffer = Vec::new();
        serialize_entities_to_csv_denormalized(&entities, &mut buffer, config, &client)
            .await
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("P279"));
        assert!(output.contains("Q5"));
    }

    #[tokio::test]
    async fn test_serialize_entities_to_csv_denormalized_empty() {
        let entities = EntityCollection::new();
        let client = crate::wikidata::WikidataClient::new();

        let config = CsvDenormalizedConfig::default();
        let mut buffer = Vec::new();
        let result =
            serialize_entities_to_csv_denormalized(&entities, &mut buffer, config, &client).await;

        assert!(result.is_ok());
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_serialize_entities_to_json_default() {
        let entities = create_test_entity_collection();

        let config = JsonConfig::default();
        let mut buffer = Vec::new();
        serialize_entities_to_json(&entities, &mut buffer, config).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("Q42"));
        assert!(output.contains("Test Entity"));
    }

    #[test]
    fn test_serialize_entities_to_json_only_values() {
        let entities = create_test_entity_collection();

        let config = JsonConfig {
            only_values: true,
            ..Default::default()
        };
        let mut buffer = Vec::new();
        serialize_entities_to_json(&entities, &mut buffer, config).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("Q42"));
    }

    #[test]
    fn test_serialize_entities_to_json_preserve_qualifiers() {
        // Create an entity with properties that have qualifiers
        let mut entity = Entity::new(WikidataId::qid(30), "item".to_string());
        entity.label = Some("United States".to_string());

        // P6375 (street address) - with qualifiers (should be preserved)
        let mut p6375_statement_with_qualifiers =
            super::super::property::Statement::new(EntityValue::MonolingualText {
                text: "1600 Pennsylvania Avenue".to_string(),
                language: "en".to_string(),
            });
        p6375_statement_with_qualifiers
            .qualifiers
            .push(super::super::property::Qualifier {
                property: WikidataId::pid(580).with_label(Some("start time".to_string())), // P580 (start time)
                value: EntityValue::String("2021-01-20".to_string()),
            });
        p6375_statement_with_qualifiers
            .qualifiers
            .push(super::super::property::Qualifier {
                property: WikidataId::pid(582).with_label(Some("end time".to_string())), // P582 (end time)
                value: EntityValue::String("2025-01-20".to_string()),
            });

        // P6375 statement WITHOUT qualifiers - should still use object format when property is in preserve list
        let p6375_statement_no_qualifiers =
            super::super::property::Statement::new(EntityValue::MonolingualText {
                text: "Another Address".to_string(),
                language: "en".to_string(),
            });

        let mut property_p6375 = Property::new();
        property_p6375.add_statement(p6375_statement_with_qualifiers);
        property_p6375.add_statement(p6375_statement_no_qualifiers);
        entity
            .properties
            .insert(WikidataId::pid(6375), property_p6375);

        // P31 (instance of) - with qualifiers (should NOT be preserved)
        let mut p31_statement = super::super::property::Statement::new(EntityValue::WikidataItem(
            WikidataId::qid(6256),
        ));
        p31_statement
            .qualifiers
            .push(super::super::property::Qualifier {
                property: WikidataId::pid(1810), // P1810 (subject named as)
                value: EntityValue::String("Country".to_string()),
            });
        let mut property_p31 = Property::new();
        property_p31.add_statement(p31_statement);
        entity.properties.insert(WikidataId::pid(31), property_p31);

        let entities = EntityCollection::from_vec(vec![entity]);

        // Configure to preserve qualifiers only for P6375
        let config = JsonConfig {
            only_values: true,
            preserve_qualifiers_for: vec![WikidataId::pid(6375)],
        };

        let mut buffer = Vec::new();
        serialize_entities_to_json(&entities, &mut buffer, config).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Parse JSON to verify structure
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        // Verify P6375 has 2 statements
        let p6375_statements = &json["Q30"]["properties"]["P6375"];
        assert!(p6375_statements.is_array(), "P6375 should be an array");
        assert_eq!(
            p6375_statements.as_array().unwrap().len(),
            2,
            "P6375 should have 2 statements"
        );

        // Test first statement (with qualifiers)
        let p6375_first = &p6375_statements[0];
        assert!(
            p6375_first.is_object(),
            "P6375 first statement should be an object"
        );
        assert!(
            p6375_first["qualifiers"].is_object(),
            "P6375 first statement qualifiers should be an object"
        );

        let qualifiers = p6375_first["qualifiers"].as_object().unwrap();
        assert_eq!(
            qualifiers.len(),
            2,
            "P6375 first statement should have 2 qualifier properties"
        );

        // Verify the qualifier structure - keys are property labels, values are arrays
        assert!(
            qualifiers.contains_key("start time"),
            "Qualifiers should have 'start time' key"
        );
        assert!(
            qualifiers.contains_key("end time"),
            "Qualifiers should have 'end time' key"
        );

        // Verify the values are arrays with unwrapped strings
        let start_time_values = qualifiers["start time"].as_array().unwrap();
        assert_eq!(start_time_values.len(), 1, "start time should have 1 value");
        assert_eq!(
            start_time_values[0].as_str().unwrap(),
            "2021-01-20",
            "start time value should be unwrapped string"
        );

        let end_time_values = qualifiers["end time"].as_array().unwrap();
        assert_eq!(end_time_values.len(), 1, "end time should have 1 value");
        assert_eq!(
            end_time_values[0].as_str().unwrap(),
            "2025-01-20",
            "end time value should be unwrapped string"
        );

        // Test second statement (WITHOUT qualifiers) - should still be object format with empty qualifiers
        let p6375_second = &p6375_statements[1];
        assert!(
            p6375_second.is_object(),
            "P6375 second statement should be an object even without qualifiers"
        );
        assert!(
            p6375_second["value"].is_string(),
            "P6375 second statement should have a value field"
        );
        assert_eq!(
            p6375_second["value"].as_str().unwrap(),
            "Another Address",
            "P6375 second statement value should be 'Another Address'"
        );
        assert!(
            p6375_second["qualifiers"].is_object(),
            "P6375 second statement should have qualifiers field as empty object"
        );
        let empty_qualifiers = p6375_second["qualifiers"].as_object().unwrap();
        assert_eq!(
            empty_qualifiers.len(),
            0,
            "P6375 second statement qualifiers should be empty"
        );

        // Verify P31 does NOT have qualifiers (should be just the value)
        let p31_statements = &json["Q30"]["properties"]["P31"];
        assert!(p31_statements.is_array(), "P31 should be an array");
        let p31_first = &p31_statements[0];
        // When qualifiers are not preserved, the statement should be just the value
        // For WikidataItem, it should be an object with id/label, not have a "qualifiers" field
        assert!(
            !p31_first.as_object().unwrap().contains_key("qualifiers"),
            "P31 should not have qualifiers field when not in preserve list"
        );
    }
}
