use super::global_id::GlobalId;
use crate::Error;
use sanitize_filename::{Options, sanitize_with_options};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Sanitize a filename for safe filesystem usage
/// Returns (sanitized_filename, was_modified)
fn sanitize_filename_for_cache(filename: &str) -> (String, bool) {
    let options = Options {
        truncate: true,
        windows: true, // Be conservative for cross-platform compatibility
        replacement: "_",
    };

    let sanitized = sanitize_with_options(filename, options);
    let was_modified = sanitized != filename;

    if was_modified {
        tracing::warn!(
            "Filename was sanitized for cache: '{}' -> '{}'",
            filename,
            sanitized
        );
    }

    (sanitized, was_modified)
}

/// Trait for entities that can be cached
pub trait Cacheable: Serialize + DeserializeOwned + Sized {
    /// Extract the global ID from this entity
    fn global_id(&self) -> GlobalId;

    /// Key for HashMap (usually the ID as string)
    fn cache_key(&self) -> String {
        self.global_id().to_key()
    }

    /// Check if this entity should be cached
    /// Default is true. Override for entities that might be partial/incomplete.
    fn should_cache(&self) -> bool {
        true
    }
}

/// Unified disk cache middleware for all entity types
#[derive(Debug)]
pub struct DiskCacheMiddleware {
    cache_dir: PathBuf,
    // Writer uses interior mutability for pending writes
    pending_writes: Arc<Mutex<HashMap<GlobalId, serde_json::Value>>>,
}

impl DiskCacheMiddleware {
    /// Create a new cache middleware with the given cache directory
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self, Error> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Create directory structure
        fs::create_dir_all(cache_dir.join("wikidata/Q"))?;
        fs::create_dir_all(cache_dir.join("wikidata/P"))?;
        fs::create_dir_all(cache_dir.join("osm/node"))?;
        fs::create_dir_all(cache_dir.join("osm/way"))?;
        fs::create_dir_all(cache_dir.join("osm/rel"))?;
        fs::create_dir_all(cache_dir.join("wikicommons/category"))?;
        fs::create_dir_all(cache_dir.join("wikicommons/pageid"))?;
        fs::create_dir_all(cache_dir.join("wikicommons/filename"))?;

        Ok(Self {
            cache_dir,
            pending_writes: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Lock-free read of a single entity
    pub fn get<T: Cacheable>(&self, id: &GlobalId) -> Result<Option<T>, Error> {
        let path = id.to_cache_path(&self.cache_dir);

        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path)?;
        let entity: T = serde_json::from_str(&contents)?;
        Ok(Some(entity))
    }

    /// Lock-free batch read - returns (cached_map, missing_ids)
    pub fn get_many<T, K>(&self, ids: &[GlobalId]) -> Result<(BTreeMap<K, T>, Vec<GlobalId>), Error>
    where
        T: Cacheable,
        K: From<GlobalId> + Ord,
    {
        let mut cached = BTreeMap::new();
        let mut missing = Vec::new();

        for id in ids {
            match self.get::<T>(id)? {
                Some(entity) => {
                    tracing::debug!("Cache hit for {:?}", id);
                    let key = K::from(id.clone());
                    cached.insert(key, entity);
                }
                None => {
                    tracing::debug!("Cache miss for {:?}", id);
                    missing.push(id.clone());
                }
            }
        }

        Ok((cached, missing))
    }

    /// Stage a single entity for deferred write
    pub fn stage<T: Cacheable>(&self, entity: &T) -> Result<(), Error> {
        let id = entity.global_id();
        let value = serde_json::to_value(entity)?;

        let mut pending = self.pending_writes.lock().unwrap();
        pending.insert(id, value);

        Ok(())
    }

    /// Stage multiple entities for deferred write
    /// Filters out partial entities using should_cache()
    pub fn stage_many<T, K>(&self, entities: &BTreeMap<K, T>) -> Result<(), Error>
    where
        T: Cacheable,
        K: Ord,
    {
        let mut pending = self.pending_writes.lock().unwrap();

        for entity in entities.values() {
            // Skip partial entities that shouldn't be cached
            if !entity.should_cache() {
                tracing::debug!(
                    "Skipping cache for {:?} - partial entity",
                    entity.global_id()
                );
                continue;
            }

            let id = entity.global_id();
            let value = serde_json::to_value(entity)?;
            pending.insert(id, value);
        }

        Ok(())
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<(), Error> {
        let mut pending = self.pending_writes.lock().unwrap();
        let entries: Vec<_> = pending.drain().collect();
        drop(pending); // Release lock before I/O

        for (id, value) in entries {
            let path = id.to_cache_path(&self.cache_dir);
            let json = serde_json::to_string(&value)?;
            fs::write(path, json)?;
        }

        Ok(())
    }

    /// Get a Wikicommons page by title (via filename symlink cache)
    /// Returns None if not cached or symlink is broken
    pub fn get_by_title(
        &self,
        title: &str,
    ) -> Result<Option<crate::wikicommons::models::PageImageInfo>, Error> {
        // Strip "File:" prefix if present
        let filename = title.strip_prefix("File:").unwrap_or(title);

        // Sanitize filename
        let (sanitized, _) = sanitize_filename_for_cache(filename);

        // Build symlink path
        let symlink_path = self.cache_dir.join("wikicommons/filename").join(&sanitized);

        // Check if symlink exists
        if !symlink_path.exists() {
            tracing::debug!("No filename cache entry for: {}", title);
            return Ok(None);
        }

        // Try to read the symlink
        match fs::read_link(&symlink_path) {
            Ok(target) => {
                // Build the full target path (symlink might be relative)
                let target_path = if target.is_relative() {
                    symlink_path.parent().unwrap().join(target)
                } else {
                    target
                };

                // Check if target exists
                if !target_path.exists() {
                    tracing::warn!(
                        "Broken symlink for title '{}': target does not exist, removing symlink",
                        title
                    );
                    // Remove broken symlink
                    let _ = fs::remove_file(&symlink_path);
                    return Ok(None);
                }

                // Read and deserialize the target file
                let contents = fs::read_to_string(&target_path)?;
                let page_info: crate::wikicommons::models::PageImageInfo =
                    serde_json::from_str(&contents)?;
                tracing::debug!("Cache hit for title: {}", title);
                Ok(Some(page_info))
            }
            Err(e) => {
                tracing::warn!("Failed to read symlink for title '{}': {}", title, e);
                Ok(None)
            }
        }
    }

    /// Create a symlink from a title to its pageid cache entry
    /// Strips "File:" prefix and sanitizes the filename
    pub fn cache_title_symlink(&self, title: &str, pageid: u64) -> Result<(), Error> {
        // Strip "File:" prefix if present
        let filename = title.strip_prefix("File:").unwrap_or(title);

        // Sanitize filename (will warn if modified)
        let (sanitized, _) = sanitize_filename_for_cache(filename);

        // Build symlink path
        let symlink_path = self.cache_dir.join("wikicommons/filename").join(&sanitized);

        // Build relative target path: ../pageid/{pageid}.json
        let relative_target = PathBuf::from("../pageid").join(format!("{}.json", pageid));

        // Remove existing symlink if present
        if symlink_path.exists() {
            fs::remove_file(&symlink_path)?;
        }

        // Create the symlink (Unix-only for now)
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&relative_target, &symlink_path)?;
            tracing::debug!(
                "Created symlink: {} -> {}",
                sanitized,
                relative_target.display()
            );
        }

        #[cfg(not(unix))]
        {
            return Err(Error::InvalidInput(
                "Symlink creation is only supported on Unix systems".to_string(),
            ));
        }

        Ok(())
    }

    /// Generic fetch with caching - works for ANY cacheable entity type
    pub async fn fetch<T, F, Fut, Id, K>(
        &self,
        ids: Vec<Id>,
        fetcher: F,
    ) -> Result<BTreeMap<K, T>, Error>
    where
        T: Cacheable,
        Id: Into<GlobalId> + Clone,
        K: From<GlobalId> + Ord,
        F: FnOnce(Vec<Id>) -> Fut,
        Fut: std::future::Future<Output = Result<BTreeMap<K, T>, Error>>,
    {
        // Convert IDs to GlobalIds
        let global_ids: Vec<GlobalId> = ids.iter().cloned().map(Into::into).collect();

        // 1. Lock-free read phase
        let (cached, missing_global_ids) = self.get_many::<T, K>(&global_ids)?;

        if missing_global_ids.is_empty() {
            return Ok(cached);
        }

        // 2. Reconstruct original IDs for fetcher
        let missing_ids = self.reconstruct_ids(&missing_global_ids, &ids)?;

        // 3. Fetch missing entities
        let fetched = fetcher(missing_ids).await?;

        // 4. Stage for deferred write
        self.stage_many(&fetched)?;

        // 5. Merge and return
        let mut all = cached;
        all.extend(fetched);
        Ok(all)
    }

    /// Helper to map GlobalIds back to original ID type
    fn reconstruct_ids<Id>(
        &self,
        global_ids: &[GlobalId],
        original_ids: &[Id],
    ) -> Result<Vec<Id>, Error>
    where
        Id: Into<GlobalId> + Clone,
    {
        let original_map: HashMap<String, Id> = original_ids
            .iter()
            .map(|id| {
                let gid: GlobalId = id.clone().into();
                (gid.to_key(), id.clone())
            })
            .collect();

        global_ids
            .iter()
            .map(|gid| {
                original_map
                    .get(&gid.to_key())
                    .cloned()
                    .ok_or_else(|| Error::InvalidInput("ID mapping failed".to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wikidata::WikidataId;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
    struct TestEntity {
        id: String,
        value: String,
    }

    impl Cacheable for TestEntity {
        fn global_id(&self) -> GlobalId {
            let wid = WikidataId::qid(123);
            GlobalId::Wikidata(wid)
        }

        fn cache_key(&self) -> String {
            self.id.clone()
        }
    }

    #[test]
    fn test_cache_middleware_creation() {
        let temp_dir = TempDir::new().unwrap();
        let _cache = DiskCacheMiddleware::new(temp_dir.path()).unwrap();

        assert!(temp_dir.path().join("wikidata/Q").exists());
        assert!(temp_dir.path().join("wikidata/P").exists());
        assert!(temp_dir.path().join("osm/node").exists());
        assert!(temp_dir.path().join("osm/way").exists());
        assert!(temp_dir.path().join("osm/rel").exists());
    }

    #[test]
    fn test_stage_and_flush() {
        let temp_dir = TempDir::new().unwrap();
        let cache = DiskCacheMiddleware::new(temp_dir.path()).unwrap();

        let entity = TestEntity {
            id: "Q123".to_string(),
            value: "test".to_string(),
        };

        cache.stage(&entity).unwrap();
        cache.flush().unwrap();

        let gid = GlobalId::Wikidata(WikidataId::qid(123));
        let retrieved: Option<TestEntity> = cache.get(&gid).unwrap();
        assert_eq!(retrieved, Some(entity));
    }
}
