use crate::overpass::OSMId;
use crate::wikidata::WikidataId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

/// Coordinate point with latitude and longitude (used in geometry arrays)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinate {
    pub lat: f64,
    pub lon: f64,
}

/// Trait for OSM elements that can have tags
pub trait HasTags {
    fn tags(&self) -> Option<&HashMap<String, String>>;

    /// Extract WikidataId from the "wikidata" tag if present
    fn wikidata_id(&self) -> Option<WikidataId> {
        self.tags()
            .and_then(|tags| tags.get("wikidata"))
            .and_then(|id_str| WikidataId::try_from(id_str.as_str()).ok())
    }
}

/// Overpass API response structure
#[derive(Debug, Clone, Serialize)]
pub struct OverpassResponse {
    pub version: Option<f64>,
    pub generator: Option<String>,
    pub osm3s: Option<Osm3s>,
    #[serde(serialize_with = "serialize_elements")]
    pub elements: BTreeMap<OSMId, Element>,
}

impl<'de> Deserialize<'de> for OverpassResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OverpassResponseHelper {
            version: Option<f64>,
            generator: Option<String>,
            osm3s: Option<Osm3s>,
            elements: Vec<Element>,
        }

        let helper = OverpassResponseHelper::deserialize(deserializer)?;
        let mut elements_map = BTreeMap::new();

        for element in helper.elements {
            let osm_id = match &element {
                Element::Node(n) => OSMId::Node(n.id),
                Element::Way(w) => OSMId::Way(w.id),
                Element::Relation(r) => OSMId::Relation(r.id),
            };
            elements_map.insert(osm_id, element);
        }

        Ok(OverpassResponse {
            version: helper.version,
            generator: helper.generator,
            osm3s: helper.osm3s,
            elements: elements_map,
        })
    }
}

/// Custom serialization for elements BTreeMap to convert OSMId keys to strings
fn serialize_elements<S>(
    elements: &BTreeMap<OSMId, Element>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(elements.len()))?;
    for (key, value) in elements {
        map.serialize_entry(&key.to_string(), value)?;
    }
    map.end()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Osm3s {
    pub timestamp_osm_base: Option<String>,
    pub copyright: Option<String>,
}

/// An OSM element (node, way, or relation)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Node(Node),
    Way(Way),
    Relation(Relation),
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Element::Node(n) => write!(f, "node/{}", n.id),
            Element::Way(w) => write!(f, "way/{}", w.id),
            Element::Relation(r) => write!(f, "relation/{}", r.id),
        }
    }
}

impl Element {
    /// Get the ID of this element
    pub fn id(&self) -> u64 {
        match self {
            Element::Node(n) => n.id,
            Element::Way(w) => w.id,
            Element::Relation(r) => r.id,
        }
    }

    /// Get the type of this element as a string
    pub fn element_type(&self) -> &str {
        match self {
            Element::Node(_) => "node",
            Element::Way(_) => "way",
            Element::Relation(_) => "relation",
        }
    }
}

impl HasTags for Element {
    fn tags(&self) -> Option<&HashMap<String, String>> {
        match self {
            Element::Node(n) => n.tags.as_ref(),
            Element::Way(w) => w.tags.as_ref(),
            Element::Relation(r) => r.tags.as_ref(),
        }
    }
}

/// Implement Cacheable trait for Element
impl crate::Cacheable for Element {
    fn global_id(&self) -> crate::GlobalId {
        let osm_id = match self {
            Element::Node(n) => crate::overpass::OSMId::Node(n.id),
            Element::Way(w) => crate::overpass::OSMId::Way(w.id),
            Element::Relation(r) => crate::overpass::OSMId::Relation(r.id),
        };
        crate::GlobalId::OSM(osm_id)
    }

    fn cache_key(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
}

impl HasTags for Node {
    fn tags(&self) -> Option<&HashMap<String, String>> {
        self.tags.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Way {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Vec<Coordinate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
}

impl HasTags for Way {
    fn tags(&self) -> Option<&HashMap<String, String>> {
        self.tags.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<Member>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Vec<Coordinate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changeset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
}

impl HasTags for Relation {
    fn tags(&self) -> Option<&HashMap<String, String>> {
        self.tags.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    #[serde(rename = "type")]
    pub member_type: String,
    #[serde(rename = "ref")]
    pub reference: u64,
    pub role: String,
    /// Geometry coordinates for this member (populated when using `out geom;`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Vec<Coordinate>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_node() {
        let json = r#"{
            "type": "node",
            "id": 123,
            "lat": 28.8,
            "lon": -96.2,
            "tags": {"name": "Test"}
        }"#;

        let element: Element = serde_json::from_str(json).unwrap();
        match element {
            Element::Node(node) => {
                assert_eq!(node.id, 123);
                assert_eq!(node.lat, Some(28.8));
                assert_eq!(node.lon, Some(-96.2));
            }
            _ => panic!("Expected Node"),
        }
    }

    #[test]
    fn test_deserialize_way() {
        let json = r#"{
            "type": "way",
            "id": 456,
            "nodes": [1, 2, 3],
            "tags": {"highway": "primary"}
        }"#;

        let element: Element = serde_json::from_str(json).unwrap();
        match element {
            Element::Way(way) => {
                assert_eq!(way.id, 456);
                assert_eq!(way.nodes, Some(vec![1, 2, 3]));
            }
            _ => panic!("Expected Way"),
        }
    }

    #[test]
    fn test_deserialize_response() {
        let json = r#"{
            "version": 0.6,
            "generator": "Overpass API",
            "elements": [
                {"type": "node", "id": 123, "lat": 28.8, "lon": -96.2}
            ]
        }"#;

        let response: OverpassResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.version, Some(0.6));
        assert_eq!(response.elements.len(), 1);
        assert!(response.elements.contains_key(&OSMId::Node(123)));
    }

    #[test]
    fn test_element_display() {
        let node = Element::Node(Node {
            id: 123,
            lat: Some(28.8),
            lon: Some(-96.2),
            tags: None,
            timestamp: None,
            version: None,
            changeset: None,
            user: None,
            uid: None,
        });
        assert_eq!(node.to_string(), "node/123");
        assert_eq!(node.id(), 123);
        assert_eq!(node.element_type(), "node");
    }
}
