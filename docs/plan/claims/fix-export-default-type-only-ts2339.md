**2026-04-27 12:59:00**

# fix(checker,binder): block type-only namespace members from leaking into value export resolution

Status: claim
Branch: claude/exciting-keller-hIafr

Goal: fix fingerprint mismatch for `TS2339 e.ts:2:11 Property 'A' does not exist on type 'typeof import("b")'.` in `TypeScript/tests/cases/conformance/externalModules/typeOnly/exportDefault.ts`.

Root cause: when file `b.ts` does `import type * as types from './a'` and `export default types`, the class `A` from `./a` was leaking into `b.ts`'s `file_locals`. During cross-file module resolution, the `file_locals` fallback would find `A` and return it as a value member of `./b`, preventing TS2339 emission.

Three-layer fix:
1. **Binder** (`core.rs`): Add `!symbol.is_type_only` guard when merging namespace module exports into `file_exports`, so type-only namespace import members don't leak into module export tables.
2. **Checker type_only query** (`type_only.rs`): Add `!symbol.is_type_only` early return in `is_type_only_import_equals_namespace_expr`, so plain `import X = require('...')` is correctly treated as a value import-equals (emitting TS2339) rather than type-only (which would emit TS1361).
3. **Checker module resolution** (`module.rs`): Gate `file_locals` fallback in both `resolve_export_in_file` and `resolve_cross_file_export_from_file` on `module_exports` being empty. When `module_exports` is populated, it is the authoritative source — `file_locals` fallback should not supplement with additional symbols.

Test target:
- `TypeScript/tests/cases/conformance/externalModules/typeOnly/exportDefault.ts` (was fingerprint-only, now passes).

Scope: thin changes — binder export guard + checker early return + checker resolution fallback guard. Solver untouched.

Conformance impact: +1 test (exportDefault.ts flips from fingerprint-only to pass). No regressions in typeOnly or externalModules suites.
