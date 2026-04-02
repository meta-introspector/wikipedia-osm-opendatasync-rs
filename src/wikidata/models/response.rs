use crate::wikidata::WikidataId;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt::{self, Display};

#[derive(Debug, Deserialize, Serialize)]
pub struct GetEntitiesResponse {
    pub success: Option<i32>,
    pub entities: Option<HashMap<String, Entity>>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetClaimsResponse {
    pub claims: Option<HashMap<String, Vec<Claim>>>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Entity {
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub labels: Option<HashMap<String, Label>>,
    pub descriptions: Option<HashMap<String, Description>>,
    pub aliases: Option<HashMap<String, Vec<Alias>>>,
    pub claims: Option<HashMap<String, Vec<Claim>>>,
    pub sitelinks: Option<HashMap<String, Sitelink>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Label {
    pub language: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Description {
    pub language: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Alias {
    pub language: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Claim {
    pub id: Option<String>,
    pub mainsnak: Option<Snak>,
    pub qualifiers: Option<HashMap<String, Vec<Snak>>>,
    pub references: Option<Vec<Reference>>,
    pub rank: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Snak {
    pub snaktype: Option<String>,
    pub property: Option<String>,
    pub datavalue: Option<DataValue>,
    pub datatype: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DataValue {
    pub value: Option<DataValueType>,
    pub r#type: Option<String>,
}

#[derive(Debug, Serialize)]
pub enum DataValueType {
    WikibaseItem(WikidataId),
    String(String),
    MonolingualText {
        language: String,
        text: String,
    },
    GlobeCoordinate {
        latitude: f64,
        longitude: f64,
        precision: Option<f64>,
        globe: Option<String>,
    },
    Time {
        time: String,
        precision: Option<u8>,
        calendarmodel: Option<String>,
    },
    Quantity {
        amount: String,
        unit: Option<String>,
    },
    Raw(serde_json::Value), // Fallback for unknown types
}

impl<'de> Deserialize<'de> for DataValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawDataValue {
            value: Option<serde_json::Value>,
            r#type: Option<String>,
        }

        let raw = RawDataValue::deserialize(deserializer)?;
        let value = match (&raw.r#type, &raw.value) {
            (Some(data_type), Some(json_value)) => {
                match data_type.as_str() {
                    "wikibase-entityid" => {
                        // Parse wikibase-item/property - handle multiple JSON formats
                        let id_str =
                            if let Some(id_str) = json_value.get("id").and_then(|v| v.as_str()) {
                                // Old format: {"id": "Q123"}
                                Some(id_str.to_string())
                            } else if let Some(entity_type) =
                                json_value.get("entity-type").and_then(|v| v.as_str())
                            {
                                // Format: {"entity-type": "item", "numeric-id": 123}
                                if let Some(numeric_id) =
                                    json_value.get("numeric-id").and_then(|v| v.as_u64())
                                {
                                    match entity_type {
                                        "item" => Some(format!("Q{}", numeric_id)),
                                        "property" => Some(format!("P{}", numeric_id)),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            } else if let Some(wikibase_item) = json_value.get("WikibaseItem") {
                                // Format: {"WikibaseItem": {"Q": 30}}
                                wikibase_item
                                    .get("Q")
                                    .and_then(|v| v.as_u64())
                                    .map(|q_value| format!("Q{}", q_value))
                            } else if let Some(wikibase_prop) = json_value.get("WikibaseProperty") {
                                // Format: {"WikibaseProperty": {"P": 123}}
                                wikibase_prop
                                    .get("P")
                                    .and_then(|v| v.as_u64())
                                    .map(|p_value| format!("P{}", p_value))
                            } else {
                                None
                            };

                        if let Some(id_str) = id_str {
                            match WikidataId::try_from(id_str.as_str()) {
                                Ok(wikidata_id) => Some(DataValueType::WikibaseItem(wikidata_id)),
                                Err(_) => Some(DataValueType::Raw(json_value.clone())),
                            }
                        } else {
                            Some(DataValueType::Raw(json_value.clone()))
                        }
                    }
                    "string" => {
                        if let Some(s) = json_value.as_str() {
                            Some(DataValueType::String(s.to_string()))
                        } else {
                            Some(DataValueType::Raw(json_value.clone()))
                        }
                    }
                    "monolingualtext" => {
                        let language = json_value
                            .get("language")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let text = json_value
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(DataValueType::MonolingualText { language, text })
                    }
                    "globecoordinate" => {
                        let latitude = json_value
                            .get("latitude")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let longitude = json_value
                            .get("longitude")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let precision = json_value.get("precision").and_then(|v| v.as_f64());
                        let globe = json_value
                            .get("globe")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Some(DataValueType::GlobeCoordinate {
                            latitude,
                            longitude,
                            precision,
                            globe,
                        })
                    }
                    "time" => {
                        let time = json_value
                            .get("time")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let precision = json_value
                            .get("precision")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u8);
                        let calendarmodel = json_value
                            .get("calendarmodel")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Some(DataValueType::Time {
                            time,
                            precision,
                            calendarmodel,
                        })
                    }
                    "quantity" => {
                        let amount = json_value
                            .get("amount")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let unit = json_value
                            .get("unit")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        Some(DataValueType::Quantity { amount, unit })
                    }
                    _ => Some(DataValueType::Raw(json_value.clone())),
                }
            }
            (_, Some(json_value)) => Some(DataValueType::Raw(json_value.clone())),
            _ => None,
        };

        Ok(DataValue {
            value,
            r#type: raw.r#type,
        })
    }
}

impl Display for DataValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValueType::WikibaseItem(id) => write!(f, "{}", id),
            DataValueType::String(s) => write!(f, "{}", s),
            DataValueType::MonolingualText { text, .. } => write!(f, "{}", text),
            DataValueType::GlobeCoordinate {
                latitude,
                longitude,
                ..
            } => write!(f, "{},{}", latitude, longitude),
            DataValueType::Time { time, .. } => write!(f, "{}", time),
            DataValueType::Quantity { amount, .. } => write!(f, "{}", amount),
            DataValueType::Raw(json) => write!(f, "{}", json),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Reference {
    pub snaks: Option<HashMap<String, Vec<Snak>>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Sitelink {
    pub site: String,
    pub title: String,
    pub badges: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiError {
    pub code: String,
    pub info: String,
}
