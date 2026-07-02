# Declarations: Imports, Exports, Namespaces, Modules, and Ambient

This subsystem is the checker's *module boundary*: it validates everything that
crosses a file or namespace edge. It answers questions like "does this module
exist?" (`TS2307`/`TS2792`), "does the module export the member you asked for?"
(`TS2305`/`TS2614`/`TS2613`), "does this import collide with a local
declaration?" (`TS2440`/`TS2865`), "is `export =` legal here?"
(`TS1203`/`TS2309`), "does this re-export forward a type without `export type`?"
(`TS1205`/`TS1448`), and "is this `import`/`export` syntactically allowed under
`verbatimModuleSyntax` / `isolatedModules`?" (`TS1484`/`TS1485`/`TS1286`). It
also owns namespace declaration merging into class/function/object surfaces and
the dynamic `import(...)` call checks.

The code lives almost entirely under
`crates/tsz-checker/src/declarations`, with the import/export family further
split into `declarations/import` and its `declarations/import/core` leaf. The
orchestration that *invokes* these checks is the per-statement dispatcher in
`crates/tsz-checker/src/state/state_checking_members/statement_callback_bridge.rs`
plus the source-file-level sweep in
`crates/tsz-checker/src/state/state_checking/source_file.rs`. As with the rest
of the checker, this subsystem **orchestrates and attaches diagnostics to source
spans** but it **asks the solver** for every semantic answer (assignability of
an import attribute value, the type of a dynamic-import specifier, the shape of
a namespace export member). Module *resolution* — turning a specifier string
into a target file index — is owned by the driver/`ModuleResolver`; the checker
consumes the precomputed maps through `CheckerContext` query methods.

```text
SourceFile statements
        |
  statement_callback_bridge.rs        ── per-statement dispatch
        |   (IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION,
        |    EXPORT_DECLARATION, EXPORT_ASSIGNMENT, MODULE_DECLARATION)
        v
  declarations/import/*  declarations/module_checker.rs  declarations_module.rs
        |                                                    namespace_checker.rs
        |  resolve specifier -> file idx       merge namespace exports
        v                                                    |
  CheckerContext::resolve_import_target*  <── driver maps (resolved_module_paths,
        |   get_resolution_error*              resolved_modules, global_*indexes)
        v
  binder export tables  +  solver (attribute assignability, specifier type)
        |
  error_reporter  ── TS2307/TS2305/TS2440/TS1203/TS1205/TS1484/... with spans
```

Source-file-scoped sweeps that run once per file (not per statement) are kicked
off from `check_source_file` in `source_file.rs`:
`check_export_assignment(&statements)` (TS1203/TS2309/TS2714/TS2883),
`check_import_alias_duplicates`, `check_import_declaration_duplicate_bindings`,
and `check_wildcard_reexport_collisions` (TS2308). See lines 546-556 of
`source_file.rs`.

## Owns / Must not own

**Owns:**

- Syntactic and binding-level validation of `import`, `import =`, `export`,
  `export =`, `export default`, `export * [as ns]`, and module/namespace
  declarations.
- The decision of *which* module-not-found family to emit
  (`TS2307` vs `TS2792` vs `TS2580`/`TS2591` for Node builtins vs `TS2882`
  for side-effect imports), and the suppression precedence between
  extension diagnostics (`TS2846`/`TS5097`) and "cannot find module".
- Import-vs-local conflict policy (`TS2440`/`TS2865`) including the
  type-space/value-space distinction and `isolatedModules` rules.
- `verbatimModuleSyntax`/`isolatedModules` import/export erasability checks
  (`TS1484`/`TS1485`/`TS1205`/`TS1448`/`TS1286`/`TS1295`).
- Namespace declaration merging into class/function/object types and the
  `TS2300` duplicate-export-member collisions it produces.
- Cross-binder re-export chain traversal and cycle guards.

**Must not own:**

- *Module resolution itself.* The mapping `specifier -> file index` and the
  recorded resolution errors are produced by the driver/`ModuleResolver` and
  read through `CheckerContext`. This layer never re-implements
  `node_modules` walking, `exports`/`typesVersions`, or `paths`.
- *Relation/assignability kernels.* Import-attribute and dynamic-import-option
  validation route through the shared assignability gateway and solver
  relation outcomes (`call_arg_relation_outcome`,
  `check_assignable_or_report*`); this layer never hand-rolls structural
  subtyping. See [checker-assignability-gateway](checker-assignability-gateway.md).
- *Type construction policy.* Namespace member types come from the solver
  factory; this layer assembles `PropertyInfo` records but does not invent
  type shapes.
- *Printer output as a predicate.* No decision here reads rendered type text.

## Module / file map

| Path | Role |
| --- | --- |
| `declarations/mod.rs` | Module tree root: `import`, `module_checker`, `namespace_checker`, `declarations`, `declarations_module`, `dynamic_import_checker`. |
| `declarations/import/declaration_check_body.rs` | `check_import_declaration` — the top-level `import ... from` validator; extension diagnostics, CJS/ESM boundary (TS1479/TS1541), TS2307 family. |
| `declarations/import/declaration.rs` | Module-indicator/global-augmentation detection, `check_import_binding_reserved_words` (TS1214). |
| `declarations/import/declaration_resolution.rs` | Cross-binder re-export traversal, cycle guards, `check_import_declaration_conflicts` (TS2440/TS2865). |
| `declarations/import/equals.rs` | `import X = require(...)` / `import X = ns.Y` (TS1202/TS1147/TS2438/TS2439), ambient default-namespace dup (TS2300). |
| `declarations/import/exports.rs` | Wildcard re-export collision detection (TS2308) and export-origin tracing. |
| `declarations/import/verbatim.rs` | `verbatimModuleSyntax` import checks (TS1484/TS1485/TS1295/TS2748). |
| `declarations/import/declaration_attributes.rs` | Import-attribute grammar and assignability (TS2821-TS2880). |
| `declarations/import/core/import_members.rs` | `check_imported_members` — named/default/namespace export existence (TS2305/TS2613/TS2614). |
| `declarations/import/core/module_exports.rs` | `check_export_assignment` (TS1203/TS2309/TS2714/TS2883) and export-= target checks. |
| `declarations/import/core/helpers.rs` | `module_not_found_diagnostic*`, resolution-mode derivation, `export=` target classification (TS2497), `ModuleNotFoundSite`. |
| `declarations/module_checker.rs` | `check_export_module_specifier` (re-export resolution, TS2307), `check_export_star_of_export_equals_module` (TS2498). |
| `declarations/module_checker/verbatim_module_syntax.rs` | `verbatimModuleSyntax`/`isolatedModules` named-export checks (TS1205/TS1448). |
| `declarations/declarations_module.rs` | `check_module_declaration` for `namespace`/`declare module` (TS2397/TS2567/TS2668/TS2669...), namespace+class/function merge driver. |
| `declarations/namespace_checker.rs` | Namespace export merging into class/function/object surfaces; `merge_exports_into_props` (TS2300). |
| `declarations/dynamic_import_checker.rs` | `import(specifier, options)` checks (TS7036 specifier type, TS2322 options). |
| `module_resolution.rs` (crate root) | `build_module_resolution_maps`, `module_specifier_candidates`, file-index probing helpers consumed by `CheckerContext`. |
| `context/package_resolution.rs`, `context/resolver.rs`, `context/core.rs` | `CheckerContext` query surface: `resolve_import_target*`, `get_resolution_error*`, `resolution_mode_for_request`, `declared_modules_contains`. |

## Module-resolution integration on the checker side

The checker never walks the filesystem. The driver populates several maps that
`CheckerContext` exposes; the declaration checks call those query methods.

| Query (on `CheckerContext`) | Backing data | Used for |
| --- | --- | --- |
| `resolve_import_target(specifier)` / `..._from_file(idx, spec)` | `resolved_module_paths` (`(usize, String) -> usize`), then `global_file_name_index` fan-out, then `resolved_module_paths` fallback | specifier -> target file index |
| `resolve_import_target_from_file_with_mode(idx, spec, mode)` | mode-keyed variant for `import type` resolution-mode overrides | Node16/NodeNext dual resolution |
| `get_resolution_error(spec)` / `..._with_mode` / `..._for_request` | `ResolutionError` map recorded by the driver | exact TS2307/TS2792/TS2834/TS6142 code from the resolver |
| `resolved_modules` (`FxHashSet<String>`) via `resolved_module_set_contains_specifier` | driver's successful-resolution set | O(1) "did this resolve?" test |
| `declared_modules_contains(binder, name)` / `global_declared_modules` | binder ambient-module tables, prebuilt `GlobalDeclaredModules` (exact set + tsc-faithful prefix/suffix wildcard scan) | ambient `declare module "x"` and wildcard `declare module "*.css"` |
| `module_exports_contains_module(binder, name)` | binder `module_exports` table | bare module export surface lookup |
| `files_for_module_specifier(spec)` / `global_module_binder_index` | `FxHashMap<String, Vec<usize>>` | O(1) candidate binders for cross-file re-export resolution |

`resolve_import_target_from_file` (in `context/package_resolution.rs`) layers
three stages and deliberately prefers the driver's authoritative
`resolved_module_paths` over the heuristic file-index fan-out, because the
fan-out spells out `<stem>.<ext>` candidates directly and does not model
resolver features such as `moduleSuffixes` (`foo.ios.ts`), `exports`, or
`typesVersions`. A bare specifier that the real resolver missed but that matches
a `declare module "<spec>"` is intentionally *not* bound to an accidental
sibling source file (a `react.ts` importing `"react"`):
`bare_specifier_is_declared_ambient_module` short-circuits the file-index probe
so the import checker binds it against the ambient module instead (parity with
tsc, which resolves bare specifiers via `paths`/`baseUrl`/`node_modules`/ambient
declarations, never a relative sibling).

`module_specifier_candidates` (in `module_resolution.rs`) produces the
normalization fan-out — canonical form, raw input, and extension-stripped stem —
used to probe the resolved-module set. It is thread-local-memoized
(`CANDIDATES_MEMO`). Crucially, `module_specifier_error_candidates` does *not*
fan out to the stem: resolution *errors* are keyed by the exact user-written
specifier, because `import "./index.js"` and `import "./index"` can resolve
completely differently (the former via synthetic `.js -> .ts` substitution, the
latter failing with TS2835), and conflating them would mislabel the error line.

## Walk-through: `import { Foo } from "./mod"`

Tracing a named import through `check_import_declaration`
(`declaration_check_body.rs`):

1. `statement_callback_bridge::check_import_declaration` delegates to
   `CheckerState::check_import_declaration(stmt_idx)`.
2. The arena yields `ImportDeclData`. `resolution_mode_for_request` derives the
   effective `resolution-mode` from the module kind and the import's
   attributes. `is_type_only_import` is read from the `ImportClause`.
3. Wrong-context handling: `is_in_non_module_element_context` (an import in a
   bare block). Inside a function body the check returns early; otherwise
   module semantics still run after the grammar diagnostic.
4. Grammar/binding checks fire regardless of resolution:
   `check_deferred_import_restrictions` (TS18058/TS18059),
   the TS1363 default+named type-only clash,
   `check_import_attributes_assignability`/`check_import_attributes_grammar`,
   `check_import_binding_reserved_words` (TS1214), and
   `check_import_declaration_conflicts` (TS2440/TS2865).
5. If the import statement subtree has parse errors, semantic checks stop here.
6. `check_module_specifier_ts_extension` runs the TS2846 (`.d.ts` needs
   `import type`) and TS5097 (`.ts` needs `allowImportingTsExtensions`) checks.
   When it emits, `emitted_extension_diagnostic` suppresses the later TS2307.
7. The resolver is consulted: `resolve_import_target_from_file_for_request`
   (mode-aware) or `resolve_import_target`. `would_create_cycle` checks the
   `import_resolution_stack` for a re-entrant specifier; on a cycle it emits a
   "Circular import detected" message under the TS2307 code and returns.
8. The specifier is pushed onto `import_resolution_stack`. If
   `get_resolution_error_for_request` returns a recorded error, the code is
   normalized (TS2307/TS2792 routed through `module_not_found_diagnostic`,
   side-effect imports converted to TS2882) and emitted unless suppressed.
9. Ambient short-circuits: `is_ambient_module_match`, then the
   `global_declared_modules`/`shorthand_ambient_modules` lookup. Either returns
   after `check_imported_members` (which still emits JS-mode TS18042).
10. If `resolved_module_set_contains_specifier`, the target file index is
    resolved and the file-kind gates run: `File is not a module` (TS2306) when
    the target is a non-JS, non-JSON, non-ambient, non-external-module file;
    TS1479 (CJS importing ESM under Node16/Node18); TS1541 (type-only import
    crossing the CJS->ESM boundary without a resolution-mode). Then
    `check_imported_members` validates each binding, and
    `check_verbatim_module_syntax_imports` runs the VMS checks.
11. If nothing resolved and nothing is ambient, the fallback emits
    `module_not_found_diagnostic` (deduped via `modules_with_ts2307_emitted`),
    and the specifier is popped off the stack.

Inside `check_imported_members` (`core/import_members.rs`), the export table is
built lazily: a default-only import only needs to know whether a default-like
binding exists (`needs_full_exports` is false), avoiding the cost of
materializing a large bundle's full export surface (React's `.d.ts`). When a
named member is missing, the code distinguishes TS2305 ("has no exported
member"), TS2614 ("did you mean `import { X } from`"), TS2613 (default vs
named), the "did you mean Y?" spelling-suggestion variant
(`HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN`), and TS2459/TS2460 (member is
declared locally but not exported) — checking re-export chains before settling
on TS2305.

## Walk-through: `import D = require("./x")` inside a namespace

In `check_import_equals_declaration` (`equals.rs`):

1. JS files bail immediately — `import =` is TS-only syntax (`is_js_file`).
2. `modules_with_ts2307_emitted` is cleared for this module so each
   `import = require(...)` statement gets its own TS2307 chance (per-site
   parity with tsc).
3. TS1294 (`erasableSyntaxOnly`), the strict-mode reserved-word check, and
   TS2438 (`import string = ns.Foo` clobbering a reserved type name — only when
   `import_alias_target_has_type` is true) run.
4. TS1392 (`An import alias cannot use 'import type'`) fires for the
   namespace-alias form (`import type Foo = ns.Foo`) but not for
   `import type X = require(...)`, and is suppressed under parse errors.
5. TS2846/TS5097 run on the `require(...)` specifier via
   `require_specifier_span` + `check_module_specifier_ts_extension`, anchored on
   `findAncestor(location, isImportEqualsDeclaration)` like tsc.
6. The enclosing `MODULE_DECLARATION` is walked. When the statement is inside a
   value namespace, TS1147 ("Import declarations in a namespace cannot reference
   a module") is emitted *instead of* TS2307 — unless the required module is an
   ambient module declared anywhere (`declared_modules_contains` /
   `files_for_module_specifier`), in which case the reference is valid.
   Inside `declare global { }` in an external module, imports are rejected with
   "Imports are not permitted in module augmentations".

`import X = ns.Member` (the alias form) resolves the right-hand qualified name
via `resolve_qualified_symbol`, follows the alias chain with
`AliasCycleTracker`, and feeds the file-level `check_circular_import_aliases`
(TS2303) sweep.

## Walk-through: `export = X` / `export default X`

`check_export_assignment` (`core/module_exports.rs`) collects every
`EXPORT_ASSIGNMENT` and default-export statement in one pass over the file:

- TS1294 (`export =` under `erasableSyntaxOnly`), TS1282/TS1283 (`export =`
  under `verbatimModuleSyntax` via `check_vms_export_equals`), and TS2714
  ("expression of an export assignment must be an identifier or qualified name
  in an ambient context") for non-identifier ambient `export =`/`export default`.
- TS2303 ("Circular definition of import alias") when an `export = ident`
  forms a cycle through a global-augmentation namespace
  (`global_augmentation_namespace_export_cycle_report_node`).
- TS2883 ("inferred type of 'default' cannot be named without a reference to an
  external package") when emitting declarations for a non-call default export
  whose inferred type points at a non-portable cross-package reference
  (`first_non_portable_type_reference`).
- After the loop, TS1203 (`export =` cannot be used targeting ES modules — gated
  on file extension: `.cts`/`.d.ts` exempt, `.d.mts` not) and TS2309
  (`export =` cannot be used with other exported members).

The `export =` *target* is classified by
`export_equals_target_is_not_module_or_variable` in `core/helpers.rs`: a module
whose `export=` symbol is a class/function/interface (not `Module | Variable`)
cannot be namespace- or named-imported without `esModuleInterop` /
`allowSyntheticDefaultImports` (TS2497). `check_export_star_of_export_equals_module`
(`module_checker.rs`) emits TS2498 when a `export * [as ns] from "./m"`
re-exports a module that itself uses `export =`.

## Walk-through: re-exports `export { A } from "./b"` and `export * from "./c"`

`check_export_module_specifier` (`module_checker.rs`) is the re-export
counterpart of `check_import_declaration`:

- `export { } from "..."` and `export type { } from "..."` with an *empty*
  `NAMED_EXPORTS` clause (`export_named_clause_is_empty`) skip resolution
  entirely — nothing is imported, so tsc requires no module and emits no
  extension diagnostic.
- TS2846/TS5097 run on the re-export specifier first (anchored on
  `findAncestor(location, isExportDeclaration)`), then the
  `report_unresolved_imports` gate, then the per-site TS2307 dedup clear.
- Resolution is attempted via `resolved_modules`, then `module_exports`, then
  `shorthand_ambient_modules`, then `declared_modules`. On success
  `check_export_target_is_module` and `validate_reexported_members` run, plus
  `check_reexport_chain_for_cycles` over `wildcard_reexports`. Unlike imports,
  re-exports report TS2307 **per declaration site** — tsc does not dedup
  multiple `export ... from "x"` statements against the same missing module.

Wildcard re-export *collisions* are a separate file-level sweep,
`check_wildcard_reexport_collisions` (`import/exports.rs`): when two
`export * from`/`export type * from` statements re-export the same name from
different modules it emits TS2308. It suppresses the error when both paths trace
to the same origin file (`trace_export_origin` follows ALIAS chains through
`import_module`), matching tsc's "same value" rule, and never forwards
`default` (which `export *` does not re-export).

## Namespace merging

Namespace declaration checking has two halves. `check_module_declaration`
(`declarations_module.rs`) validates the declaration itself: TS2397
(`namespace globalThis`), TS2567 (namespace merging with a `const enum`), TS2668
(`export` on an ambient module), TS2669/TS2670 (global-scope augmentation
nesting), and routing into namespace+class/function merge checks
(`check_namespace_merges_with_class_or_function`).

`namespace_checker.rs` owns the *type-surface* merge. When a namespace shares a
name with a class, function, or object, its exported members fold into that
type's apparent surface:

- `merge_namespace_exports_into_constructor` (class `static` surface,
  `check_prototype = true`),
- `merge_namespace_exports_into_function`,
- `merge_namespace_exports_into_object`,
- `build_namespace_object_type` (standalone `namespace X { export ... }`).

All three call `merge_exports_into_props`. That loop:

- skips members already in the active `symbol_resolution_set` (recursion guard);
- skips members not visible on the exported surface
  (`namespace_member_visible_on_exported_surface` — an `export`-less inner
  member of a `declare`/identifier-named namespace is still publicly visible as
  `X.member`);
- skips type-only exports (interfaces, type aliases) — they do not collide with
  value properties;
- skips non-instantiated namespace exports (`is_namespace_instantiated`) since
  they produce no runtime value;
- on a name collision with an existing prop (or, for classes, `"prototype"`),
  emits TS2300 ("Duplicate identifier") on a *directly declared* class static
  member via `report_duplicate_on_class_static_member`. For an *inherited*
  duplicate it instead replaces the inherited property with the namespace
  export so `typeof Derived` reflects the namespace version (which then triggers
  TS2417 when incompatible) — parity with tsc.

The recursion depth is bounded by `MAX_MERGE_DEPTH = 32`, sharing the context's
`symbol_resolution_depth` counter to stop infinite recursion on namespaces that
re-export each other circularly. The member type comes from
`namespace_export_member_type` (solver factory), never hand-built.

## Ambient and `declare` handling

Ambient module declarations participate at three points:

- **Resolution suppression.** `is_ambient_module_match`,
  `any_ambient_module_declared`, and `wildcard_ambient_module_declared`
  (`core/ambient_modules.rs`) consult `global_declared_modules` (an exact
  `FxHashSet` plus a tsc-faithful prefix/suffix scan for wildcard
  `declare module "*.css"`),
  falling back to a per-binder scan of `declared_modules` /
  `shorthand_ambient_modules` / `module_exports`. A matching ambient module
  suppresses TS2307 — but `check_imported_members` still runs (so missing named
  members of a non-shorthand ambient module are still validated, and JS-mode
  TS18042 still fires).
- **Global augmentation policy.**
  `source_file_has_top_level_global_augmentation` and
  `source_file_has_module_augmentation` distinguish `declare global { }`
  (global-scope) from `declare module "X" { }` (module augmentation).
  `maybe_emit_imported_global_augmentation_errors` emits "Augmentations for the
  global scope can only be directly nested in external modules or ambient module
  declarations" when a `declare global` appears in a file with no module
  indicator (`source_file_has_syntactic_module_indicator`). A `.d.ts` file whose
  only top-level construct is a `declare global` is treated as a
  global-augmentation file: it is *not* required to be a module (the TS2306
  "File is not a module" gate skips it via
  `target_is_global_augmentation_dts`).
- **Import-conflict exemptions.** `check_import_declaration_conflicts` skips any
  candidate declaration inside a module augmentation
  (`is_inside_module_augmentation`) or global augmentation
  (`is_inside_global_augmentation`), and skips `export as namespace X`
  (`decl_is_namespace_export_declaration`), because those merge into other
  tables/scopes and must not collide with a module-scope import.

`TS2300` for `export default N` colliding with a sibling type-only namespace
`N` inside an ambient external module is handled by
`check_ambient_default_namespace_export_duplicates` (`equals.rs`); it
deliberately yields to TS2395 when a sibling value declaration shares the name,
to avoid double-anchoring (`namespaceNotMergedWithFunctionDefaultExport.ts`).

## Import-vs-local conflicts (TS2440 / TS2865)

`check_import_declaration_conflicts` (`declaration_resolution.rs`) is the most
intricate check here. For each import binding it resolves the alias
(`resolve_alias_symbol` with `AliasCycleTracker`) to determine whether the
imported target carries `Value` and/or `Type` meaning, then looks for a local
declaration with the same name in the same scope. Key behaviors:

- A namespace import (`import * as X`) always creates a value binding (the
  module namespace object) even when the target is unresolved.
- The value/type meaning is computed both from the resolved symbol's flags and,
  when resolution stays cross-file, by re-deriving from the target binder's
  export table (following re-export chains via `resolve_export_in_file` and
  `alias_partner_for`). A pure `VALUE_MODULE` namespace only counts as a value
  if some declaration is *instantiated*.
- Scope matching compares `ScopeId`s, with a merged-namespace fallback that
  treats two scopes as the same when their container nodes map to the same
  symbol.
- Type aliases/interfaces (type-declaration space) only conflict with imports
  that *also* carry type meaning; value imports only conflict with value-space
  locals.
- When the imported target has **no value**, `report_isolated_modules_import_conflicts`
  decides between TS2440 (local *type* collides with imported type) and TS2865
  (local *value* collides with a type-only-target import, only under
  `isolatedModules` and not `verbatimModuleSyntax`). TS2865 anchors at the whole
  import specifier; TS2440 anchors at the imported name.

Reported names are recorded in `import_conflict_names` so the type-alias
circularity check (`TS2456`) can suppress a false positive caused by the same
conflict.

## verbatimModuleSyntax / isolatedModules

Two files split these checks by direction:

- **Imports** — `import/verbatim.rs` (`check_verbatim_module_syntax_imports`):
  TS1295 (ESM `import` syntax in a CJS+VMS file, anchored at the binding name),
  TS1484 ("X is a type and must be imported using a type-only import") for a
  directly-typed import, and TS1485 ("X resolves to a type-only declaration")
  for an alias/re-export chain. The TS1484-vs-TS1485 split keys on whether the
  source export is a direct type declaration
  (`is_import_specifier_type_only && !is_import_specifier_alias_reexport`) or an
  alias.
- **Exports** — `module_checker/verbatim_module_syntax.rs`
  (`check_verbatim_module_syntax_named_exports`): TS1205 ("Re-exporting a type
  when '{0}' is enabled requires using 'export type'") and TS1448 (the
  `isolatedModules` "resolves to a type-only declaration and must be re-exported
  using a type-only re-export" variant). The option name in the message is
  `verbatimModuleSyntax` or `isolatedModules` depending on which is set;
  `.d.ts` files are exempt.

The underlying type-only determination uses
`binder_symbol_is_type_only`/`symbol_has_runtime_value_in_binder` (`verbatim.rs`),
which treats a symbol as type-only when it is `is_type_only`, a pure
interface/type-alias with no value flags, or a namespace/value-module symbol
with no runtime value (recursively checking members and `export * as Ns`
aliases).

## Dynamic `import(...)`

`dynamic_import_checker.rs` runs from the call checker
(`types/computation/call/mod.rs` line 1499) when a `CallExpression` is an
`import(...)`:

- `check_dynamic_import_specifier_type` — TS7036: the first argument's type must
  be assignable to `string`. `string`/`any`/`error`/`never` pass trivially;
  otherwise it routes through `call_arg_relation_outcome(arg_type, STRING)` (a
  solver relation outcome, not a hand-rolled check).
- `check_dynamic_import_options_type` — TS2322/TS2559: the second argument
  (options) must be assignable to `{ with?: ImportAttributes; assert?:
  ImportAttributes }`, built from the *augmented* `ImportAttributes`
  (resolved with `declare global` user augmentations) via the solver factory.
  It also runs TS2880 (deprecated `assert`) on the options object. Because
  `ImportCallOptions` is a weak type, a primitive/literal source emits TS2559
  directly with tsc's exact wording.

## Caches and invariants

| Cache / field (on `CheckerContext`) | Purpose | Invalidation / lifetime |
| --- | --- | --- |
| `modules_with_ts2307_emitted: CowCache<FxHashSet<String>>` | Dedup of module-not-found emissions within one statement's resolution attempts | Each import/import-equals/re-export *site* `.remove`s its specifier up front, so every declaration gets one chance; insert-on-emit prevents the resolution-error path and the fallback path from double-emitting within the same statement. |
| `import_resolution_stack: Vec<String>` | Re-entrancy / cycle detection across `import` + `export ... from` chains | Push on entry, pop on every return path; `would_create_cycle` tests membership. Pairs with "Circular import/re-export detected" under the TS2307 code. |
| `import_conflict_names: FxHashSet<String>` | Names that triggered TS2440/TS2865 | Read by the TS2456 circular-type-alias check to suppress a follow-on false positive. |
| `global_declared_modules: Arc<GlobalDeclaredModules>` | O(1) exact + tsc-faithful wildcard-scan ambient-module membership | Prebuilt once by the driver; read-only during checking. |
| `global_module_binder_index: Arc<FxHashMap<String, Vec<usize>>>` | O(1) candidate binders for a module specifier | Prebuilt; falls back to an O(N) binder scan when absent. |
| `resolved_module_paths` / `resolved_modules` | Authoritative driver resolution map + success set | Prebuilt by the driver; the checker only reads them. |
| `CANDIDATES_MEMO` (thread-local in `module_resolution.rs`) | Memoized `module_specifier_candidates` fan-out | Per-thread; lives for the process. |

Invariants worth preserving:

- TS2307 is **per import declaration** but **per re-export site**; the dedup set
  is cleared at the start of every site so multiple declarations of the same
  module each report once.
- Extension diagnostics (TS2846/TS5097/TS2876/TS2877) take precedence over
  TS2307: when `emitted_extension_diagnostic` is set, the module-not-found
  family is suppressed for that site.
- Side-effect imports (`import "mod"`) are *silently ignored* on resolution
  failure when `noUncheckedSideEffectImports` is off (the default), and use
  TS2882 (not TS2307/TS2792) when it is on.
- A resolved target is *never* TS2307 even with an empty export surface
  (`export {}`, `declare global`, only side-effect imports) — TS2307 means the
  file cannot be found, not that it has no exports.

## Edge cases and tsc parity

- **`.d.ts` import requires `import type`.** TS2846 fires only when the `.d.ts`
  module actually resolves; an unresolved one yields TS2307 instead. The
  suggested replacement honors `allowImportingTsExtensions` and the module kind
  (extensionless for CommonJS-like kinds, `.js`/`.mjs`/`.cjs` for ESM-like).
- **`.ts` extension under `allowImportingTsExtensions`.** TS5097 is suppressed
  by both `allowImportingTsExtensions` and `rewriteRelativeImportExtensions`,
  and is never emitted inside `.d.ts` files or when the resolver already
  reported TS6142 (`jsx` not set).
- **CJS/ESM boundary.** TS1479 (CJS importing ESM) only applies under
  Node16/Node18 module kinds — Node20/NodeNext, bundler resolution, and pure
  ESM kinds handle interop transparently. `.cjs` relative imports are
  suppressed; `.cts` still reports. TS1541 requires a `resolution-mode` on a
  type-only import that crosses the CJS->ESM boundary.
- **`export * from` does not forward `default`.** Both the collision check
  (TS2308) and export-name collection skip `default`.
- **AMD/System/classic resolution.**
  `deprecated_mode_suppresses_module_not_found` mirrors issue #3077: the
  secondary missing-module diagnostic surfaces only when the TS5107 deprecation
  is silenced via `ignoreDeprecations`.
- **Node builtins.** A missing `"fs"`/`"node:fs"` import uses the
  `@types/node` install hint (TS2580 for normal TS sites, TS2591 for
  require-like / `import type` / JS sites) rather than TS2307, decided by
  `is_known_node_module` + `ModuleNotFoundSite` in `core/helpers.rs`.
- **`export {} from "..."` / `export type {} from "..."`.** An empty named
  clause binds nothing, so resolution and extension diagnostics are skipped
  entirely.
- **Namespace `import` inside a namespace.** `import X = require(...)` in a value
  namespace reports TS1147 instead of TS2307, but is valid (no diagnostic) when
  the required module is an ambient module declared anywhere in the program.

## Cross-references

Module resolution policy and the data maps consumed here are an interface to the
driver/CLI surface ([end-to-end-timeline](end-to-end-timeline.md)). The binder
populates `module_exports`, `reexports`, `wildcard_reexports`,
`declared_modules`, and `shorthand_ambient_modules` that every check above reads
([binder](binder.md)). Import-attribute and dynamic-import-option validation
route through the assignability gateway
([checker-assignability-gateway](checker-assignability-gateway.md)) and the
solver relation kernel ([solver-relations](solver-relations.md)). Namespace
member types are constructed by the solver factory
([solver-types-intern-def](solver-types-intern-def.md),
[solver-instantiation](solver-instantiation.md)), and the lazy `DefId ->
TypeId` resolution that backs cross-file symbols is described in
[checker-context-and-state](checker-context-and-state.md). The duplicate-export
and merge collisions overlap class checking
([checker-classes](checker-classes.md)) and enum/JSX surfaces
([checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md)).
Diagnostic emission and span anchoring go through the error reporter
([checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md)),
and parser-supplied syntax kinds (`IMPORT_DECLARATION`, `EXPORT_ASSIGNMENT`,
`MODULE_DECLARATION`, `NAMED_IMPORTS`, …) come from the front end
([front-end-scanner-parser](front-end-scanner-parser.md)). Emit-side handling of
imports/exports and `export =` lowering lives in [emitter](emitter.md).
