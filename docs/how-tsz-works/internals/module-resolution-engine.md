# The Module-Resolution Engine and Re-Export Validation

This deep-dive fills a gap the boundary-level docs leave open: how a raw
specifier string like `"./utils"`, `"lodash"`, `"#core/opts"`, or
`"react/jsx-runtime"` is turned into an absolute file path, and how the checker
validates the *members* a `export ... from` statement forwards. The sibling
[Declarations: Imports, Exports, Namespaces, Modules, and Ambient](checker-declarations-modules.md)
covers the checker's *module boundary* — `TS2307`, `export =` legality,
`verbatimModuleSyntax`, namespace merging — but it deliberately stops at
"specifier → file index" and says that step is "owned by the
driver/`ModuleResolver`". This document is that owner. It traces the resolver
kernel in `crates/tsz-core/src/module_resolver`, the filesystem-probing and
package-metadata helpers it leans on, and the narrow slice of checker code that
validates re-exported members (`validate_reexported_members`, `TS2305`/`TS2724`)
and import-attribute grammar (`TS2821`–`TS2880`, `TS1453`–`TS1464`, `TS1543`).

The resolver is a `tsz-core` component, not part of the type-checking kernel. It
does no type computation: it answers a path question with a path answer plus a
diagnostic-code selection. The driver (CLI / LSP / WASM) calls it to build the
file graph; the checker consumes the precomputed resolution maps. Where this doc
touches checker code, it stays on the *module-boundary validation* that the
resolver makes possible — never on relation/inference/evaluation, which the
[solver docs](solver-relations.md) own. For how the resolver result is folded
into program construction and watch re-resolution, see
[Driver: Incremental and Watch](driver-incremental-and-watch.md) and
[Driver: Project References and Build Mode](driver-project-references-and-build-mode.md);
for the LSP's per-keystroke re-resolution, see
[LSP and WASM Surfaces](lsp-and-wasm-surfaces.md).

---

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Specifier classification (relative / rooted / bare / `#imports` / `node:`) | Type computation of any kind — that is the solver |
| `node_modules` walk-up, `@types` redirection, type-roots | The program file graph / BFS ordering (that is the driver) |
| tsconfig `paths`/`baseUrl` substitution and single-pattern selection | Reading the printer's rendered output as a predicate |
| `package.json` `exports`/`imports` conditional + pattern resolution | Deciding *whether a member exists* in a module surface (checker binder tables) |
| Extension priority (`.mts` vs `.cts` vs `.ts`) per package type | Emit transforms or output rewriting (that is the emitter) |
| Module-not-found **diagnostic code selection** (`TS2307`/`TS2792`/`TS2834`/`TS2835`/`TS5097`/`TS2732`/`TS6263`/`TS7016`) | Attaching diagnostics to AST nodes (the checker does that) |
| Filesystem existence memoization (`FILE_EXISTS`/`DIR_EXISTS`, per-package caches) | Persisting state across compilations without an explicit reset |

The re-export and import-attribute validation lives on the **checker** side
(`crates/tsz-checker/src/declarations`). It *consumes* the resolver's
file-index maps and emits member-level diagnostics; it asks the binder for the
module's export surface and the solver for attribute-value assignability.

---

## Module map

### Resolver kernel — `crates/tsz-core/src/module_resolver`

| File | Role |
| --- | --- |
| `mod.rs` | `ModuleResolver` struct, caches, `resolve_with_kind*`, `resolve_uncached` (the step ladder), and `lookup` (the driver-facing entry that owns diagnostic-code selection) |
| `request_types.rs` | Public boundary types: `ModuleLookupRequest`, `ModuleLookupResult`, `ModuleLookupOutcome`, `ResolvedModule`, `ModuleExtension`, `ImportKind`, `ImportingModuleKind`, `PackageType`; `is_path_relative` / `is_external_module_name_relative` |
| `relative_resolution.rs` | `resolve_relative` / `resolve_absolute`; `rootDirs` virtual remap; `TS2834`/`TS2835` extension-needed logic |
| `path_mapping.rs` | `try_path_mappings`: select exactly one tsconfig `paths` pattern, probe its targets |
| `node_modules_resolution.rs` | `resolve_bare_specifier` (the `node_modules` walk-up), `resolve_classic_non_relative`, `resolve_package` (entry-point resolution), `should_stop_on_exports_failure`, `should_skip_fallback_on_not_found` |
| `exports_imports.rs` | `package.json#exports`/`imports` resolution: conditional matching, wildcard substitution, `get_export_conditions`, `resolve_package_imports`, `resolve_types_versions` |
| `package_json.rs` | `read_package_json` (cached parse), `get_package_type_for_dir`, `target_package_type_from_json`, `importer_package_type` |
| `file_probing.rs` | `try_file` / `try_file_no_index` / `try_directory` / `try_file_or_directory` / `try_export_target` / `try_types_entry`; `extension_candidates_for_package_type` |
| `self_reference.rs` | `try_self_reference_v2`: a package importing itself by name through its own `exports` (`TS2209` ambiguous-root case) |
| `diagnostics.rs` | `ResolutionFailure` enum, code constants, `to_diagnostic`, `should_try_fallback` |

### Shared resolution helpers — `crates/tsz-core/src/resolution/helpers.rs`

Re-exported into the resolver as `module_resolver_helpers`. Owns the
thread-local `FILE_EXISTS`/`DIR_EXISTS` caches (`cached_is_file`/`cached_is_dir`),
`normalize_path_segments`, `parse_package_specifier`, `types_package_name`, the
`PackageJson`/`PackageExports` serde structs, the extension-candidate constant
tables, `node16_extension_substitution`, and the Node.js `PATTERN_KEY_COMPARE`
specificity comparator (`export_pattern_specificity` /
`find_best_export_pattern`).

### Checker re-export / import-attribute validation — `crates/tsz-checker/src/declarations`

| File | Role |
| --- | --- |
| `module_checker.rs` | `check_export_module_specifier`, `validate_reexported_members` (`TS2305`/`TS2614`/`TS2724`), `check_export_target_is_module` (`TS2306` file-is-not-a-module), cycle detection |
| `import/declaration_attributes.rs` | `check_import_attributes_grammar` (`TS2821`/`TS2823`/`TS2836`/`TS2856`/`TS2822`/`TS2857`/`TS2880`), `check_import_attributes_assignability` (`TS2322`/`TS2858`), `check_type_only_resolution_mode_attribute_grammar` (`TS1453`–`TS1464`), deferred-import restrictions (`TS18058`/`TS18059`), JSON ESM attribute (`TS1543`) |
| `import/declaration_check_body.rs` | `check_module_specifier_ts_extension` (`TS2846`/`TS5097`), `TS2876`/`TS2877` rewrite-extension checks |
| `import/core/helpers.rs` | `get_resolution_mode_override`, `resolution_mode_override_is_effective`, `requested_resolution_mode`, `module_not_found_diagnostic` |

---

## The two entry points: `resolve_with_kind` and `lookup`

The resolver has two public surfaces, and the distinction matters:

```text
                 +-------------------------------+
specifier  --->  | resolve_with_kind*            |  pure path resolution
                 |   -> resolve_uncached         |  (Result<ResolvedModule,
                 |        (the 6-step ladder)     |   ResolutionFailure>)
                 +-------------------------------+
                                |
                 +-------------------------------+
                 | lookup(request, fallback,     |  driver-facing: owns ALL
                 |        is_ambient_module, ...) |  diagnostic-code selection,
                 |   -> ModuleLookupResult        |  ambient/untyped-JS handling,
                 +-------------------------------+  node: retry, fallback bridge
```

`resolve_with_kind_and_module_kind` (see `mod.rs`) returns a
`Result<ResolvedModule, ResolutionFailure>`. It is the cache-fronted core: it
memoizes on `ResolutionCacheKey = (PathBuf, String, ImportingModuleKind,
ImportKind)` — the containing **directory** (not the full file path), the
specifier, the importer's ESM/CJS classification, and the import syntax kind. A
hit returns a `clone()` immediately.

`lookup` (also `mod.rs`) is the entry the driver calls. It wraps
`resolve_with_kind_and_module_kind` and adds everything that depends on
*program* state rather than just the filesystem: ambient-module matching
(via the `is_ambient_module` closure), a `fallback_resolve` closure (the
driver's legacy resolution path for virtual test files), `node:`-builtin
scheme stripping, untyped-JS probing, and the final mapping of a
`ResolutionFailure` to a `ModuleLookupResult` carrying the right `u32`
diagnostic code. The CLI driver calls it in
`crates/tsz-cli/src/driver/source_resolution_setup.rs` and `.../sources.rs`,
then `.classify()` collapses the result into a `ModuleLookupOutcome`
(`resolved_path`, `is_resolved`, `error`).

The hard invariant, stated at the top of `mod.rs`: *module existence truth comes
from `resolve_with_kind` outcomes, and not-found code selection
(`TS2307`/`TS2792`/`TS2834`/`TS2835`/`TS5097`/`TS2732`) is owned here and
propagated to the checker via resolution records.* The checker never recomputes
a not-found code from partial state.

---

## The resolution ladder: `resolve_uncached`

`resolve_uncached` is the heart. It tries six steps in order and returns
`(Result<ResolvedModule, ResolutionFailure>, path_mapping_attempted)`:

```text
specifier
  |
  | Step 1: starts with '#'  ----------------> resolve_package_imports
  |          (invalid '#' / '#/' -> NotFound;  (package.json#imports field)
  |           !resolve_package_json_imports -> NotFound)
  |
  | Step 2: paths non-empty && paths_base set && !is_external_module_name_relative
  |          ----------------> try_path_mappings  (sets path_mapping_attempted)
  |          a matched pattern's hit returns immediately
  |
  | Step 3: is_path_relative(specifier)  -----> resolve_relative
  |
  | Step 4: starts with '/'  -----------------> resolve_absolute
  |
  | Step 5: !path_mapping_attempted && base_url set
  |          ----------------> base_url.join(specifier) probed via try_file_or_directory
  |
  | Step 6: Classic mode  --------------------> resolve_classic_non_relative
  |         else          --------------------> resolve_bare_specifier (node_modules)
  v
(result, path_mapping_attempted)
```

Two parity-load-bearing gates live in this ladder:

- **Step 2's `!is_external_module_name_relative` guard.** A catch-all `"*"`
  mapping in tsconfig `paths` must not intercept `./sibling` imports. The
  classifier `is_external_module_name_relative` (in `request_types.rs`) returns
  `true` for `./`, `../`, `.`, `..`, and rooted `/…`; only names where it
  returns `false` consult `paths`/`baseUrl`. This matches tsc's
  `isExternalModuleNameRelative`. Note `is_path_relative` is stricter than "starts
  with `.`" — `.prisma/client` starts with `.` but is a *bare* name, mirroring
  tsc's `pathIsRelative` regex `/^\.\.?(?:$|[\\/])/`.

- **Step 5's `!path_mapping_attempted` guard.** tsc reaches the bare `baseUrl`
  join only when *no* `paths` pattern matched. When a pattern matched but its
  on-disk targets were missing, tsc commits to that pattern and skips `baseUrl`,
  continuing only to `node_modules`. After Step 6, a `NotFound` that came after
  a path-mapping attempt is rewritten to `PathMappingFailed` (still `TS2307`
  in message, but it records that a mapping was the failed route).

After `resolve_uncached`, `resolve_with_kind_and_module_kind` applies three
post-checks before caching:

1. **`TS5097` upgrade.** If a specifier carries an explicit TS extension
   (`.ts`/`.tsx`/`.mts`/`.cts`, via `explicit_ts_extension`), the flags
   `allowImportingTsExtensions`/`allowArbitraryExtensions`/`rewriteRelativeImportExtensions`
   are all off, no path mapping was attempted, and the result is `NotFound`,
   the failure becomes `ImportingTsExtensionNotAllowed`.
2. **`TS6142` JSX-not-enabled.** A successful `.tsx`/`.jsx` resolution with
   `jsx: None` becomes `JsxNotEnabled` (keeping the resolved path so the
   checker can still type it).
3. **`TS2732` JSON-without-resolveJsonModule.** A `.json` resolution with
   `resolveJsonModule` off becomes `JsonModuleWithoutResolveJsonModule`.

---

## ESM vs CJS classification: the one rule everyone must agree on

`importing_module_kind_for_import` (`mod.rs`) is the single owner of the
ESM/CJS decision for an import *site*. Divergence here silently resolves the
same import two different ways — a different conditional `exports` branch and a
different Node16 extension priority. The precedence:

1. A driver-supplied `resolution_mode_override` (from a `with { "resolution-mode": ... }`
   import attribute) wins over everything.
2. `import()` (`ImportKind::DynamicImport`) is always ESM; `require(...)`
   (`CjsRequire`) is always CJS — independent of the file's extension or
   package type.
3. Under `module: preserve` (tsc's bundler-style mode), an ordinary
   `import`/`export ... from` is ESM unless the file extension forces CJS
   (`.cts`/`.cjs`).
4. Otherwise `get_importing_module_kind` decides from extension
   (`.mts`/`.mjs` force ESM, `.cts`/`.cjs` force CJS), the `module` target,
   and the nearest `package.json#type` (via `get_package_type_for_dir`).

`ImportingModuleKind::as_condition_str` maps `Esm -> "import"` and
`CommonJs -> "require"` — the condition string used in `get_export_conditions`.

---

## Walk-through 1: a relative `.ts` import under Node16

Source: `import { f } from "./util"` in `/app/src/main.mts`, `module: nodenext`.

1. `lookup` builds the request; `import_kind = EsmImport`,
   `resolution_mode_override = None`. `.mts` forces ESM, so the importer kind is
   `Esm`.
2. `resolve_uncached`: Step 3 fires (`is_path_relative("./util")`).
3. `resolve_relative` (`relative_resolution.rs`) joins `candidate = /app/src/util`.
   `specifier_has_extension` is `false`.
4. The `needs_extension_check` gate: Node16/NodeNext + extensionless +
   containing file is not JS + the import is ESM-in-ESM → `true`. So even though
   `/app/src/util.ts` exists on disk, the resolver tries to resolve it *only to
   determine the suggestion*, then returns an error. `try_resolve_candidate`
   finds `util.ts` (not via directory index), so `resolved_via_index` is `false`.
5. `suggested_runtime_extension(Ts)` returns `.js`. The result is
   `ImportPathNeedsExtension { suggested_extension: ".js" }` →
   `to_diagnostic` produces **TS2835**: "…Did you mean `'./util.js'`?".

If `util` resolved only through `util/index.ts` (a directory index), the
suggestion would be empty and the code would be **TS2834** instead, because
appending `.js` to `./util` would not name the index file. Bare-dot specifiers
(`.`, `./`, `..`, `../`) that resolve via index are special-cased to emit plain
`TS2307` — there is no filename to attach an extension to.

The `package_type` for the probe is computed *per candidate*: when the import
uses require-resolution (CJS import in an ESM-syntax statement, or `require`),
Node16/NodeNext walks up to the candidate's own `package.json#type` via
`get_package_type_for_dir`; otherwise it inherits the importer's
`importer_package_type`.

---

## Walk-through 2: a bare specifier through `node_modules`

Source: `import _ from "lodash"` from `/app/src/main.ts`, `module: nodenext`.

1. Step 6: `resolve_bare_specifier` (`node_modules_resolution.rs`).
   `parse_package_specifier("lodash")` → `("lodash", None)`.
   `get_export_conditions(Esm)` yields, in order:
   `["types", "node", "import", "default"]` (custom conditions prepend; tsc
   always checks `types` first; `node` is added only for Node16/NodeNext;
   `bundler` mode does **not** default to `browser`).
2. `try_self_reference_v2` first (Walk-through 4) → `NotSelfReference`, fall
   through.
3. Walk up the directory tree. At each level, `node_modules.is_dir()` is read
   through the per-ancestor `node_modules_dir_cache` (a syscall sibling files
   would otherwise repeat). For each discovered `node_modules` root, probe
   `node_modules/lodash`.
4. `cached_is_dir` hits → `resolve_package(/app/node_modules/lodash, None, ...)`.
5. `resolve_package` reads `package.json` (cached). With no subpath it tries, in
   order: `exports` `"."` entry → `typesVersions` for `index` → `types`/`typings`
   field → `main` field (with `.js`→`.ts`/`.d.ts` substitution and
   declaration-sidecar probing) → `index.{ts,tsx,d.ts}` fallback.
6. lodash ships only `lodash.js`; `@types/lodash` ships the declarations. So the
   runtime `.js` entry is **deferred** into `js_fallback`, and the walk keeps
   going. `types_package_name("lodash")` = `@types/lodash`. Its
   `cached_is_dir` hits, `resolve_package` resolves its `index.d.ts`, and that
   **typed** resolution returns immediately — winning over the deferred JS entry.

This deferral is the structural rule: a JS-only entry never short-circuits the
search; a matching `@types/...` package (or a `.ts`/`.d.ts` runtime entry, or a
type-root) always wins. Only after every `@types` and type-root probe fails does
`js_fallback` get returned (binding as `any`, surfacing `TS7016` at the call
site). `types_package_name` handles scoping: `@storybook/react` →
`@types/storybook__react` (the `@` is dropped and `/` becomes `__`).

---

## Walk-through 3: `exports` subpath with conditions and wildcards

Source: `import { jsx } from "react/jsx-runtime"`, `module: nodenext`.

1. `parse_package_specifier` → `("react", Some("jsx-runtime"))`.
2. `resolve_package(react_dir, Some("jsx-runtime"), ...)`. `subpath_key =
   "./jsx-runtime"`.
3. `resolve_package_exports_with_conditions` (`exports_imports.rs`) walks the
   `exports` map. For a `Map` variant it first tries an **exact** key match
   (keys containing `*` are excluded), then falls to pattern matching via
   `find_best_export_pattern`.
4. The matched value is typically a `Conditional` map (`{ "types": "...",
   "import": "...", "require": "...", "default": "..." }`). `condition_key_matches`
   iterates the **JSON key order** of the conditional (not our condition
   priority order) and returns the first key present in `conditions`. So `types`
   wins, pointing at a `.d.ts`.
5. The string target goes through `package_relative_target_path` (which rejects
   `..` escapes and `node_modules` segments per Node's `PACKAGE_TARGET_RESOLVE`)
   then `try_export_target` (runtime) or `try_types_entry` (declaration-aware,
   used when a types-flavored condition matched).

**Pattern specificity.** When two `exports` keys both match, the winner is
chosen by `export_pattern_specificity` (`helpers.rs`), a faithful port of
Node's `PATTERN_KEY_COMPARE`: `(base_length, is_pattern, total_length)`,
"larger wins". A directory key `"./lib/"` (base 6) outranks `"./*"` (base 3); a
wildcard `"./lib/*"` (base 7) outranks the directory; at equal base length a
wildcard beats an exact/directory key; ties break by total length. True ties
resolve to the first key in **`IndexMap` insertion order** (JSON source order),
which is why `PackageExports::Map` and `imports` use `IndexMap`. Wildcard
substitution (`apply_wildcard_substitution` / `substitute_wildcard_in_exports`)
replaces the captured `*` text into the target *before* probing, and
distinguishes `*`-pattern keys (replace `*` only) from `/`-suffix directory keys
(append the wildcard to a `/`-ending target).

**`exports` is authoritative.** Under Node16/NodeNext/Bundler, once a package
has an `exports` field, a subpath the map does not expose is *unresolved* —
`resolve_package` returns `NotFound` rather than falling back to
`typesVersions` or file/directory probing (mirroring tsc's
`loadModuleFromExports` returning unconditionally). `should_stop_on_exports_failure`
encodes this so the `node_modules` walk does not keep climbing to a parent
`node_modules` after an authoritative miss.

**`resolved_using_ts_extension`.** `key_ends_with_ts_extension` records whether
the *matched key* literally ends in a TS extension (e.g. the author wrote
`"./*.ts": "./*.js"`). This is propagated all the way out to `ResolvedModule`
and feeds the checker's `TS2877` rewrite-extension gate — wildcard substitutions
that merely *capture* a `.ts` do **not** count.

---

## Walk-through 4: self-reference and `#imports`

`try_self_reference_v2` (`self_reference.rs`) handles a package importing itself
by its own name. Only Node16/NodeNext/Bundler support it. It walks up to the
nearest `package.json`, and only if `package_json.name == package_name` and an
`exports` field is present does it resolve the subpath through
`resolve_package_exports_with_conditions`. If the name matches and `exports`
resolves, it returns `Resolved`. If `exports` is present but resolves nothing
and neither `rootDir` nor `outDir` is set, it returns `AmbiguousRoot` — surfaced
as **TS2209** ("The project root is ambiguous, but is required to resolve export
map entry…"). A name match with no `exports` is `NotSelfReference` (fall through
to `node_modules`).

`#`-prefixed `imports` are handled by `resolve_package_imports`
(`exports_imports.rs`). Per Node's `LOOKUP_PACKAGE_SCOPE`, a `#` specifier
resolves against the *single nearest enclosing* `package.json` (the importer's
own scope) — the walk-up only *finds* that scope; once a readable
`package.json` is found, `#imports` resolve against it alone and never fall
through to an ancestor package, even when the nearest scope has no matching key.
Targets starting with `./` are validated by `is_valid_relative_package_target`
(no `..`, no `node_modules` segment) and probed via `try_export_target` (or
`try_types_entry` for a types condition); bare targets
(`is_valid_bare_imports_target`) recurse through `resolve_bare_specifier`,
supporting self-referencing imports like `"#type": "some-package"`. Invalid
specifiers (`"#"` or `"#/..."`) short-circuit to `NotFound` via
`is_invalid_package_import_specifier`.

---

## File probing: the bottom of the stack

Every concrete on-disk attempt routes through `file_probing.rs`. The key axis is
**extension priority**, which depends on the (target) package type:

`extension_candidates_for_package_type` returns one of the constant tables in
`helpers.rs`:

| Mode / package type | Candidate order |
| --- | --- |
| Node16/NodeNext, `type: module` | `mts, d.mts, ts, tsx, d.ts, cts, d.cts` |
| Node16/NodeNext, `type: commonjs` | `cts, d.cts, ts, tsx, d.ts, mts, d.mts` |
| Node16/NodeNext, no package context (`None`) | `TS_EXTENSION_CANDIDATES` (`ts, tsx, d.ts, cts, d.cts, mts, d.mts`) |
| Classic | `CLASSIC_EXTENSION_CANDIDATES` (same as TS-only) |
| Other (Node10, Bundler) | `TS_EXTENSION_CANDIDATES` |

With `allowJs`, the `*_ALLOWJS_*` tables append `.js`/`.jsx`/`.mjs`/`.cjs` in
the matching order. The structural rule (called out in `resolve_package`): file
probing inside a target package walks the **target's** extension priority, not
the importer's — a `main` resolution for a `type: module` package tries
`.mts`/`.d.mts` first regardless of the importer's CJS-ness, and vice versa.

`try_file_inner` is the shared body of `try_file` and `try_file_no_index`. It:
normalizes `.`/`..` first (so alias branches return identical `PathBuf`s),
probes arbitrary-extension declarations (`.d.<ext>.ts`), applies
`node16_extension_substitution` (`.js`→`.ts`/`.tsx`/`.d.ts`, `.mjs`→`.mts`, etc.
— applied in *all* modes, since tsc maps `.js` imports to `.ts` sources
everywhere), applies the `rewriteRelativeImportExtensions` `.ts`→`.d.ts` remap,
falls back to the literal extension, then (if `try_index`) probes
`path/index.{ext}`. `try_file_no_index` omits the trailing index probe for ESM
packages under Node16/NodeNext, where Node.js forbids directory-index resolution.

`try_export_target` is deliberately *stricter* than `try_file`: in
Node16/NodeNext/Bundler an **extensionless** runtime `exports`/`imports` target
gets **no** extension addition and **no** index lookup — Node's
`PACKAGE_TARGET_RESOLVE` returns the target verbatim. Only an explicit
`.js`/`.mjs`/`.cjs` extension is remapped to its `.ts` sibling. The legacy
`Node` (node10) and `Classic` modes predate the spec'd algorithm and *do* apply
classic file/directory probing to an extensionless `exports` target, so they
fall through. `try_types_entry` is declaration-aware: it resolves explicit TS
extensions exactly and refuses to invent an extension where tsc would not.

---

## Caches and invariants

The resolver carries five owned caches plus two thread-local existence caches.
Their lifetimes and reset rules are the parity-critical part.

| Cache | Location | Key → Value | Invalidation |
| --- | --- | --- | --- |
| `resolution_cache` | `ModuleResolver` (owned `FxHashMap`) | `(containing_dir, specifier, ImportingModuleKind, ImportKind)` → `Result<ResolvedModule, ResolutionFailure>` | `clear_cache` |
| `package_json_cache` | `RefCell<FxHashMap>` | canonical path → `Result<PackageJson, String>` (Ok **and** Err cached) | `clear_cache` |
| `package_type_cache` | `RefCell<FxHashMap>` | dir → `Option<PackageType>` (walk-up memoized, all visited dirs filled) | `clear_cache` |
| `skip_fallback_cache` | `RefCell<FxHashMap>` | `(containing_dir, specifier, ImportingModuleKind)` → `bool` | `clear_cache` |
| `node_modules_dir_cache` | `RefCell<FxHashMap>` | dir → `bool` (is there a `node_modules` here?) | `clear_cache` |
| `FILE_EXISTS` | thread-local in `helpers.rs` | path → `bool` | `clear_path_existence_caches` / `reset_path_existence_caches` |
| `DIR_EXISTS` | thread-local in `helpers.rs` | path → `bool` | same |

Several caches use `RefCell` rather than `&mut self` so the cold-path probing in
`file_probing` / `exports_imports` / `self_reference` can populate them through
`&self` without cascading mutability. The same `node_modules/foo/package.json`
was previously read+parsed once per *role* (package type, exports, main, types,
self-reference) — five-plus identical disk+`serde_json` cycles per package. The
`package_json_cache` collapses those to one parse; caching the `Err` arm means
missing/invalid files are not re-stat'd either.

**The existence caches are thread-local, not on the resolver.** This is the
subtle one. `FILE_EXISTS`/`DIR_EXISTS` memoize `is_file`/`is_dir` for the
lifetime of a compilation (the filesystem is assumed stable, mirroring tsc's
`ModuleResolutionHost`). They live in a `thread_local!`, so constructing a fresh
`ModuleResolver` per compilation does **not** clear them. Only
`ModuleResolver::clear_cache` (which calls `clear_path_existence_caches`) or the
free-function `reset_path_existence_caches` does. The batch / merge-group worker
reuses one thread across many compilations without a long-lived resolver, so the
per-compilation boundary (`clear_batch_iteration_state`) must call
`reset_path_existence_caches` alongside the interner/solver-limit/checker resets.
This completes the worker-reuse isolation contract that keeps batch results
byte-identical to a fresh process (the `#13368`/`#13255` isolation family) — a
later compilation on a reused worker must never read a stale existence answer for
a path whose on-disk state changed (emit-then-recheck, watch rebuild, reused
temp path).

**Cache-statistics surface.** `ModuleResolverCacheStatistics` and
`cache_estimated_size_bytes` expose entry counts, hit/miss counters
(`Cell<u64>`), and an approximate retained-bytes estimate, consumed by the
performance/residency tooling.

**Path identity is textual.** Two `PathBuf`s with different segment shapes are
treated as distinct files even when they point at the same physical location.
`normalize_path_segments` (delegating to
`tsz_common::module_resolution::path_identity::normalize_segments`) collapses
`./`/`../` so `baseUrl`-joined, `paths`-target, `main`/`exports`-target, and
container-relative specifiers do not mint two identities for one file. A `..`
that escapes the root clamps (rather than surviving as `/../foo`), matching the
CLI driver's canonical identity so the two cannot drift.

---

## From `ResolutionFailure` to a diagnostic code

`diagnostics.rs` owns the failure taxonomy and code selection. The `lookup`
method translates the kernel result into a `ModuleLookupResult` whose `error`
carries the final `u32`:

| Failure / situation | Code | Message family |
| --- | --- | --- |
| `NotFound`, `InvalidSpecifier`, `PackageJsonError`, `CircularResolution`, `PathMappingFailed` | `TS2307` | "Cannot find module '…' or its corresponding type declarations." |
| Classic-style + bare specifier, `implied_classic_resolution` | `TS2792` | "…Did you mean to set the 'moduleResolution' option to 'nodenext'…" |
| `.json` import, no `resolveJsonModule` | `TS2732` | "…Consider using '--resolveJsonModule'…" |
| ESM extensionless relative, Node16/NodeNext, no suggestion | `TS2834` | "Relative import paths need explicit file extensions…" |
| same, with a concrete suggestion | `TS2835` | "…Did you mean '…ext'?" |
| Explicit `.ts`/`.tsx`/`.mts`/`.cts` without `allowImportingTsExtensions` | `TS5097` | "An import path can only end with a '…' extension…" |
| `.tsx`/`.jsx` resolved but `jsx` unset | `TS6142` | "Module '…' was resolved to '…', but '--jsx' is not set." |
| `.d.<ext>.ts` resolved without `allowArbitraryExtensions` | `TS6263` | "…but '--allowArbitraryExtensions' is not set." |
| Self-reference, `exports` present, no `rootDir`/`outDir` | `TS2209` | "The project root is ambiguous…" |
| Imported JS, `noImplicitAny`, no declarations | `TS7016` | "Could not find a declaration file for module '…'." |

The classic→`TS2792` override (in `lookup`) is structural: tsc emits the
resolution-mode hint for any bare specifier that fails under classic-style
resolution — including `module: amd|system|umd|none` and explicit
`moduleResolution: classic` — regardless of whether a local `node_modules/<pkg>`
happens to exist. Relative/absolute specifiers stay on plain `TS2307` because
the hint cannot help them.

`should_try_fallback` marks the "soft" failures
(`NotFound`/`ModuleResolutionModeMismatch`/`PackageJsonError`/`PathMappingFailed`)
for which `lookup` will consult the driver's `fallback_resolve` closure before
giving up — but only when `should_skip_fallback_on_not_found` says not to skip
it. That skip-walk is itself the dominant BFS cost on multi-package projects
(an unbounded directory walk with a `package.json`+`exports`-map evaluation at
every `node_modules` ancestor), which is why it has its own
`skip_fallback_cache`.

The `node:`-builtin path: a specifier like `node:fs` that fails under its full
form retries with the scheme stripped (`fs`), resolving against `@types/node`,
matching tsc's handling of Node builtins as ambient module / package subpaths.

---

## Re-export validation: `validate_reexported_members`

When the driver has resolved an `export { … } from "mod"` statement, the checker
validates that each forwarded member actually exists in `mod`. This is the
checker's job, not the resolver's — the resolver only made the file index
available. The entry point is `check_export_module_specifier`
(`module_checker.rs`), which:

1. Skips `export { } from "…"` / `export type { } from "…"` with an empty clause
   (nothing is imported, so tsc does not require the module to exist).
2. Computes `requested_resolution_mode` from the statement's attributes.
3. Runs `check_module_specifier_ts_extension` (the shared `TS2846`/`TS5097`
   extension gate — re-exports follow the same rule as imports because tsc
   anchors the check on `findAncestor(location, isExportDeclaration)`).
4. Detects circular re-export chains (`would_create_cycle` against the
   `import_resolution_stack`; the binder's `wildcard_reexports` map feeds
   `check_reexport_chain_for_cycles`). A cycle emits `TS2307` with a "Circular
   re-export detected: a -> b -> c" message, deduped via
   `modules_with_ts2307_emitted`.
5. Once the module is known (in `resolved_modules`, `module_exports`, or a
   `declared_modules` ambient block), calls `check_export_target_is_module`
   (`TS2306` "File '…' is not a module" when the target is a script with no
   exports and is not JS/JSON) and then `validate_reexported_members`.

`validate_reexported_members` only inspects `NAMED_EXPORTS` clauses
(`export { foo, bar as baz } from "mod"`). It fetches the module's canonical
export surface via `resolve_effective_module_exports_with_mode` (a
binder-backed `SymbolTable`), then for each non-type-only specifier:

- The checked name is the **property name** (`bar` in `bar as baz`), or the
  plain name otherwise.
- `default` is allowed when the module has `export=` and
  `allowSyntheticDefaultImports`, or when it is a JSON default export.
- If the name is absent, it picks a diagnostic in priority order:
  - **TS2724** ("…has no exported member named '…'. Did you mean '…'?") when
    `get_spelling_suggestion` finds a near-match among the export names.
  - **TS2614** (the "did you mean to use `import … from` instead?" variant) when
    the module has a default-like export (`default`/`export=`/`module.exports`)
    but not the requested named member.
  - **TS2305** ("Module '…' has no exported member '…'.") otherwise.
- A `module.exports` plus default-like export combination produces the
  `import … from` suggestion form.

Type-only re-exports are skipped (the referenced type may not appear in the
runtime exports table). Finally, members that re-export a type-only symbol
(`import_binding_is_type_only`) are recorded in `ctx.type_only_nodes` so the
emitter elides them — the resolver/checker boundary feeding the
[emitter](emitter.md), not a semantic decision the emitter makes itself.

The export surface and "does this name exist" answer come from the binder's
`SymbolTable`, never from re-running module resolution or reading printer
output. Member *existence* is a binder query; member *type* (for the dynamic
`import()` namespace object built by `get_dynamic_import_type`) is a solver
query via `get_type_of_symbol`.

---

## Import-attribute grammar and assignability (`TS2821`–`TS2880`)

The `with { … }` / `assert { … }` clause on an import/export, and the type-only
`resolution-mode` attribute, are validated in
`import/declaration_attributes.rs`. These are *grammar* and *assignability*
checks; the resolver consumes the resulting `resolution-mode` override but does
not validate the clause.

### `check_import_attributes_grammar`

This mirrors tsc's `checkImportAttributes` ordering exactly — the order matters
because each step *returns*, and the code depends on whether the deprecated
`assert` keyword or the `with` keyword was used (`uses_with` is read from
`attrs_data.token == WithKeyword`). The diagnostic selection routes through
`report_import_attribute_grammar_error`, which picks the `with`-keyword code or
its `assert`-keyword counterpart.

| Step | Condition | `with` code | `assert` code |
| --- | --- | --- | --- |
| 1 | Effective `resolution-mode` on a type-only decl | *(accepted — return)* | *(accepted)* |
| 2 | `module` does not support import attributes (`FeatureGate::ImportAttributes`) | `TS2823` | `TS2821` |
| 3/4 | `assert` keyword: hard error under node20/nodenext, deprecation elsewhere | — | `TS2880` |
| 5 | Statement compiles to CommonJS `require` (`import_declaration_emits_commonjs`) | `TS2856` | `TS2836` |
| 6 | Type-only declaration | `TS2857` | `TS2822` |

Steps 5 and 6 are intentionally CommonJS-before-type-only: tsc reports the
`require`-incompatibility even for `import type` statements, because the grammar
check does not depend on whether the binding is erased. The `assert` step is
split: under node20/nodenext it is a hard `TS2880` that stops; in other supported
modes it is a deprecation (gated on `ignoreDeprecations`) and checking continues.

### `check_import_attributes_assignability` (`TS2322` / `TS2858`)

For `import … with { type: "json" }`, this builds an object type from the
attribute entries and checks it against the global `ImportAttributes` interface
(`resolve_lib_type_by_name("ImportAttributes")`, which includes user
`declare global { interface ImportAttributes { … } }` augmentations). A
non-string-literal value emits **TS2858** ("Import attribute values must be
string literal expressions") at the value position; object-literal values are
widened for display. The relation check itself is a solver query
(`import_attributes_relation_outcome`); a failure emits **TS2322** via the
[assignability gateway](checker-assignability-gateway.md). The checker constructs
the candidate object type and *asks* the solver — it does not run the relation
itself.

### `check_type_only_resolution_mode_attribute_grammar` (`TS1453`–`TS1464`)

Whole-declaration `import type` / `export type` statements get extra grammar
validation for the `resolution-mode` attribute, but only under Node16/NodeNext
(`module.is_node_module()`). It mirrors tsc's
`getResolutionModeOverride(…, grammarErrorOnNode)`:

- Wrong arity (not exactly one key) → **TS1464** (`with`) / **TS1456**
  (`assert`).
- Key other than `resolution-mode` (and not a `type: "json"` attribute) →
  **TS1463** (`with`) / **TS1455** (`assert`).
- Value other than `"import"`/`"require"` → **TS1453** ("resolution-mode should
  be either 'require' or 'import'").

### Related attribute diagnostics

- `maybe_emit_json_esm_import_attribute_required` emits **TS1543** ("Importing a
  JSON file into an ECMAScript module requires a 'type: \"json\"' import
  attribute…") for default/namespace JSON imports under node18/node20/nodenext
  ESM without the attribute.
- `check_deferred_import_restrictions` emits **TS18058** (default imports) and
  **TS18059** (named imports) for `import defer …`, which must use `* as ns`.

### How the override flows back to the resolver

`get_resolution_mode_override` reads the `resolution-mode` attribute value into
a `ResolutionModeOverride::{Import, Require}`.
`resolution_mode_override_is_effective` decides whether it actually changes
resolution (it is effective whenever `FeatureGate::ImportAttributes` is
available, or — narrowly — under Node16 on a type-only declaration with only the
`resolution-mode` attribute). `requested_resolution_mode` turns that into the
`Option<ResolutionModeOverride>` the driver places in
`ModuleLookupRequest::resolution_mode_override`, which
`importing_module_kind_for_import` honors *above all else*. The override's
`ImportingModuleKind` then selects the `import` vs `require` conditional-`exports`
branch and the Node16 extension priority — closing the loop between the
checker's grammar validation and the resolver's path computation.

---

## Edge cases and tsc parity

- **Empty re-export clause.** `export { } from "./x"` and `export type { } from
  "./x"` do not require `./x` to exist and emit no extension diagnostic — matched
  structurally on `NAMED_EXPORTS` + empty elements (`export_named_clause_is_empty`).

- **Per-site re-export diagnostics.** Unlike imports (deduped per module),
  re-exports report `TS2307` *per `export ... from` site*.
  `check_export_module_specifier` removes the module's `modules_with_ts2307_emitted`
  entry up front so each site gets one chance.

- **Extension diagnostic priority.** When `TS2846` or `TS5097` already fired for
  a re-export/import specifier, the lower-priority `TS2307`/`TS2792`
  "cannot find module" family is suppressed — tsc prioritizes the
  extension-specific diagnostic. Other resolution errors (e.g. `TS6142`) still
  surface.

- **`exports` blocks `typesVersions`.** A package with an `exports` map never
  consults `typesVersions` as a fallback (`resolve_package` returns `NotFound`
  for an unexposed subpath under Node16/NodeNext/Bundler). The legacy
  `typesVersions` field applies only to packages *without* an `exports` map.

- **Symlinked package roots.** `resolve_package` skips the `index.{ext}` fallback
  for a symlinked package root *only* when its `package.json` declares entry-point
  fields (`exports`/`main`/`types`/`typings`) — matching tsc's expectation of an
  explicit entry point for linked packages while preserving index fallback for
  bare ones.

- **`main` field is non-recursive.** A `main` pointing at a directory tries only
  `index.{ext}` inside it; it does **not** read a nested `package.json`.

- **Ambient `declare module "…"` in `.d.ts`.** `resolve_bare_specifier` can match
  an ambient subpath declared inside a package's `.d.ts` entry, but only through
  `source_declares_ambient_module`, a code-scope scan that skips comments,
  strings, regexes, and template literals — a substring scan would be fooled by a
  `declare module "…"` inside a JSDoc example.

- **`module.exports = …` as `export =`.** `target_module_has_export_equals` scans
  the target AST for both `export = …` and the JS-equivalent top-level
  `module.exports = …` assignment, so re-export and namespace-display parity holds
  for CommonJS-style modules.

- **JSX runtime extension suggestion.** `suggested_runtime_extension` maps a
  resolved `.tsx` to `.jsx` only under `jsx: preserve`, else `.js` — so the
  `TS2835` suggestion matches what would actually exist after emit.

---

## Where to look next

- The driver code that *calls* `lookup`, maps resolved paths to file indices,
  and feeds the checker's resolution maps:
  [Driver: Incremental and Watch](driver-incremental-and-watch.md),
  [Driver: Project References and Build Mode](driver-project-references-and-build-mode.md).
- The checker boundary that *consumes* resolution maps for member/namespace/
  `export =` validation: [Declarations: Imports, Exports, Namespaces, Modules,
  and Ambient](checker-declarations-modules.md).
- How module-not-found and extension diagnostics are formatted and attached:
  [Checker: Error Reporter and Diagnostics](checker-error-reporter-diagnostics.md).
- The assignability path behind the `TS2322` import-attribute check:
  [Checker: The Assignability Gateway](checker-assignability-gateway.md).
- The whole pipeline in sequence: [End-to-End Timeline](end-to-end-timeline.md).
