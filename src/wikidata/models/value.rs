use crate::wikidata::WikidataId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{self, Display};

/// Represents different types of values that can be stored in Wikidata properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityValue {
    WikidataItem(WikidataId),
    String(String),
    MonolingualText {
        language: String,
        text: String,
    },
    GlobeCoordinate {
        latitude: f64,
        longitude: f64,
        precision: Option<f64>,
    },
    Time {
        time: String,
        precision: Option<u8>,
    },
    Quantity {
        amount: String,
        unit: Option<WikidataId>,
    },
    CommonsMedia(String),
    CommonsMediaPageId(CommonsMediaPageId),
    ExternalId(String),
    Url(String),
    NoValue,
    SomeValue,
    Unknown(String), // For unrecognized value types
}

/// Represents a Commons media file with enriched pageid information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonsMediaPageId {
    pub pageid: u64,
    pub filename: Option<String>,
}

impl EntityValue {
    /// Get the display string for this value
    pub fn display_value(&self) -> String {
        match self {
            EntityValue::WikidataItem(id) => id.to_string(),
            EntityValue::String(s) => s.clone(),
            EntityValue::MonolingualText { text, .. } => text.clone(),
            EntityValue::GlobeCoordinate {
                latitude,
                longitude,
                ..
            } => {
                format!("{},{}", latitude, longitude)
            }
            EntityValue::Time { time, .. } => time.clone(),
            EntityValue::Quantity { amount, .. } => amount.clone(),
            EntityValue::CommonsMedia(filename) => filename.clone(),
            EntityValue::CommonsMediaPageId(pageid_info) => pageid_info.pageid.to_string(),
            EntityValue::ExternalId(id) => id.clone(),
            EntityValue::Url(url) => url.clone(),
            EntityValue::NoValue => "no value".to_string(),
            EntityValue::SomeValue => "unknown value".to_string(),
            EntityValue::Unknown(s) => s.clone(),
        }
    }

    /// Get the resolved display value using a label map for WikidataItems
    pub fn display_value_resolved(
        &self,
        labels: &std::collections::HashMap<WikidataId, String>,
    ) -> String {
        match self {
            EntityValue::WikidataItem(id) => {
                labels.get(id).cloned().unwrap_or_else(|| id.to_string())
            }
            EntityValue::Quantity { amount, unit } => {
                if let Some(unit_id) = unit {
                    let unit_label = labels
                        .get(unit_id)
                        .cloned()
                        .unwrap_or_else(|| unit_id.to_string());
                    format!("{} {}", amount, unit_label)
                } else {
                    amount.clone()
                }
            }
            _ => self.display_value(),
        }
    }

    /// Collect all WikidataIds referenced in this value
    pub fn collect_referenced_ids(&self, ids: &mut BTreeSet<WikidataId>) {
        match self {
            EntityValue::WikidataItem(id) => {
                ids.insert(id.clone());
            }
            EntityValue::Quantity {
                unit: Some(unit_id),
                ..
            } => {
                ids.insert(unit_id.clone());
            }
            _ => {} // Other types don't reference WikidataIds
        }
    }

    /// Check if this value contains a WikidataId that could be resolved
    pub fn contains_resolvable_id(&self) -> bool {
        matches!(
            self,
            EntityValue::WikidataItem(_) | EntityValue::Quantity { unit: Some(_), .. }
        )
    }

    /// Apply resolved labels from a label map to WikidataIds in this value
    pub fn apply_resolved_labels_from_map(
        &mut self,
        label_map: &std::collections::BTreeMap<String, crate::wikidata::WikidataId>,
    ) {
        match self {
            EntityValue::WikidataItem(id) => {
                let id_str = id.to_string();
                if let Some(resolved_id) = label_map.get(&id_str) {
                    *id = resolved_id.clone();
                }
            }
            EntityValue::Quantity {
                unit: Some(unit_id),
                ..
            } => {
                let id_str = unit_id.to_string();
                if let Some(resolved_id) = label_map.get(&id_str) {
                    *unit_id = resolved_id.clone();
                }
            }
            _ => {} // Other types don't contain WikidataIds
        }
    }
}

impl Display for EntityValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_value())
    }
}

/// Represents the rank/priority of a statement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StatementRank {
    Preferred,
    #[default]
    Normal,
    Deprecated,
}

impl Display for StatementRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementRank::Preferred => write!(f, "preferred"),
            StatementRank::Normal => write!(f, "normal"),
            StatementRank::Deprecated => write!(f, "deprecated"),
        }
    }
}

/// Data value type for backwards compatibility
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataValue {
    pub value: Option<EntityValue>,
    pub r#type: Option<String>,
}

impl DataValue {
    pub fn new(value: EntityValue, data_type: String) -> Self {
        Self {
            value: Some(value),
            r#type: Some(data_type),
        }
    }
}
