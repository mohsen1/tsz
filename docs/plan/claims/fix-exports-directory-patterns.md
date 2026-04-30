# Fix: Directory export patterns in package.json exports field

**Status**: claim
**Branch**: fix/exports-specifier-generation-directory
**Diagnostic codes**: TS2307 (was extra; now correct)

## Scope

- `match_export_pattern` in `crates/tsz-core/src/resolution/helpers.rs`: Handle directory export
  patterns (keys ending with `/`, e.g. `"./"` in `"exports": { "./": "./" }`) by matching
  any subpath starting with that prefix.
- `substitute_wildcard_in_exports` and `apply_wildcard_substitution`: When the target has
  no `*` wildcard, append the matched portion (Node.js directory export semantics —
  `"./": "./"` with subpath `./index.js` resolves to `./index.js`).
- `try_export_target` in `crates/tsz-core/src/module_resolver/file_probing.rs`: In
  Node16/NodeNext mode, skip extension probing for extensionless export targets.
  Node.js resolves export targets literally, so `./other` must exist as-is — tsz
  must not add `.d.ts`/.ts and silently resolve `import "pkg/other"` when the
  specifier lacks an explicit JS extension.

## Root cause

`match_export_pattern` only handled wildcard patterns (`"./*"`) but not directory
patterns (`"./"`). `substitute_wildcard_in_exports` only replaced `*` with the
matched wildcard, so for directory targets like `"./"` with matched `"index.js"`,
the target remained `"./"` instead of becoming `"./index.js"`.

Additionally, `try_export_target` probed for extensions on extensionless export
targets, which contradicts tsc's ESM extension requirement in Node16/NodeNext.
This caused `import "pkg/other"` (no extension) to silently resolve to
`pkg/other.d.ts`, when tsc correctly emits TS2307.

## Verification

- Unit tests for `match_export_pattern`, `substitute_wildcard_in_exports`,
  `apply_wildcard_substitution` directory target behavior
- Conformance test `nodeModulesExportsSpecifierGenerationDirectory.ts` now passes
- No regressions in 154 existing module_resolver unit tests
