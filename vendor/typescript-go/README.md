# Vendored typescript-go diagnostics

This directory contains the minimal native diagnostic inputs needed to match
TypeScript 7.0.2 without a network checkout at generation time.

- Upstream: <https://github.com/microsoft/typescript-go>
- Tag: `typescript/v7.0.2`
- Commit: `2bd066d87f5bafd315be9f40889d0a60b9e58e0b`
- License: Apache-2.0 (see the upstream `LICENSE` and `NOTICE.txt`)

The files under `internal/diagnostics` are byte-for-byte copies from
that commit. `artifacts.json` records the compressed and uncompressed hashes,
sizes, locale destinations, and entry counts. The native extra diagnostic
catalog overlays the legacy
`TypeScript/src/compiler/diagnosticMessages.json` catalog by numeric code,
matching typescript-go's generator.

## Regenerating TSZ outputs

The sync is deterministic and offline once these inputs and the pinned
`TypeScript/` corpus are present:

```bash
node scripts/setup/sync-typescript-diagnostics.mjs --write
node scripts/setup/sync-typescript-diagnostics.mjs --check
```

`--write` expands the exact vendored locale bytes into
`crates/tsz-core/data/locales/` and regenerates the merged Rust diagnostic
catalog. `--check` verifies the pin metadata, every vendored hash, every
expanded locale, and every generated diagnostic file without writing.

When advancing the TypeScript pin, copy only
`internal/diagnostics/extraDiagnosticMessages.json` and
`internal/diagnostics/loc/*.json.gz` from the exact matching
typescript-go commit, then update `artifacts.json` before running the two
commands above.
