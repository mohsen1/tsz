# Sound Overlay Cache Note

This note makes the overlay-cache design in [SOUND_MODE.md](./SOUND_MODE.md) more concrete in three places:

1. the exact canonical JSON payload for `resolution_profile_hash`
2. the decision to use one shared object store for packages and referenced-project declaration outputs
3. a minimal Rust schema prototype that pressure-tests the manifest/index shape
4. the operational rules for lock scope and cache garbage collection

## Decision: One Shared Object Store

Use one object store under `.tsz/sound-overlays/objects/<entry_hash>/` for both:

1. external package overlays from `node_modules` / package-manager installs
2. referenced-project emitted declaration outputs used as trust boundaries

Do **not** build separate cache systems for package overlays and project-declaration overlays.

The storage, commit protocol, and GC rules are the same. The thing that differs is the tagged `subject` identity and the lock scope.

### Why one store is better

1. The same transform pipeline can apply to package-owned `.d.ts` and referenced-project emitted `.d.ts`.
2. The same manifest and output-file hashing rules should govern both.
3. The same debug tooling (`tsz debug sound-overlays ...`) can inspect both.
4. Composite projects are a primary design target for sound-mode boundary handling, so pushing them into a second cache would create drift quickly.

### Subject kinds

The manifest/index schema should use a tagged `subject` union:

1. `package`
2. `project_declarations`

The lock scope should still be subject-specific, for example:

1. `locks/pkg.react@18.3.1.lock`
2. `locks/project.packages-lib-tsconfig.lock`

The important invariant is:

1. one writer per logical subject at a time
2. one content-addressed object format for everything

## Exact `resolution_profile_hash` Payload

`resolution_profile_hash` should hash the **resolved**, canonical module-resolution behavior that can change which declaration entrypoints or package-owned declaration files are selected.

It should be computed as:

1. build a canonical JSON payload from resolved values, not raw tsconfig syntax
2. serialize with a deterministic field order
3. hash the UTF-8 bytes of that canonical JSON

### Included fields

These fields should be included exactly:

1. `schema_version`
2. `effective_module_resolution`
3. `resolve_package_json_exports`
4. `resolve_package_json_imports`
5. `module_suffixes`
6. `allow_arbitrary_extensions`
7. `allow_importing_ts_extensions`
8. `rewrite_relative_import_extensions`
9. `resolve_json_module`
10. `custom_conditions`
11. `types_versions_compiler_version`

### Field rules

1. `effective_module_resolution` should be the resolved behavior (`classic`, `node`, `node16`, `nodenext`, `bundler`), not the raw optional tsconfig field.
2. `module_suffixes` must preserve order because probing order affects resolution.
3. `custom_conditions` must preserve order because the resolver can evaluate them in order.
4. Booleans must be fully resolved to `true` / `false`, not omitted.
5. `types_versions_compiler_version` should be the resolved version string or `null`.

### Explicit non-members

These should **not** be part of `resolution_profile_hash`:

1. package bytes or package identity
2. upstream declaration file hashes
3. transform pipeline configuration
4. consumer-owned path mappings like `baseUrl` / `paths`
5. `preserve_symlinks`

Rationale:

1. package bytes belong to the subject identity and declaration-closure hash
2. transform behavior belongs to `transform_profile_hash`
3. `baseUrl` / `paths` can affect consumer resolution globally, but should not change the package-owned declaration closure for a resolved external package object
4. `preserve_symlinks` affects how the subject is located, not the declaration bytes once the subject has been selected

### Canonical payload shape

```jsonc
{
  "schema_version": 1,
  "effective_module_resolution": "node16",
  "resolve_package_json_exports": true,
  "resolve_package_json_imports": true,
  "module_suffixes": [""],
  "allow_arbitrary_extensions": false,
  "allow_importing_ts_extensions": false,
  "rewrite_relative_import_extensions": false,
  "resolve_json_module": false,
  "custom_conditions": ["types"],
  "types_versions_compiler_version": "5.9.0"
}
```

For referenced-project declaration outputs, the same payload shape still applies. If a field is irrelevant in a particular access mode, it should still be emitted in canonical form with the resolved `false`, `[]`, or `null` value.

## Exact `transform_profile_hash` Payload

`transform_profile_hash` should hash the transform pipeline alone, not the selected upstream files.

It should be computed from:

1. `overlay_schema_version`
2. the ordered transform pipeline
3. each transform `id`
4. each transform implementation version or build fingerprint
5. each transform's canonicalized option payload
6. printer settings that affect output bytes

### Canonical payload shape

```jsonc
{
  "overlay_schema_version": 1,
  "pipeline": [
    {
      "id": "any_to_unknown_boundaries",
      "impl_version": "1.2.0",
      "options": {
        "callback_parameter_policy": "project_to_unknown",
        "readable_property_policy": "project_to_unknown",
        "top_level_sink_parameter_policy": "preserve_any"
      }
    },
    {
      "id": "method_to_property_variance",
      "impl_version": "1.2.0",
      "options": {
        "skip_overloads": true
      }
    }
  ],
  "printer": {
    "line_endings": "lf",
    "trailing_newline": true
  }
}
```

It should change when:

1. a transform is added, removed, or reordered
2. a transform implementation changes
3. a transform option changes
4. printer output changes in a byte-visible way

It should **not** change when:

1. a package updates but the transform pipeline stays the same
2. the resolved declaration closure changes under the same transform policy
3. lock/index file layout changes without changing emitted output semantics

## Minimal Rust Schema Prototype

The corresponding prototype types live in:

1. `crates/tsz-core/src/sound_overlay_cache.rs`

They are intentionally small and non-integrated. The goal is to pressure-test:

1. whether the tagged `subject` split is ergonomic
2. whether the index/manifest duplication still feels reasonable in real structs
3. whether the canonical payloads are still believable once `serde` owns them

The prototype currently includes:

1. `SoundOverlayResolutionProfile`
2. `SoundOverlayTransformProfile`
3. `SoundOverlaySubject`
4. `SoundOverlayCacheLayout`
5. `from_resolved_compiler_options(...)`
6. `publish_object(...)` / `read_manifest_if_valid(...)`
7. `build_package_declaration_closure(...)`
8. `compute_entry_hash(...)`
9. `lock_scope_name()`

A small prototype debug surface also exists in:

1. `crates/tsz-cli/src/sound_overlay_debug.rs`

It currently renders the computed subject plus resolution/transform/closure/entry hashes for:

1. a package rooted at `package.json`
2. a referenced/composite project represented by `ResolvedProject`

## Lock Scope and GC Rules

The lock scope should be subject-oriented, not object-oriented.

That means:

1. use one lock per logical package subject
2. use one lock per logical project-declarations subject
3. do not lock on `entry_hash`, because writers need to decide whether an object already exists before they know whether to publish a new one

### Lock naming

Prototype naming rule:

1. packages: `locks/pkg.<sanitized-name>.<version>.lock`
2. projects: `locks/project.<sanitized-config-path>.lock`

This is not the final naming contract, but it is good enough to prove that package/project lock scopes should be different even inside one shared object store.

### GC rules

The object store needs background cleanup rules from day one, even if the first implementation is small.

Recommended minimum rules:

1. prune stale `tmp/` staging directories older than a short safety window
2. remove orphaned `objects/<entry_hash>/` directories that are not referenced by `index.json` and are older than a second safety window
3. never delete an object whose manifest is currently referenced by `index.json`
4. never trust `index.json` alone for deletion; confirm the object manifest exists and matches the entry

Recommended timing model:

1. prune `tmp/` opportunistically on writer startup
2. run orphan-object GC only on explicit maintenance commands or bounded startup checks
3. avoid heavy directory scans on every normal read path

### Why GC must stay simple at first

1. overlay infrastructure is already later-phase work
2. the first implementation should optimize for correctness and debuggability, not maximal cache compaction
3. usage-based eviction or cross-process LRU policy can come later if disk growth becomes a real problem

## Proposed invariants

1. `index.json` is advisory only
2. `objects/<entry_hash>/manifest.json` is authoritative
3. every object must be self-describing without consulting `index.json`
4. every cache entry must be identifiable by `(subject, resolution_profile_hash, transform_profile_hash, upstream_declaration_closure_hash)`
5. package and project declarations may share the object store but must never share the same subject identity

## Relationship To Projected Boundary Caches

Projected boundary caches and overlay object caches should stay **separate**.

They may consume some of the same underlying facts:

1. declaration file identity
2. emitted `.d.ts` freshness
3. package / project closure hashes

But they should not share one invalidation table or one storage layer.

### Why keep them separate

1. Projected boundary caches are checker-local and semantic.
   They are keyed by observed symbol/type, polarity, and instantiation context.
2. Overlay object caches are filesystem-persistent and textual.
   They are keyed by subject identity plus resolution/transform/closure hashes.
3. They invalidate for different reasons and at different granularities.
4. Trying to unify them too early would make both designs harder to reason about.

### Practical rule

1. share low-level source facts where useful
   - file hashes
   - declaration closure hashes
   - referenced-project freshness signals
2. do **not** share higher-level cache metadata structures
3. do **not** let projected-type cache invalidation depend on the overlay object-store index

This keeps the persistent overlay cache an optional later optimization layer instead of making it a prerequisite for projected-boundary correctness.
