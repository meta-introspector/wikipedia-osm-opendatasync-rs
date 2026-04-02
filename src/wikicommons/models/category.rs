use super::imageinfo::ImageInfo;
use crate::{Cacheable, GlobalId};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;

/// MediaWiki namespace constants
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaWikiNS {
    Special = -1,
    Media = -2,
    Main = 0,
    Talk = 1,
    User = 2,
    UserTalk = 3,
    Project = 4,
    ProjectTalk = 5,
    File = 6,
    FileTalk = 7,
    MediaWiki = 8,
    MediaWikiTalk = 9,
    Template = 10,
    TemplateTalk = 11,
    Help = 12,
    HelpTalk = 13,
    Category = 14,
    CategoryTalk = 15,
}

impl MediaWikiNS {
    /// Get the lowercase name for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaWikiNS::Special => "special",
            MediaWikiNS::Media => "media",
            MediaWikiNS::Main => "main",
            MediaWikiNS::Talk => "talk",
            MediaWikiNS::User => "user",
            MediaWikiNS::UserTalk => "user talk",
            MediaWikiNS::Project => "project",
            MediaWikiNS::ProjectTalk => "project talk",
            MediaWikiNS::File => "file",
            MediaWikiNS::FileTalk => "file talk",
            MediaWikiNS::MediaWiki => "mediawiki",
            MediaWikiNS::MediaWikiTalk => "mediawiki talk",
            MediaWikiNS::Template => "template",
            MediaWikiNS::TemplateTalk => "template talk",
            MediaWikiNS::Help => "help",
            MediaWikiNS::HelpTalk => "help talk",
            MediaWikiNS::Category => "category",
            MediaWikiNS::CategoryTalk => "category talk",
        }
    }
}

impl From<i32> for MediaWikiNS {
    fn from(value: i32) -> Self {
        match value {
            -1 => MediaWikiNS::Special,
            -2 => MediaWikiNS::Media,
            0 => MediaWikiNS::Main,
            1 => MediaWikiNS::Talk,
            2 => MediaWikiNS::User,
            3 => MediaWikiNS::UserTalk,
            4 => MediaWikiNS::Project,
            5 => MediaWikiNS::ProjectTalk,
            6 => MediaWikiNS::File,
            7 => MediaWikiNS::FileTalk,
            8 => MediaWikiNS::MediaWiki,
            9 => MediaWikiNS::MediaWikiTalk,
            10 => MediaWikiNS::Template,
            11 => MediaWikiNS::TemplateTalk,
            12 => MediaWikiNS::Help,
            13 => MediaWikiNS::HelpTalk,
            14 => MediaWikiNS::Category,
            15 => MediaWikiNS::CategoryTalk,
            _ => MediaWikiNS::Main, // Default fallback
        }
    }
}

impl Serialize for MediaWikiNS {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MediaWikiNS {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "special" => Ok(MediaWikiNS::Special),
            "media" => Ok(MediaWikiNS::Media),
            "main" => Ok(MediaWikiNS::Main),
            "talk" => Ok(MediaWikiNS::Talk),
            "user" => Ok(MediaWikiNS::User),
            "user talk" => Ok(MediaWikiNS::UserTalk),
            "project" => Ok(MediaWikiNS::Project),
            "project talk" => Ok(MediaWikiNS::ProjectTalk),
            "file" => Ok(MediaWikiNS::File),
            "file talk" => Ok(MediaWikiNS::FileTalk),
            "mediawiki" => Ok(MediaWikiNS::MediaWiki),
            "mediawiki talk" => Ok(MediaWikiNS::MediaWikiTalk),
            "template" => Ok(MediaWikiNS::Template),
            "template talk" => Ok(MediaWikiNS::TemplateTalk),
            "help" => Ok(MediaWikiNS::Help),
            "help talk" => Ok(MediaWikiNS::HelpTalk),
            "category" => Ok(MediaWikiNS::Category),
            "category talk" => Ok(MediaWikiNS::CategoryTalk),
            _ => Ok(MediaWikiNS::Main), // Default fallback
        }
    }
}

/// API response for categorymembers query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMembersResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batchcomplete: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#continue: Option<ContinueToken>,
    pub query: CategoryMembersQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinueToken {
    pub cmcontinue: String,
    pub r#continue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMembersQuery {
    pub categorymembers: Vec<CategoryMemberRaw>,
}

/// Raw category member as returned by the API (internal use only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMemberRaw {
    pub pageid: u64,
    pub ns: i32,
    pub title: String,
}

/// Category member for output (indexed by ns and pageid in CategoryResult)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMember {
    pub title: String,
}

/// Enriched category member with imageinfo from imageinfo API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedCategoryMember {
    pub title: String,
    /// Single imageinfo object extracted from imageinfo[0]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imageinfo: Option<ImageInfo>,
}

impl From<CategoryMember> for EnrichedCategoryMember {
    fn from(member: CategoryMember) -> Self {
        Self {
            title: member.title,
            imageinfo: None,
        }
    }
}

/// Wrapper for storing complete category results with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryResult {
    /// The category name (without "Category:" prefix)
    pub category: String,
    /// All members of the category, indexed by namespace then pageid
    pub members: BTreeMap<MediaWikiNS, BTreeMap<u64, CategoryMember>>,
    /// Parent category (if this was discovered through recursive traversal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_category: Option<String>,
}

impl Cacheable for CategoryResult {
    fn global_id(&self) -> GlobalId {
        GlobalId::WikicommonsCategory(self.category.clone())
    }

    fn cache_key(&self) -> String {
        self.category.clone()
    }
}

/// Enriched category result with imageinfo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedCategoryResult {
    /// The category name (without "Category:" prefix)
    pub category: String,
    /// All members of the category with enriched imageinfo, indexed by namespace then pageid
    pub members: BTreeMap<MediaWikiNS, BTreeMap<u64, EnrichedCategoryMember>>,
}
