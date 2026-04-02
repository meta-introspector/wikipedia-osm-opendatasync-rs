use crate::{overpass::OSMId, wikidata::WikidataId};
use std::path::{Path, PathBuf};

/// Global identifier for all cacheable entities
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlobalId {
    Wikidata(WikidataId),
    OSM(OSMId),
    WikicommonsCategory(String),
    WikicommonsPageId(u64),
}

impl GlobalId {
    /// Convert to cache file path
    pub fn to_cache_path(&self, cache_dir: &Path) -> PathBuf {
        match self {
            GlobalId::Wikidata(id) => {
                let prefix = if id.to_string().starts_with('Q') {
                    "Q"
                } else {
                    "P"
                };
                cache_dir.join(format!("wikidata/{}/{}.json", prefix, id))
            }
            GlobalId::OSM(id) => match id {
                OSMId::Node(n) => cache_dir.join(format!("osm/node/{}.json", n)),
                OSMId::Way(w) => cache_dir.join(format!("osm/way/{}.json", w)),
                OSMId::Relation(r) => cache_dir.join(format!("osm/rel/{}.json", r)),
            },
            GlobalId::WikicommonsCategory(name) => {
                cache_dir.join(format!("wikicommons/category/{}.json", name))
            }
            GlobalId::WikicommonsPageId(id) => {
                cache_dir.join(format!("wikicommons/pageid/{}.json", id))
            }
        }
    }

    /// Convert to string key for HashMap
    pub fn to_key(&self) -> String {
        match self {
            GlobalId::Wikidata(id) => id.to_string(),
            GlobalId::OSM(id) => match id {
                OSMId::Node(n) => format!("node/{}", n),
                OSMId::Way(w) => format!("way/{}", w),
                OSMId::Relation(r) => format!("relation/{}", r),
            },
            GlobalId::WikicommonsCategory(name) => name.clone(),
            GlobalId::WikicommonsPageId(id) => id.to_string(),
        }
    }
}

impl From<WikidataId> for GlobalId {
    fn from(id: WikidataId) -> Self {
        GlobalId::Wikidata(id)
    }
}

impl From<GlobalId> for WikidataId {
    fn from(gid: GlobalId) -> Self {
        match gid {
            GlobalId::Wikidata(id) => id,
            _ => panic!("Cannot convert non-Wikidata GlobalId to WikidataId"),
        }
    }
}

impl From<OSMId> for GlobalId {
    fn from(id: OSMId) -> Self {
        GlobalId::OSM(id)
    }
}

impl From<GlobalId> for OSMId {
    fn from(gid: GlobalId) -> Self {
        match gid {
            GlobalId::OSM(id) => id,
            _ => panic!("Cannot convert non-OSM GlobalId to OSMId"),
        }
    }
}

impl From<GlobalId> for String {
    fn from(gid: GlobalId) -> Self {
        gid.to_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wikidata_q_path() {
        let id = WikidataId::qid(123);
        let gid = GlobalId::Wikidata(id);
        let path = gid.to_cache_path(&PathBuf::from("/cache"));
        assert_eq!(path, PathBuf::from("/cache/wikidata/Q/Q123.json"));
    }

    #[test]
    fn test_wikidata_p_path() {
        let id = WikidataId::pid(456);
        let gid = GlobalId::Wikidata(id);
        let path = gid.to_cache_path(&PathBuf::from("/cache"));
        assert_eq!(path, PathBuf::from("/cache/wikidata/P/P456.json"));
    }

    #[test]
    fn test_osm_paths() {
        let cache = PathBuf::from("/cache");

        let node = GlobalId::OSM(OSMId::Node(123));
        assert_eq!(
            node.to_cache_path(&cache),
            PathBuf::from("/cache/osm/node/123.json")
        );

        let way = GlobalId::OSM(OSMId::Way(456));
        assert_eq!(
            way.to_cache_path(&cache),
            PathBuf::from("/cache/osm/way/456.json")
        );

        let rel = GlobalId::OSM(OSMId::Relation(789));
        assert_eq!(
            rel.to_cache_path(&cache),
            PathBuf::from("/cache/osm/rel/789.json")
        );
    }

    #[test]
    fn test_to_key() {
        let q_id = GlobalId::Wikidata(WikidataId::qid(123));
        assert_eq!(q_id.to_key(), "Q123");

        let node = GlobalId::OSM(OSMId::Node(456));
        assert_eq!(node.to_key(), "node/456");
    }
}
