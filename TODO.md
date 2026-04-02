# TODO: Reusable Forgejo Action

## Future Enhancement: action.yml

Similar to [rrule-calc's reusable action](https://codeberg.org/houston-open-source-society/rrule-calc/src/branch/main/action.yml), opendatasync could provide a reusable Forgejo/GitHub action for batch processing with built-in caching.

### Proposed Features

- **Inputs**:
  - `batch-file` (required): Path to the batch TOML configuration file
  - `output-dir` (required): Directory where output JSON files will be written
  - `verbose` (optional): Enable verbose output for debugging
  - `version` (optional): Podman image version tag (default: `latest`)

- **Outputs**:
  - `cache-hit`: Boolean indicating whether cached output was used

- **Caching Strategy**:
  - Cache key based on batch file content hash
  - Cache output directory files
  - Skip execution when cache hit, significantly speeding up CI pipelines

### Example Usage

```yaml
- name: Sync open data
  uses: https://codeberg.org/houston-open-source-society/opendatasync@main
  with:
    batch-file: ./run/opendatasync/batch.toml
    output-dir: ./data
    verbose: true
```

### Benefits

1. **Simplified CI integration**: One-line action usage instead of manual podman commands
2. **Automatic caching**: Built-in cache management reduces API load and build time
3. **Version pinning**: Users can specify exact version for reproducibility
4. **Consistent interface**: Matches rrule-calc's action pattern

### Implementation Notes

- Use `docker://codeberg.org/houston-open-source-society/opendatasync:${{ inputs.version }}`
- Cache all files in output directory
- Consider partial cache support for individual data source files (overpass.json, wikidata-*.json, etc.)

### Priority

**Medium** - Current podman usage works well, but action would improve developer experience for CI/CD users.

See [rrule-calc/action.yml](https://codeberg.org/houston-open-source-society/rrule-calc/src/branch/main/action.yml) for reference implementation.
