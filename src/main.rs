use clap::{Parser, Subcommand};
use opendatasync::{
    Cacheable, DiskCacheMiddleware, Error,
    overpass::BoundingBox,
    request::{
        DEFAULT_KEEP_QIDS, DEFAULT_ONLY_VALUES, DEFAULT_TIMEOUT, DEFAULT_TRAVERSE_COMMONS,
        DEFAULT_TRAVERSE_OSM, OverpassQueryRequest, WikicommonsCategorymembersRequest,
        WikicommonsImageinfoRequest, WikicommonsRequest, WikidataGetClaimsRequest,
        WikidataGetEntitiesRequest, WikidataRequest,
    },
    types::{CommonsTraverseDepth, OutputFormat, ResolveMode},
    wikicommons::WikicommonsClient,
    wikidata::{
        CsvConfig, GetClaimsQuery, JsonConfig, WikidataClient, WikidataId,
        fetch_and_process_wikidata, serialize_claims_to_csv, serialize_entities_to_csv,
        serialize_entities_to_json,
    },
};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "opendatasync")]
#[command(about = "A tool for syncing open data from various sources")]
#[command(version)]
struct Cli {
    #[arg(long, help = "Enable verbose logging")]
    verbose: bool,
    #[arg(long, help = "Output directory for saving results to files")]
    output_dir: Option<String>,
    #[arg(
        long,
        default_value = "./cache",
        help = "Cache directory for storing API responses"
    )]
    cache_dir: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Wikidata {
        #[command(subcommand)]
        action: WikidataAction,
    },
    Wikicommons {
        #[command(subcommand)]
        action: WikicommonsAction,
    },
    Overpass {
        #[command(subcommand)]
        action: OverpassAction,
    },
    /// Process multiple data sources from a batch file
    Batch {
        #[arg(
            long,
            help = "Path to batch TOML file containing data sources to process"
        )]
        batch_file: String,
    },
}

#[derive(Subcommand)]
enum WikidataAction {
    Wbgetentities {
        #[arg(short, long = "qid", action = clap::ArgAction::Append, conflicts_with = "ids_from_file", required_unless_present = "ids_from_file")]
        qids: Vec<String>,
        #[arg(long = "ids-from-file", help = "Read QIDs from a file (one per line)")]
        ids_from_file: Option<String>,
        #[arg(short, long, value_enum, default_value_t)]
        format: OutputFormat,
        #[arg(
            long = "resolve-headers",
            value_enum,
            default_value_t,
            help = "Resolve header property IDs to names: none, all (default), or select"
        )]
        resolve_headers: ResolveMode,
        #[arg(
            long = "select-header",
            required_if_eq("resolve_headers", "select"),
            help = "Specific header properties to resolve (e.g., P31, P106). Required when --resolve-headers=select"
        )]
        select_headers: Vec<String>,
        #[arg(
            long = "resolve-data",
            value_enum,
            default_value_t,
            help = "Resolve data property values to names: none, all (default), or select"
        )]
        resolve_data: ResolveMode,
        #[arg(
            long = "select-data",
            required_if_eq("resolve_data", "select"),
            help = "Specific data properties to resolve (e.g., P31, P106). Required when --resolve-data=select"
        )]
        select_data: Vec<String>,
        #[arg(long = "keep-qids", action = clap::ArgAction::Set, default_value_t = DEFAULT_KEEP_QIDS, help = "When used with --resolve-data, keep QID columns alongside resolved names (default: true)")]
        keep_qids: bool,
        #[arg(
            long = "keep-filename",
            help = "When used with --resolve-data for Commons media properties, keep filename columns alongside resolved pageids (default: false)"
        )]
        keep_filename: bool,
        #[arg(long = "only-values", action = clap::ArgAction::Set, default_value_t = DEFAULT_ONLY_VALUES, help = "In JSON output, prune non-value fields from statements (id, rank, qualifiers, references) (default: true)")]
        only_values: bool,
        #[arg(
            long = "preserve-qualifiers-for",
            help = "When --only-values is true, preserve qualifiers for these specific properties (e.g., P6375, P969)"
        )]
        preserve_qualifiers_for: Vec<String>,
        #[arg(
            long = "traverse-properties",
            help = "Recursively follow and fetch entities referenced by these properties (e.g., P361 for 'part of')"
        )]
        traverse_properties: Vec<String>,
        #[arg(long = "traverse-osm", action = clap::ArgAction::Set, default_value_t = DEFAULT_TRAVERSE_OSM, help = "Shortcut to traverse all OSM properties (P402, P10689, P11693). Can be combined with --traverse-properties (default: true)")]
        traverse_osm: bool,
        #[arg(long = "traverse-commons", action = clap::ArgAction::Set, default_value_t = DEFAULT_TRAVERSE_COMMONS, help = "Shortcut to traverse Commons category property (P373) and fetch category members. Can be combined with --traverse-properties (default: true)")]
        traverse_commons: bool,
        #[arg(
            long = "traverse-commons-depth",
            value_enum,
            default_value_t,
            help = "Depth of Commons traversal: category (members only) or page (members + imageinfo) (default: page)"
        )]
        traverse_commons_depth: CommonsTraverseDepth,
    },
    Wbgetclaims {
        #[arg(short, long)]
        entity: String,
        #[arg(short, long)]
        property: Option<String>,
        #[arg(short, long, value_enum, default_value_t)]
        format: OutputFormat,
    },
}

#[derive(Subcommand)]
enum WikicommonsAction {
    /// Fetch category members from Wikimedia Commons
    Categorymembers {
        /// Category names (without "Category:" prefix)
        #[arg(long = "category", action = clap::ArgAction::Append, conflicts_with = "ids_from_file")]
        categories: Vec<String>,
        #[arg(
            long = "ids-from-file",
            help = "Read category names from a file (one per line)"
        )]
        ids_from_file: Option<String>,
        /// Fetch imageinfo for all NS_FILE pageids and write to separate file (wikicommons-imageinfo.json)
        #[arg(long = "traverse-pageid")]
        traverse_pageid: bool,
        /// Regex pattern to match subcategories for recursive traversal
        #[arg(long = "recurse-subcategory-pattern")]
        recurse_subcategory_pattern: Option<String>,
        /// Download images to output directory when --traverse-pageid is used
        #[arg(long = "download-images", requires = "traverse_pageid")]
        download_images: bool,
    },
    /// Fetch image info from Wikimedia Commons
    Imageinfo {
        /// Page IDs to fetch image info for
        #[arg(long = "pageid", action = clap::ArgAction::Append, conflicts_with = "titles", conflicts_with = "ids_from_file")]
        pageids: Vec<u64>,

        /// File titles to fetch image info for (e.g., WestburyHouston.JPG)
        #[arg(long = "title", action = clap::ArgAction::Append, conflicts_with = "pageids", conflicts_with = "ids_from_file")]
        titles: Vec<String>,

        /// Read page IDs or titles from a file (one per line)
        #[arg(
            long = "ids-from-file",
            conflicts_with = "pageids",
            conflicts_with = "titles"
        )]
        ids_from_file: Option<String>,
    },
}

#[derive(Subcommand)]
enum OverpassAction {
    /// Query specific OSM elements by ID
    Query {
        /// Bounding box constraint (optional): "south,west,north,east"
        #[arg(short = 'b', long)]
        bbox: Option<BoundingBox>,

        /// Node IDs to query
        #[arg(short = 'n', long = "node", conflicts_with = "ids_from_file")]
        nodes: Vec<u64>,

        /// Way IDs to query
        #[arg(short = 'w', long = "way", conflicts_with = "ids_from_file")]
        ways: Vec<u64>,

        /// Relation IDs to query
        #[arg(short = 'r', long = "relation", conflicts_with = "ids_from_file")]
        relations: Vec<u64>,

        /// Read OSM IDs from a file (format: "node/123", "way/456", "relation/789", one per line)
        #[arg(long = "ids-from-file")]
        ids_from_file: Option<String>,

        /// Timeout in seconds
        #[arg(long, default_value_t = DEFAULT_TIMEOUT)]
        timeout: u8,
    },
}

/// Create output writer based on output directory and filename
fn create_output_writer(output_dir: Option<&str>, filename: &str) -> Result<Box<dyn Write>, Error> {
    match output_dir {
        Some(dir) => {
            let dir_path = Path::new(dir);
            fs::create_dir_all(dir_path)?;
            let file_path = dir_path.join(filename);
            let file = fs::File::create(&file_path)?;
            Ok(Box::new(file))
        }
        None => Ok(Box::new(io::stdout())),
    }
}

/// Prepare CSV configuration based on resolution flags
fn prepare_csv_config(
    resolve_headers: ResolveMode,
    select_headers: &[String],
    resolve_data: ResolveMode,
    select_data: &[String],
    keep_qids: bool,
    keep_filename: bool,
) -> Result<CsvConfig, Error> {
    let resolve_property_headers = match resolve_headers {
        ResolveMode::None => None,
        ResolveMode::All => Some(Vec::new()), // Empty vec means resolve all
        ResolveMode::Select => {
            let resolved_ids: Result<Vec<WikidataId>, Error> = select_headers
                .iter()
                .map(|s| WikidataId::try_from(s.as_str()))
                .collect();
            Some(resolved_ids?)
        }
    };

    let resolve_data_values = match resolve_data {
        ResolveMode::None => None,
        ResolveMode::All => Some(Vec::new()), // Empty vec means resolve all
        ResolveMode::Select => {
            let resolved_ids: Result<Vec<WikidataId>, Error> = select_data
                .iter()
                .map(|s| WikidataId::try_from(s.as_str()))
                .collect();
            Some(resolved_ids?)
        }
    };

    Ok(CsvConfig {
        resolve_property_headers,
        resolve_data_values,
        keep_qids,
        keep_filename,
        language: "en".to_string(),
    })
}

/// Handle Wikidata commands
async fn handle_wikidata_command(
    request: WikidataRequest,
    output_dir: Option<&str>,
    cache: Option<Arc<DiskCacheMiddleware>>,
) -> Result<(), Error> {
    match request {
        WikidataRequest::Wbgetentities(ref wbgetentities_req) => {
            // Require cache for processing
            let cache_arc = cache.clone().ok_or_else(|| {
                Error::InvalidInput("Cache required for Wikidata processing".to_string())
            })?;

            // Call shared processing function
            let (entities, osm_elements, commons_data, commons_imageinfo) =
                fetch_and_process_wikidata(wbgetentities_req, cache_arc).await?;

            // Extract config for output writing
            let WikidataGetEntitiesRequest {
                format,
                resolve_headers,
                select_headers,
                resolve_data,
                select_data,
                keep_qids,
                keep_filename,
                only_values,
                preserve_qualifiers_for,
                ..
            } = wbgetentities_req.clone();

            // Write OSM elements if any
            if !osm_elements.is_empty() {
                let filename = "overpass.json";
                let mut writer = create_output_writer(output_dir, filename)?;
                // Convert OSMId keys to strings for JSON serialization
                use std::collections::BTreeMap;
                let elements_with_string_keys: BTreeMap<String, &opendatasync::overpass::Element> =
                    osm_elements
                        .iter()
                        .map(|(id, element)| (id.cache_key(), element))
                        .collect();
                writeln!(
                    writer,
                    "{}",
                    serde_json::to_string_pretty(&elements_with_string_keys)?
                )?;
            }

            // Write Commons categorymembers if any
            if !commons_data.is_empty() {
                let filename = "wikicommons-categorymembers.json";
                let mut writer = create_output_writer(output_dir, filename)?;
                writeln!(writer, "{}", serde_json::to_string_pretty(&commons_data)?)?;
            }

            // Write Commons imageinfo if any
            if !commons_imageinfo.is_empty() {
                let filename = "wikicommons-imageinfo.json";
                let mut writer = create_output_writer(output_dir, filename)?;
                writeln!(
                    writer,
                    "{}",
                    serde_json::to_string_pretty(&commons_imageinfo)?
                )?;
            }

            // Write main Wikidata entities output
            match format {
                OutputFormat::Csv => {
                    let config = prepare_csv_config(
                        resolve_headers,
                        &select_headers,
                        resolve_data,
                        &select_data,
                        keep_qids,
                        keep_filename,
                    )?;
                    let filename = "wikidata-wbgetentities.csv";
                    let mut writer = create_output_writer(output_dir, filename)?;
                    serialize_entities_to_csv(&entities, &mut writer, config).await?;
                }
                OutputFormat::Json => {
                    // Convert preserve_qualifiers_for from Vec<String> to Vec<WikidataId>
                    let preserve_qualifiers: Vec<WikidataId> = preserve_qualifiers_for
                        .iter()
                        .map(|s| WikidataId::try_from(s.as_str()))
                        .collect::<Result<Vec<_>, _>>()?;

                    let json_config = JsonConfig {
                        only_values,
                        preserve_qualifiers_for: preserve_qualifiers,
                    };
                    let filename = "wikidata-wbgetentities.json";
                    let mut writer = create_output_writer(output_dir, filename)?;
                    serialize_entities_to_json(&entities, &mut writer, json_config)?;
                }
            }
        }
        WikidataRequest::Wbgetclaims(WikidataGetClaimsRequest {
            entity,
            property,
            format,
        }) => {
            let client = if let Some(cache_ref) = cache {
                WikidataClient::with_cache(cache_ref)
            } else {
                WikidataClient::new()
            };
            let entity_id = WikidataId::try_from(entity.as_str())?;

            let query = match property {
                Some(prop_str) => {
                    let property_id = WikidataId::try_from(prop_str.as_str())?;
                    GetClaimsQuery::builder()
                        .entity(entity_id)
                        .property(Some(property_id))
                        .build()?
                }
                None => GetClaimsQuery::builder().entity(entity_id).build()?,
            };

            let response = client.get_claims(&query).await?;

            match format {
                OutputFormat::Csv => {
                    let filename = "wikidata-wbgetclaims.csv";
                    let mut writer = create_output_writer(output_dir, filename)?;
                    serialize_claims_to_csv(&entity, &response, &mut writer)?;
                }
                OutputFormat::Json => {
                    let filename = "wikidata-wbgetclaims.json";
                    let mut writer = create_output_writer(output_dir, filename)?;
                    writeln!(writer, "{}", serde_json::to_string_pretty(&response)?)?;
                }
            }
        }
    }

    Ok(())
}

/// Handle Wikicommons commands
async fn handle_wikicommons_command(
    request: WikicommonsRequest,
    output_dir: Option<&str>,
    cache: Option<Arc<DiskCacheMiddleware>>,
) -> Result<(), Error> {
    match request {
        WikicommonsRequest::Categorymembers(ref categorymembers_req) => {
            // Validate input
            let categories = categorymembers_req.resolve_categories()?;
            if categories.is_empty() {
                return Err(Error::InvalidInput(
                    "At least one --category is required".to_string(),
                ));
            }

            // Require cache for processing
            let cache_arc = cache.ok_or_else(|| {
                Error::InvalidInput("Cache required for Wikicommons processing".to_string())
            })?;

            // Convert output_dir to Path for processing function
            let output_path = output_dir.map(Path::new);

            // Call shared processing function
            let (commons_data, commons_imageinfo, _download_results) =
                opendatasync::wikicommons::fetch_and_process_categorymembers(
                    categorymembers_req,
                    cache_arc,
                    output_path,
                )
                .await?;

            // Write categorymembers output
            if !commons_data.is_empty() {
                let filename = "wikicommons-categorymembers.json";
                let mut writer = create_output_writer(output_dir, filename)?;
                writeln!(writer, "{}", serde_json::to_string_pretty(&commons_data)?)?;
            }

            // Write imageinfo output if any
            if !commons_imageinfo.is_empty() {
                let imageinfo_filename = "wikicommons-imageinfo.json";
                let mut imageinfo_writer = create_output_writer(output_dir, imageinfo_filename)?;
                writeln!(
                    imageinfo_writer,
                    "{}",
                    serde_json::to_string_pretty(&commons_imageinfo)?
                )?;
            }
        }
        WikicommonsRequest::Imageinfo(ref imageinfo_req) => {
            // Resolve pageids and titles from either inline lists or file
            let (pageids, titles) = imageinfo_req.resolve_pageids_and_titles()?;

            if pageids.is_empty() && titles.is_empty() {
                return Err(Error::InvalidInput(
                    "At least one --pageid, --title, or --ids-from-file is required".to_string(),
                ));
            }

            let client = if let Some(cache_ref) = cache {
                WikicommonsClient::with_cache(cache_ref)
            } else {
                WikicommonsClient::new()
            };

            let results = client.get_image_info(&pageids, &titles).await?;

            // Output results as map: pageid/title -> imageinfo
            let filename = "wikicommons-imageinfo.json";
            let mut writer = create_output_writer(output_dir, filename)?;
            writeln!(writer, "{}", serde_json::to_string_pretty(&results)?)?;
        }
    }

    Ok(())
}

/// Handle Overpass commands
async fn handle_overpass_command(
    request: OverpassQueryRequest,
    output_dir: Option<&str>,
    cache: Option<Arc<DiskCacheMiddleware>>,
) -> Result<(), Error> {
    // Require cache for processing
    let cache_arc = cache
        .ok_or_else(|| Error::InvalidInput("Cache required for Overpass processing".to_string()))?;

    // Call shared processing function
    let elements = opendatasync::overpass::fetch_and_process_overpass(&request, cache_arc).await?;

    // Write output
    let filename = "overpass.json";
    let mut writer = create_output_writer(output_dir, filename)?;
    writeln!(writer, "{}", serde_json::to_string_pretty(&elements)?)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    // Initialize tracing if verbose is enabled
    if cli.verbose {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    "opendatasync=debug,reqwest_middleware=debug,reqwest_tracing=debug".into()
                }),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();

        tracing::info!("Verbose logging enabled");
    }

    // Initialize cache with the provided or default cache directory
    tracing::info!("Cache enabled at: {}", cli.cache_dir);
    let cache_middleware = DiskCacheMiddleware::new(&cli.cache_dir)?;
    let cache = Some(Arc::new(cache_middleware));

    let output_dir = cli.output_dir.as_deref();

    match cli.command {
        Commands::Wikidata { action } => {
            let request = match action {
                WikidataAction::Wbgetentities {
                    qids,
                    ids_from_file,
                    format,
                    resolve_headers,
                    select_headers,
                    resolve_data,
                    select_data,
                    keep_qids,
                    keep_filename,
                    only_values,
                    preserve_qualifiers_for,
                    traverse_properties,
                    traverse_osm,
                    traverse_commons,
                    traverse_commons_depth,
                } => WikidataRequest::Wbgetentities(WikidataGetEntitiesRequest {
                    qids,
                    ids_from_file,
                    format,
                    resolve_headers,
                    select_headers,
                    resolve_data,
                    select_data,
                    keep_qids,
                    keep_filename,
                    only_values,
                    preserve_qualifiers_for,
                    traverse_properties,
                    traverse_osm,
                    traverse_commons,
                    traverse_commons_depth,
                }),
                WikidataAction::Wbgetclaims {
                    entity,
                    property,
                    format,
                } => WikidataRequest::Wbgetclaims(WikidataGetClaimsRequest {
                    entity,
                    property,
                    format,
                }),
            };
            handle_wikidata_command(request, output_dir, cache.clone()).await?
        }
        Commands::Wikicommons { action } => {
            let request = match action {
                WikicommonsAction::Categorymembers {
                    categories,
                    ids_from_file,
                    traverse_pageid,
                    recurse_subcategory_pattern,
                    download_images,
                } => WikicommonsRequest::Categorymembers(WikicommonsCategorymembersRequest {
                    categories,
                    ids_from_file,
                    traverse_pageid,
                    recurse_subcategory_pattern,
                    download_images,
                }),
                WikicommonsAction::Imageinfo {
                    pageids,
                    titles,
                    ids_from_file,
                } => WikicommonsRequest::Imageinfo(WikicommonsImageinfoRequest {
                    pageids,
                    titles,
                    ids_from_file,
                }),
            };
            handle_wikicommons_command(request, output_dir, cache.clone()).await?
        }
        Commands::Overpass { action } => {
            let request = match action {
                OverpassAction::Query {
                    bbox,
                    nodes,
                    ways,
                    relations,
                    ids_from_file,
                    timeout,
                } => OverpassQueryRequest {
                    bbox,
                    nodes,
                    ways,
                    relations,
                    ids_from_file,
                    timeout,
                },
            };
            handle_overpass_command(request, output_dir, cache.clone()).await?
        }
        Commands::Batch { batch_file } => {
            // Validate required arguments for batch command
            if output_dir.is_none() {
                return Err(Error::InvalidInput(
                    "batch command requires --output-dir to be set".to_string(),
                ));
            }

            opendatasync::batch::process_batch_file(
                &batch_file,
                output_dir.unwrap(),
                cache.clone().unwrap(),
            )
            .await?;
        }
    }

    // Flush cache to disk at the end (deferred write)
    if let Some(cache_ref) = cache {
        tracing::info!("Flushing cache to disk...");
        cache_ref.flush()?;
        tracing::info!("Cache flushed successfully");
    }

    Ok(())
}
