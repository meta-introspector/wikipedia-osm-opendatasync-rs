use crate::overpass::{BoundingBox, OSMId};
use derive_builder::Builder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    // Future: Xml, Csv
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
        }
    }
}

/// Represents an Overpass API request with builder pattern support
#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
pub struct Request {
    /// Optional bounding box constraint (becomes [bbox:...] header in query)
    #[builder(default)]
    pub bounding_box: Option<BoundingBox>,

    /// IDs to query - becomes (node(id);way(id);...) union in query
    #[builder(default)]
    pub query_by_ids: Vec<OSMId>,

    /// Output format
    #[builder(default)]
    pub output_format: OutputFormat,

    /// Query timeout in seconds
    #[builder(default = "25")]
    pub timeout: u8,
}

impl Request {
    /// Create a new request builder
    pub fn builder() -> RequestBuilder {
        RequestBuilder::default()
    }

    /// Generates the Overpass QL query string
    pub fn to_query_string(&self) -> String {
        let mut query = String::new();

        // Add settings header
        query.push_str(&format!(
            "[out:{}][timeout:{}]",
            self.output_format.as_str(),
            self.timeout
        ));

        // Add optional bounding box
        if let Some(bbox) = self.bounding_box {
            query.push_str(&format!("[bbox:{}]", bbox));
        }

        query.push_str(";\n");

        // Add ID queries as union
        if !self.query_by_ids.is_empty() {
            query.push_str("(\n");
            for id in &self.query_by_ids {
                query.push_str(&format!("  {};\n", id));
            }
            query.push_str(");\n");
        }

        // Add output statement
        query.push_str("out geom;");

        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_request_builder_defaults() {
        let request = Request::builder()
            .query_by_ids(vec![OSMId::Node(123)])
            .build()
            .unwrap();

        assert_eq!(request.timeout, 25);
        assert_eq!(request.output_format, OutputFormat::Json);
        assert!(request.bounding_box.is_none());
    }

    #[test]
    fn test_request_with_bbox() {
        let bbox = BoundingBox::from_str("28.8,-96.2,30.4,-94.7").unwrap();
        let request = Request::builder()
            .bounding_box(Some(bbox))
            .query_by_ids(vec![OSMId::Node(123)])
            .build()
            .unwrap();

        assert!(request.bounding_box.is_some());
    }

    #[test]
    fn test_query_string_simple() {
        let request = Request::builder()
            .query_by_ids(vec![
                OSMId::Node(123),
                OSMId::Way(456),
                OSMId::Relation(789),
            ])
            .build()
            .unwrap();

        let query = request.to_query_string();
        assert!(query.contains("[out:json][timeout:25]"));
        assert!(query.contains("node(123)"));
        assert!(query.contains("way(456)"));
        assert!(query.contains("relation(789)"));
        assert!(query.contains("out geom;"));
    }

    #[test]
    fn test_query_string_with_bbox() {
        let bbox = BoundingBox::from_str("28.8,-96.2,30.4,-94.7").unwrap();
        let request = Request::builder()
            .bounding_box(Some(bbox))
            .query_by_ids(vec![OSMId::Node(123)])
            .timeout(60)
            .build()
            .unwrap();

        let query = request.to_query_string();
        assert!(query.contains("[out:json][timeout:60]"));
        assert!(query.contains("[bbox:28.8,-96.2,30.4,-94.7]"));
        assert!(query.contains("node(123)"));
    }

    #[test]
    fn test_query_string_empty_ids() {
        let request = Request::builder().build().unwrap();

        let query = request.to_query_string();
        assert!(query.contains("[out:json][timeout:25]"));
        assert!(query.contains("out geom;"));
        // Should not contain union syntax
        assert!(!query.contains("(\n"));
    }
}
