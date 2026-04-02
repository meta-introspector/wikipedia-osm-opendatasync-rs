use crate::{CacheKey, Cacheable, Error, GlobalId};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// Represents an OpenStreetMap element ID with its type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OSMId {
    Node(u64),
    Way(u64),
    Relation(u64),
}

impl OSMId {
    /// Get the numeric ID regardless of type
    pub fn numeric_id(&self) -> u64 {
        match self {
            OSMId::Node(id) | OSMId::Way(id) | OSMId::Relation(id) => *id,
        }
    }

    /// Check if this is a node
    pub fn is_node(&self) -> bool {
        matches!(self, OSMId::Node(_))
    }

    /// Check if this is a way
    pub fn is_way(&self) -> bool {
        matches!(self, OSMId::Way(_))
    }

    /// Check if this is a relation
    pub fn is_relation(&self) -> bool {
        matches!(self, OSMId::Relation(_))
    }

    /// Get the type as a string
    pub fn type_string(&self) -> &'static str {
        match self {
            OSMId::Node(_) => "node",
            OSMId::Way(_) => "way",
            OSMId::Relation(_) => "relation",
        }
    }
}

/// Display format for Overpass QL queries: node(123), way(456), relation(789)
impl Display for OSMId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OSMId::Node(id) => write!(f, "node({})", id),
            OSMId::Way(id) => write!(f, "way({})", id),
            OSMId::Relation(id) => write!(f, "relation({})", id),
        }
    }
}

impl Cacheable for OSMId {
    fn global_id(&self) -> GlobalId {
        GlobalId::OSM(*self)
    }

    fn cache_key(&self) -> String {
        match self {
            OSMId::Node(id) => format!("node/{}", id),
            OSMId::Way(id) => format!("way/{}", id),
            OSMId::Relation(id) => format!("relation/{}", id),
        }
    }
}

impl CacheKey for OSMId {
    fn to_key(&self) -> String {
        self.cache_key()
    }
}

impl TryFrom<&str> for OSMId {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Support multiple formats:
        // 1. "node/123", "way/456", "relation/789"
        // 2. "n123", "w456", "r789"
        // 3. "node(123)", "way(456)", "relation(789)" (Overpass QL format)

        let value = value.trim();

        // Try slash-separated format first: "node/123"
        if let Some((type_str, id_str)) = value.split_once('/') {
            let id = id_str
                .parse::<u64>()
                .map_err(|_| Error::InvalidOSMId(value.to_string()))?;

            return match type_str {
                "node" | "n" => Ok(OSMId::Node(id)),
                "way" | "w" => Ok(OSMId::Way(id)),
                "relation" | "rel" | "r" => Ok(OSMId::Relation(id)),
                _ => Err(Error::InvalidOSMId(value.to_string())),
            };
        }

        // Try Overpass QL format: "node(123)"
        if let Some(paren_pos) = value.find('(')
            && value.ends_with(')')
        {
            let type_str = &value[..paren_pos];
            let id_str = &value[paren_pos + 1..value.len() - 1];
            let id = id_str
                .parse::<u64>()
                .map_err(|_| Error::InvalidOSMId(value.to_string()))?;

            return match type_str {
                "node" => Ok(OSMId::Node(id)),
                "way" => Ok(OSMId::Way(id)),
                "relation" => Ok(OSMId::Relation(id)),
                _ => Err(Error::InvalidOSMId(value.to_string())),
            };
        }

        // Try compact format: "n123", "w456", "r789"
        if value.len() > 1 {
            let prefix = &value[..1];
            let id_str = &value[1..];

            if let Ok(id) = id_str.parse::<u64>() {
                return match prefix {
                    "n" => Ok(OSMId::Node(id)),
                    "w" => Ok(OSMId::Way(id)),
                    "r" => Ok(OSMId::Relation(id)),
                    _ => Err(Error::InvalidOSMId(value.to_string())),
                };
            }
        }

        Err(Error::InvalidOSMId(value.to_string()))
    }
}

impl TryFrom<String> for OSMId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        OSMId::try_from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = OSMId::Node(123);
        assert_eq!(node.numeric_id(), 123);
        assert!(node.is_node());
        assert_eq!(node.to_string(), "node(123)");
        assert_eq!(node.type_string(), "node");
    }

    #[test]
    fn test_way_creation() {
        let way = OSMId::Way(456);
        assert_eq!(way.numeric_id(), 456);
        assert!(way.is_way());
        assert_eq!(way.to_string(), "way(456)");
        assert_eq!(way.type_string(), "way");
    }

    #[test]
    fn test_relation_creation() {
        let rel = OSMId::Relation(789);
        assert_eq!(rel.numeric_id(), 789);
        assert!(rel.is_relation());
        assert_eq!(rel.to_string(), "relation(789)");
        assert_eq!(rel.type_string(), "relation");
    }

    #[test]
    fn test_try_from_slash_format() {
        // Standard slash format
        assert_eq!(OSMId::try_from("node/123").unwrap(), OSMId::Node(123));
        assert_eq!(OSMId::try_from("way/456").unwrap(), OSMId::Way(456));
        assert_eq!(
            OSMId::try_from("relation/789").unwrap(),
            OSMId::Relation(789)
        );

        // Short slash format
        assert_eq!(OSMId::try_from("n/123").unwrap(), OSMId::Node(123));
        assert_eq!(OSMId::try_from("w/456").unwrap(), OSMId::Way(456));
        assert_eq!(OSMId::try_from("r/789").unwrap(), OSMId::Relation(789));
        assert_eq!(OSMId::try_from("rel/789").unwrap(), OSMId::Relation(789));
    }

    #[test]
    fn test_try_from_compact_format() {
        // Compact format
        assert_eq!(OSMId::try_from("n123").unwrap(), OSMId::Node(123));
        assert_eq!(OSMId::try_from("w456").unwrap(), OSMId::Way(456));
        assert_eq!(OSMId::try_from("r789").unwrap(), OSMId::Relation(789));
    }

    #[test]
    fn test_try_from_overpass_format() {
        // Overpass QL format
        assert_eq!(OSMId::try_from("node(123)").unwrap(), OSMId::Node(123));
        assert_eq!(OSMId::try_from("way(456)").unwrap(), OSMId::Way(456));
        assert_eq!(
            OSMId::try_from("relation(789)").unwrap(),
            OSMId::Relation(789)
        );
    }

    #[test]
    fn test_try_from_whitespace_trimming() {
        assert_eq!(OSMId::try_from("  node/123  ").unwrap(), OSMId::Node(123));
        assert_eq!(OSMId::try_from("\tway/456\n").unwrap(), OSMId::Way(456));
    }

    #[test]
    fn test_try_from_invalid_formats() {
        assert!(OSMId::try_from("").is_err());
        assert!(OSMId::try_from("invalid").is_err());
        assert!(OSMId::try_from("node").is_err());
        assert!(OSMId::try_from("node/").is_err());
        assert!(OSMId::try_from("node/abc").is_err());
        assert!(OSMId::try_from("xyz/123").is_err());
        assert!(OSMId::try_from("node(").is_err());
        assert!(OSMId::try_from("node(123").is_err());
        assert!(OSMId::try_from("node123)").is_err());
        assert!(OSMId::try_from("x123").is_err());
    }

    #[test]
    fn test_try_from_string() {
        let s = String::from("node/123");
        assert_eq!(OSMId::try_from(s).unwrap(), OSMId::Node(123));
    }
}
