use crate::{CacheKey, Cacheable, Error, GlobalId};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum IdType {
    Item,
    Property,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikidataId {
    id_type: IdType,
    id: u64,
    pub label: Option<String>,
}

// Custom Hash implementation that ignores the label field
impl Hash for WikidataId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id_type.hash(state);
        self.id.hash(state);
    }
}

// Custom PartialEq implementation that ignores the label field
impl PartialEq for WikidataId {
    fn eq(&self, other: &Self) -> bool {
        self.id_type == other.id_type && self.id == other.id
    }
}

impl Eq for WikidataId {}

// Custom PartialOrd and Ord implementations that ignore the label field
impl PartialOrd for WikidataId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WikidataId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.id_type.cmp(&other.id_type) {
            std::cmp::Ordering::Equal => self.id.cmp(&other.id),
            other => other,
        }
    }
}

impl WikidataId {
    pub fn qid(id: u64) -> Self {
        Self {
            id_type: IdType::Item,
            id,
            label: None,
        }
    }

    pub fn pid(id: u64) -> Self {
        Self {
            id_type: IdType::Property,
            id,
            label: None,
        }
    }

    pub fn with_label(mut self, label: Option<String>) -> Self {
        self.label = label;
        self
    }

    pub fn numeric_id(&self) -> u64 {
        self.id
    }

    pub fn is_item(&self) -> bool {
        self.id_type == IdType::Item
    }

    pub fn is_property(&self) -> bool {
        self.id_type == IdType::Property
    }

    pub fn id_string(&self) -> String {
        match self.id_type {
            IdType::Item => format!("Q{}", self.id),
            IdType::Property => format!("P{}", self.id),
        }
    }
}

impl TryFrom<&str> for WikidataId {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if let Some(id_str) = value.strip_prefix('Q') {
            let id = id_str
                .parse::<u64>()
                .map_err(|_| Error::InvalidId(value.to_string()))?;
            Ok(WikidataId::qid(id))
        } else if let Some(id_str) = value.strip_prefix('P') {
            let id = id_str
                .parse::<u64>()
                .map_err(|_| Error::InvalidId(value.to_string()))?;
            Ok(WikidataId::pid(id))
        } else {
            Err(Error::InvalidId(value.to_string()))
        }
    }
}

impl TryFrom<String> for WikidataId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        WikidataId::try_from(value.as_str())
    }
}

impl Display for WikidataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref label) = self.label {
            write!(f, "{}", label)
        } else {
            write!(f, "{}", self.id_string())
        }
    }
}

impl Cacheable for WikidataId {
    fn global_id(&self) -> GlobalId {
        GlobalId::Wikidata(self.clone())
    }

    fn cache_key(&self) -> String {
        self.id_string()
    }
}

impl CacheKey for WikidataId {
    fn to_key(&self) -> String {
        self.id_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qid_creation() {
        let qid = WikidataId::qid(123);
        assert_eq!(qid.numeric_id(), 123);
        assert!(qid.is_item());
        assert_eq!(qid.to_string(), "Q123");
    }

    #[test]
    fn test_pid_creation() {
        let pid = WikidataId::pid(456);
        assert_eq!(pid.numeric_id(), 456);
        assert!(pid.is_property());
        assert_eq!(pid.to_string(), "P456");
    }

    #[test]
    fn test_with_label() {
        let qid = WikidataId::qid(42).with_label(Some("Douglas Adams".to_string()));
        assert_eq!(qid.to_string(), "Douglas Adams");
        assert_eq!(qid.numeric_id(), 42);
    }

    #[test]
    fn test_try_from_str() {
        let q123 = WikidataId::try_from("Q123").unwrap();
        assert_eq!(q123.numeric_id(), 123);
        assert!(q123.is_item());

        let p456 = WikidataId::try_from("P456").unwrap();
        assert_eq!(p456.numeric_id(), 456);
        assert!(p456.is_property());

        assert!(WikidataId::try_from("invalid").is_err());
        assert!(WikidataId::try_from("Q").is_err());
        assert!(WikidataId::try_from("Qabc").is_err());
    }
}
