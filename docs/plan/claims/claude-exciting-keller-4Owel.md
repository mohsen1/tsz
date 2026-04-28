# fix(checker): use full node_modules path for virtual-FS root imports in error messages

- **Date**: 2026-04-28
- **Branch**: `claude/exciting-keller-4Owel`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — fingerprint parity)

## Intent

`esmNoSynthesizedDefault.ts` is a fingerprint-only failure: tsz emits TS1192 and
TS2339 with correct codes but the wrong module-name in the message. tsz shows
`"mdast-util-to-string"` (bare package name) while tsc shows
`"node_modules/mdast-util-to-string/index"` (full virtual-FS-relative path).

Root cause: `imported_namespace_display_module_name` resolves the bare specifier
to the real disk path (e.g. `/tmp/test123/node_modules/pkg/index.d.ts`) and passes
it to `trim_namespace_display_path`. Because `node_modules` appears at depth 3+ in
that absolute path, the function strips everything down to the bare package name.
TSC uses a virtual FS where the file is literally at `/node_modules/...` (depth 1),
so it preserves the full root-relative path.

Fix: before calling `trim_namespace_display_path`, compute the path relative to the
source file's directory. When `node_modules/` is a direct sibling of the source
file (virtual-FS-root layout), the relative path starts with `node_modules/` and
`trim_namespace_display_path` preserves it verbatim (`node_modules_idx == 0`). For
deeper project layouts (e.g. `src/app.ts` importing from `../node_modules/`), the
relative form contains `..` and the absolute path is used instead, which
`trim_namespace_display_path` strips to the bare package name.

`trim_namespace_display_path` was also extracted to module level and 7 unit tests
were added covering root, scoped, deep, virtual-root-prefix, relative, and
non-node_modules paths.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers_binding.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "esmNoSynthesizedDefault" --verbose`
- `cargo nextest run -p tsz-checker`
