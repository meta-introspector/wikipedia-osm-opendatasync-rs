# opendatasync

Pull down data from Wikidata, OpenStreetMap, Wikimedia Commons — plus eRDFa seal/witness and zkperf verification.

## Install

```bash
cargo build --release
```

## Usage

### Wikidata entities

```bash
opendatasync wikidata wbgetentities -q Q333871 -q Q1757006
# Q333871 = Monster group, Q1757006 = Monstrous moonshine
```

### Wikicommons categories

```bash
opendatasync wikicommons categorymembers --category "Category:Monster group"
```

### OpenStreetMap via Overpass

```bash
opendatasync overpass --ways 56065095
```

### Batch mode (all sources in one TOML)

```bash
opendatasync --cache-dir ./cache --output-dir ./data batch --batch-file batch.toml
```

Example `batch.toml`:

```toml
[[wikidata]]
qids = ["Q333871", "Q1757006", "Q83906", "Q41390"]
# Monster group, Monstrous moonshine, Ramanujan, Gödel
resolve_headers = "all"
resolve_data = "all"

[[wikicommons]]
[wikicommons.categorymembers]
categories = ["Category:Monster group"]

[[overpass]]
ways = [56065095]

[[erdfa]]
urls_from_file = "urls.txt"
max_depth = 2

[[zkperf]]
origin = [35, 35, 35]
```

### eRDFa: Fetch + Seal URLs

The `[[erdfa]]` batch section fetches URLs, computes SHA-256 witness + Monster Hash orbifold coordinates, and writes sealed files:

```bash
# urls.txt format: KEY=URL or bare URL, one per line
S1_moonshine=https://arxiv.org/abs/1807.00723
S3_lisi_e8=https://arxiv.org/abs/0711.0770
https://en.wikipedia.org/wiki/Monstrous_moonshine
```

Output:
- `raw/<key>.txt` — fetched content with seal header
- `seal_manifest.jsonl` — one JSON line per seal:

```json
{"key":"S1_moonshine","url":"https://arxiv.org/abs/1807.00723","witness":"9ce94a5f...","dasl":"0xda51d084...","orbifold":[26,21,42],"size":46996}
```

### zkperf: Verify seals

The `[[zkperf]]` batch section verifies seals and computes orbifold distance to an origin point:

```json
{"seal_witness":"9ce94a5f...","verified":true,"orbifold_distance":34.2,"origin":[35,35,35]}
```

## Architecture

```
opendatasync batch --batch-file batch.toml
  ├── [[wikidata]]     → wikidata-wbgetentities.json
  ├── [[wikicommons]]  → wikicommons-categorymembers.json
  ├── [[overpass]]     → overpass.json
  ├── [[erdfa]]        → raw/*.txt + seal_manifest.jsonl
  └── [[zkperf]]       → zkperf_witnesses.jsonl
```

All outputs accumulate in `--output-dir`. Cache in `--cache-dir` avoids re-fetching.

## Origin

Forked from [houston-open-source-society/opendatasync](https://codeberg.org/houston-open-source-society/opendatasync). eRDFa and zkperf modules added by [meta-introspector](https://github.com/meta-introspector).

## License

ACSL-1.4-or-newer (see LICENSE.md)
