# fix(binder): self namespace import must not expose local imported aliases

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-zQMuf`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance — module surface parity

## Intent

Fixes #3585. When a module imports itself with `import * as self from
"./self.mjs"`, tsz exposes the file's local imported aliases (default
imports and named imports from other modules) on the `self` namespace
type, so `self.default` and `self.imported` silently succeed where tsc
emits TS2339.

The leak is in the binder's `populate_module_exports_from_file_symbols`:
it adds every `file_locals` entry whose `is_exported` flag is set, but
that flag is currently set on local import aliases when an `export {
foo }` re-export specifier marks the aliased local as exported, and the
populate pass cannot distinguish a re-exported alias from a local
binding that just happens to be an alias to another module's surface.
The right fix is to only re-publish a `file_locals` entry into a
module's public `module_exports` when the local symbol is itself a
module-owned export (not a pure import alias). Pure import aliases are
already handled separately by the re-export machinery (`export { foo }
from "./other"`) which writes its own `module_exports` entries.

## Files Touched

- `crates/tsz-binder/src/state/core.rs` — narrow the
  `populate_module_exports_from_file_symbols` predicate so import
  aliases are only published when they are explicitly re-exported.
- `crates/tsz-checker/tests/self_namespace_import_alias_leak_tests.rs`
  — new unit test covering the default-import and named-import
  reproductions from the issue.

## Verification

- `cargo nextest run -p tsz-checker --test self_namespace_import_alias_leak_tests`
- `cargo nextest run -p tsz-checker --lib`
- targeted conformance runs touching namespace imports.
