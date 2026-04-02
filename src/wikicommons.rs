pub mod api;
pub mod models;
pub mod processing;

pub use api::{WikicommonsClient, enrich_category_members};
pub use models::{
    CategoryMember, CategoryMembersResponse, CategoryResult, EnrichedCategoryMember,
    EnrichedCategoryResult, ImageInfo, ImageInfoResponse, MediaWikiNS, PageImageInfo,
};
// Core processing functions shared by CLI and batch modes
pub use processing::{
    DownloadResult, fetch_and_process_categorymembers, fetch_and_process_wikicommons,
};
