use crate::{Cacheable, GlobalId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// API response for imageinfo query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfoResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batchcomplete: Option<String>,
    pub query: ImageInfoQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfoQuery {
    pub pages: serde_json::Map<String, Value>,
}

/// Individual page with imageinfo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageImageInfo {
    pub pageid: u64,
    pub ns: i32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imageinfo: Option<Vec<ImageInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptionurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumburl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbwidth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbheight: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<MetadataItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extmetadata: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataItem {
    pub name: String,
    pub value: Value,
}

impl Cacheable for PageImageInfo {
    fn global_id(&self) -> GlobalId {
        GlobalId::WikicommonsPageId(self.pageid)
    }

    fn cache_key(&self) -> String {
        self.pageid.to_string()
    }
}
