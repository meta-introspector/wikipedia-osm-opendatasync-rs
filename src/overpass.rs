pub mod api;
pub mod id;
pub mod models;
pub mod processing;
pub mod query;
pub mod request;

pub use api::OverpassClient;
pub use id::OSMId;
pub use models::{Coordinate, Element, Member, Node, OverpassResponse, Relation, Way};
pub use query::BoundingBox;
pub use request::{OutputFormat, Request, RequestBuilder, RequestBuilderError};
// Core processing function shared by CLI and batch modes
pub use processing::fetch_and_process_overpass;
