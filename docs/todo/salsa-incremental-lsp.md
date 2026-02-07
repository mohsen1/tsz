# Salsa-Based Incremental Type Checking for LSP

**Date**: 2026-02-07
**Status**: Planning
**Goal**: Sub-millisecond incremental diagnostics — faster than tsgo's LSP

---

## Executive Summary

Use [Salsa 3.x](https://github.com/salsa-rs/salsa) as the **LSP orchestration layer** to
achieve fine-grained incremental type checking. Salsa sits *on top of* the existing
checker/solver — it decides *what* needs to be re-checked, then delegates to the existing
machinery. This is fundamentally different from the failed experiment that tried to put
Salsa *inside* the solver.

**Core insight**: tsgo wins on raw throughput (full-file checking is fast in Go). We win on
*incremental* checking — on a 5000-line file, editing one function body re-checks ~1
declaration instead of 100. That's the structural advantage Rust + Salsa gives us.

---

## Step 0: Remove Failed Salsa Experiment

The old proof-of-concept in `crates/tsz-solver/src/salsa_db.rs` wraps low-level solver
operations (`is_subtype_of`, `evaluate_type`) with Salsa 0.16. This was the wrong
abstraction level — the solver's internal mutable state (`SubtypeChecker`, `TypeEvaluator`)
is incompatible with Salsa's query model at that granularity.

**What to remove:**

| File | Action |
|------|--------|
| `crates/tsz-solver/src/salsa_db.rs` | Delete entirely |
| `crates/tsz-solver/src/lib.rs` | Remove `#[cfg(feature = "experimental_salsa")] pub mod salsa_db;` and re-export |
| `crates/tsz-solver/src/tests/db_tests.rs` | Remove 3 `#[cfg(feature = "experimental_salsa")]` test functions |
| `crates/tsz-solver/Cargo.toml` | Remove `salsa` dependency and `experimental_salsa` feature |
| `Cargo.toml` (workspace) | Remove `salsa = "0.16"` from `[workspace.dependencies]`, remove `experimental_salsa` feature |

**Why remove first**: Clean slate. The old API (Salsa 0.16 query groups) is completely
different from Salsa 3.x (`#[salsa::tracked]` structs). Keeping it around creates confusion
about which approach is intended.

---

## Architecture: Salsa as the LSP Brain

```
┌─────────────────────────────────────────────────────┐
│                     LSP Server                       │
│            (JSON-RPC, textDocument/*)                 │
│                                                       │
│  On didChange:                                        │
│    db.set_file_text(file_id, new_text)               │
│                                                       │
│  On diagnostics request:                              │
│    let diags = file_diagnostics(&db, file_id)        │
│    // Salsa figures out what to recompute             │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│              Salsa Database                           │
│         (new crate: tsz-incremental)                 │
│                                                       │
│  INPUTS (set by LSP):                                │
│    SourceFile { path, text }                         │
│                                                       │
│  TRACKED QUERIES (computed, cached, auto-invalidated):│
│    parsed_file(file) → ParsedFile                    │
│    bound_file(file) → BoundFile                      │
│    file_exports(file) → ExportSignature              │
│    file_imports(file) → Vec<FileId>                  │
│    file_diagnostics(file) → Vec<Diagnostic>          │
│                                                       │
│  PHASE 2 — declaration-level:                        │
│    file_declarations(file) → Vec<Declaration>        │
│    decl_type(decl) → TypeId                          │
│    decl_diagnostics(decl) → Vec<Diagnostic>          │
│                                                       │
│  INTERNED (shared, deduplicated):                    │
│    TypeId, Atom, etc. (via existing TypeInterner)    │
│                                                       │
└──────────────────────┬──────────────────────────────┘
                       │ calls into (unchanged)
┌──────────────────────▼──────────────────────────────┐
│         Existing Checker + Solver                    │
│    (Parser, Binder, CheckerState, TypeInterner)      │
│         No modifications needed initially            │
└─────────────────────────────────────────────────────┘
```

### Why mutable solver state is NOT a problem

The old experiment failed because it tried to make `is_subtype_of` a Salsa query — but
`SubtypeChecker` uses `&mut self` internally (recursion depth, cycle detection).

In this design, Salsa wraps the **checker**, not the solver internals:

1. Salsa query `file_diagnostics(file)` creates a fresh `CheckerState`
2. `CheckerState` creates fresh `SubtypeChecker`/`TypeEvaluator` per invocation
3. All mutable state is **ephemeral** — born and dies within one query execution
4. Same AST + same imports = same diagnostics. That's all Salsa requires.

The `TypeInterner` is **append-only** — old `TypeId`s remain valid across Salsa revisions.
It lives outside Salsa as a shared side-channel (same pattern rust-analyzer uses).

---

## How Incremental Invalidation Works

### Scenario 1: Edit inside a function body (most common — ~90% of edits)

```
User types "x." inside function foo() in app.ts

1. LSP sets file_text("app.ts") = new text
2. parsed_file("app.ts") → recomputes (parser: ~1ms for big files)
3. file_exports("app.ts") → Salsa compares: UNCHANGED (body edit doesn't affect exports)
4. file_diagnostics("other.ts") → NOT recomputed (its dependency, exports, didn't change)
5. file_diagnostics("app.ts") → recomputed (its AST changed)

Phase 2 (declaration-level):
5a. decl_diagnostics("app.ts", foo) → recomputed (foo's AST changed)
5b. decl_diagnostics("app.ts", bar) → NOT recomputed (bar's AST unchanged)
```

**Result**: Only re-check the one function that changed. Everything else is cached.

### Scenario 2: Edit a comment or whitespace

```
User adds a comment in app.ts

1. LSP sets file_text("app.ts") = new text
2. parsed_file("app.ts") → recomputes
3. Salsa compares parsed AST with previous: STRUCTURALLY IDENTICAL
4. ALL downstream queries: NOT recomputed

(AST comparison ignores trivia — comments, whitespace, formatting)
```

**Result**: Zero type checking work. Instant.

### Scenario 3: Edit an exported type

```
User changes `export type Foo = string` → `export type Foo = number` in types.ts

1. LSP sets file_text("types.ts") = new text
2. parsed_file → recomputes
3. file_exports("types.ts") → CHANGED (Foo's shape changed)
4. All files importing types.ts → file_diagnostics recomputed
5. Files NOT importing types.ts → untouched
```

**Result**: Only importers of the changed export re-check. Salsa tracks this automatically.

### Comparison with tsgo

| Scenario | tsgo | tsz + Salsa |
|----------|------|-------------|
| Open file (cold) | Fast (Go speed) | Similar (Rust speed) |
| Edit function body | Re-check entire file | Re-check 1 declaration |
| Edit comment | Re-check entire file | **Zero work** |
| Edit exported type | Re-check file + importers | Re-check affected decls only |
| Rapid typing (10 chars) | 10 full re-checks | 10 micro re-checks |
| 5000-line file, edit 1 fn | Check all 100 decls | Check ~1 decl (100x less) |

---

## Implementation Plan

### Phase 0: Clean Slate (1-2 days) — DONE

**Remove old Salsa experiment entirely.** See "Step 0" above.

- [x] Delete `salsa_db.rs`
- [x] Remove `salsa = "0.16"` dependency
- [x] Remove `experimental_salsa` feature flag
- [x] Clean up tests

### Phase 0.5: Export Signature Firewall — DONE

**Smart cache invalidation: only invalidate dependent files when the public API changes.**

- [x] Created `crates/tsz-lsp/src/export_signature.rs` with `ExportSignature` type
- [x] Computes position-independent hash of: direct exports, named re-exports, wildcard
  re-exports, global augmentations, module augmentations, and exported file_locals
- [x] Stored on `ProjectFile`, recomputed on every parse/bind update
- [x] `Project::update_file()` compares old vs new signature — only invalidates
  dependent files if the signature changed
- [x] Body-only edits, comment changes, and private symbol additions skip dependent
  invalidation entirely (verified by 10 unit + integration tests)

### Phase 1: File-Level Salsa Queries (1-2 weeks)

**Goal**: Replace the manual `DependencyGraph` + `diagnostics_dirty` system with Salsa.
File-level granularity — still re-checks entire files, but Salsa handles invalidation.

**New crate: `crates/tsz-incremental/`**

```rust
// Salsa 3.x API
#[salsa::input]
pub struct SourceFile {
    #[id]
    pub path: PathBuf,
    #[return_ref]
    pub text: String,
}

#[salsa::tracked]
pub struct ParsedFile<'db> {
    #[id]
    pub source: SourceFile,
    #[return_ref]
    pub arena: NodeArena,
    #[return_ref]
    pub parser_state: ParserState,
    pub root: NodeIndex,
}

#[salsa::tracked]
pub fn parsed_file(db: &dyn Db, source: SourceFile) -> ParsedFile<'_> {
    let text = source.text(db);
    let (arena, parser_state, root) = tsz_parser::parse(text);
    ParsedFile::new(db, source, arena, parser_state, root)
}

#[salsa::tracked]
pub fn bound_file(db: &dyn Db, parsed: ParsedFile<'_>) -> BoundFile<'_> {
    let binder = tsz_binder::bind(&parsed.arena(db), parsed.root(db));
    BoundFile::new(db, parsed, binder)
}

/// The "firewall" — export signature is what importers depend on.
/// If a body-only edit doesn't change exports, importers don't re-check.
#[salsa::tracked]
pub fn file_exports(db: &dyn Db, bound: BoundFile<'_>) -> ExportSignature {
    // Extract just the public API: exported names, their types, re-exports
    extract_exports(&bound.binder(db), &bound.parsed(db).arena(db))
}

#[salsa::tracked]
pub fn file_diagnostics(db: &dyn Db, source: SourceFile) -> Vec<Diagnostic> {
    let parsed = parsed_file(db, source);
    let bound = bound_file(db, parsed);
    
    // This is the key: reading file_exports of imported files creates
    // Salsa dependency edges. If those exports don't change, this
    // query won't re-execute even if the imported file's internals changed.
    let import_types = resolve_imports(db, &bound);
    
    // Call existing checker — no changes to checker needed
    let mut checker = CheckerState::new(
        &parsed.arena(db),
        &bound.binder(db),
        &query_cache,
        source.path(db).to_string(),
        options,
    );
    checker.check_source_file(parsed.root(db));
    checker.ctx.diagnostics.clone()
}
```

**What this replaces:**
- `DependencyGraph` in `crates/tsz-lsp/src/dependency_graph.rs` → Salsa tracks deps automatically
- `diagnostics_dirty` flag on `ProjectFile` → Salsa knows what's stale
- `get_stale_diagnostics()` loop → Salsa only recomputes affected queries
- Manual `type_cache` threading → Salsa handles caching

**What stays the same:**
- The entire checker (`CheckerState`, `check_source_file`)
- The entire solver (`TypeInterner`, `QueryCache`, `SubtypeChecker`)
- The parser and binder
- All LSP feature providers (hover, completions, etc.) — initially

**Key design decision — ExportSignature as the "firewall":**

The `file_exports` query is the critical optimization. It extracts a **structural summary**
of a file's public API. When an importer reads `file_exports(dep)`, Salsa records the
dependency. On the next revision, if `file_exports(dep)` returns the same value (despite
the dep's internals changing), the importer's queries are NOT invalidated.

This is the same pattern rust-analyzer uses with its "item tree" concept.

### Phase 2: Declaration-Level Queries (2-4 weeks)

**Goal**: Don't re-check an entire file when one function changes. Split type checking
into per-declaration units.

This requires:

1. **Declaration extraction**: Parse top-level declarations into individual units
2. **Per-declaration checking**: Run `CheckerState` on one declaration at a time
3. **Dependency tracking**: Track which declarations reference which other declarations
4. **Diagnostic aggregation**: Combine per-declaration diagnostics for the file

```rust
#[salsa::tracked]
pub struct Declaration<'db> {
    #[id]
    pub file: SourceFile,
    #[id] 
    pub index: u32,
    pub kind: DeclKind,
    pub name: Option<String>,
    pub node_range: (u32, u32),  // byte range in source
}

#[salsa::tracked]
pub fn file_declarations(db: &dyn Db, parsed: ParsedFile<'_>) -> Vec<Declaration<'_>> {
    // Walk top-level statements, create a Declaration for each
    extract_declarations(db, parsed)
}

#[salsa::tracked]
pub fn decl_diagnostics(db: &dyn Db, decl: Declaration<'_>) -> Vec<Diagnostic> {
    // Type-check just this declaration
    // Reads decl_type() of referenced declarations → Salsa tracks deps
    check_declaration(db, decl)
}

#[salsa::tracked]
pub fn file_diagnostics(db: &dyn Db, source: SourceFile) -> Vec<Diagnostic> {
    let parsed = parsed_file(db, source);
    let decls = file_declarations(db, parsed);
    // Aggregate: Salsa only re-runs decl_diagnostics for changed decls
    decls.iter()
        .flat_map(|d| decl_diagnostics(db, *d))
        .collect()
}
```

**Challenge**: The current `CheckerState` walks the entire AST top-down via
`check_source_file`. Splitting into per-declaration checking requires:
- A way to check a single declaration in isolation
- Resolving cross-declaration references (function A calls function B → need B's type)
- Handling declaration merging (interfaces, namespaces)

**Approach**: Don't rewrite the checker. Instead:
1. `check_source_file` stays as-is for CLI batch mode
2. New `check_declaration(arena, binder, root, decl_range)` function that runs the checker
   on a subtree
3. Cross-references resolved via `decl_type()` Salsa queries, which call the solver

### Phase 3: LSP Feature Queries (2-3 weeks)

**Goal**: Make hover, completions, go-to-definition incremental too.

```rust
#[salsa::tracked]
pub fn hover_at(db: &dyn Db, file: SourceFile, offset: u32) -> Option<HoverInfo> {
    let parsed = parsed_file(db, file);
    let bound = bound_file(db, parsed);
    // Find node at offset, compute hover
    compute_hover(&parsed.arena(db), &bound.binder(db), offset)
}

#[salsa::tracked] 
pub fn completions_at(db: &dyn Db, file: SourceFile, offset: u32) -> Vec<CompletionItem> {
    // ...
}
```

Currently, every hover/completion request creates a fresh `CheckerState`. With Salsa,
repeated hovers at the same position (common during typing) return cached results instantly.

### Phase 4: CLI Watch Mode (future)

Once the Salsa database is proven in LSP, `tsz --watch` can use it too:

```rust
// CLI watch mode
let mut db = IncrementalDatabase::new();
for file in project_files {
    db.set_file_text(file_id, fs::read_to_string(&file)?);
}
// Initial full check
let diags = all_diagnostics(&db);

// Watch loop
for change in file_watcher {
    db.set_file_text(change.file_id, fs::read_to_string(&change.path)?);
    let diags = all_diagnostics(&db);  // Salsa only re-checks what changed
}
```

---

## Technical Decisions

### Salsa Version: 3.x (latest)

- Current codebase has `salsa = "0.16"` — this is the old API with query groups
- Salsa 3.x uses `#[salsa::tracked]` / `#[salsa::input]` — cleaner, more ergonomic
- Better cycle handling (important for recursive types)
- Built-in durability system (mark stdlib as "high durability")
- This is what rust-analyzer uses in production

### TypeInterner: Shared Side-Channel (outside Salsa)

The `TypeInterner` stays as-is — append-only, `DashMap`-based, shared across all queries.
It does NOT go through Salsa because:
- Interning is monotonic (new types only get added, never removed)
- `TypeId` equality is just `u32` comparison — no invalidation needed
- Same pattern rust-analyzer uses for its interner
- Avoids the "mutable interner" problem that killed the old experiment

### New Crate: `tsz-incremental`

Separate crate to keep Salsa isolated from the checker/solver:
- `crates/tsz-incremental/Cargo.toml` — depends on `salsa`, `tsz-parser`, `tsz-binder`, `tsz-checker`, `tsz-solver`
- The LSP crate depends on `tsz-incremental`
- The CLI crate does NOT depend on it (for now — batch mode doesn't need Salsa)

### LSP-Only Initially, Converge Later

- Phase 1-3: LSP only. CLI batch mode unchanged.
- Phase 4: CLI watch mode uses the same Salsa database.
- CLI `tsz check` (one-shot): Never needs Salsa — full checking from scratch is fine.

---

## Risks and Mitigations

### Risk 1: Salsa 3.x API stability
- **Concern**: Salsa 3.x API may change
- **Mitigation**: rust-analyzer depends on it in production. It's stable enough.
- **Mitigation**: Isolate in `tsz-incremental` crate — if API changes, only one crate updates.

### Risk 2: ExportSignature design is hard to get right
- **Concern**: If `file_exports` is too coarse, everything invalidates. Too fine, we miss changes.
- **Mitigation**: Start with a simple structural hash of exported declarations. Iterate.
- **Mitigation**: Measure with real-world projects — what % of edits trigger cross-file re-checks?

### Risk 3: Declaration-level splitting is a big refactor
- **Concern**: Phase 2 requires splitting `check_source_file` into per-declaration checking
- **Mitigation**: Phase 1 (file-level) ships first and already provides value
- **Mitigation**: Phase 2 can be incremental — start with functions/classes, add more decl types over time

### Risk 4: Memory overhead from Salsa's memo tables
- **Concern**: Salsa caches all query results — could use significant memory
- **Mitigation**: Salsa has built-in GC (`sweep` / LRU eviction)
- **Mitigation**: Measure memory usage with large projects (10K+ files) early

### Risk 5: TypeInterner growth across revisions
- **Concern**: Append-only interner means types from old revisions stick around
- **Mitigation**: Types are small (u32 IDs, shapes behind Arc). Growth is bounded.
- **Mitigation**: If it becomes a problem, add periodic compaction (copy live types to new interner)

---

## Gemini Review Findings (2026-02-07)

Conducted 10 parallel Gemini Pro reviews covering LSP architecture, checker splitting,
TypeInterner safety, ExportSignature design, parser performance, QueryCache interaction,
dependency graph replacement, old experiment removal, cross-file resolution, and overall
alignment with NORTH_STAR.md.

### CRITICAL: AST Cannot Be the Invalidation Firewall

**The original plan assumed Salsa could compare old AST vs new AST and skip downstream
queries when they're structurally identical. This is WRONG.**

`Node` stores absolute positions (`pos`, `end`). A single character insertion shifts every
subsequent node's positions. Salsa will consider the AST "changed" on every keystroke, even
for whitespace/comment edits.

**Fix**: The parser layer CANNOT stop invalidation. The **ExportSignature** (from the binder)
is the true firewall. It must be position-independent — based on symbol names, types, and
declaration structure, NOT byte offsets.

**Updated Scenario 2 (comment edit)**:
```
1. file_text changes
2. parsed_file recomputes (positions shifted — Salsa sees it as "changed")
3. bound_file recomputes (binder reruns)
4. file_exports → Salsa compares: UNCHANGED (same exported names/types)
5. file_diagnostics("other.ts") → NOT recomputed (export firewall held)
6. file_diagnostics("app.ts") → DOES recompute (its bound_file changed)

Only Phase 2 (declaration-level) avoids re-checking the whole file on comment edits.
```

### CRITICAL: DefId/SymbolRef Stability (ABA Problem)

If the binder re-runs and reuses `DefId(50)` for a different symbol than before, the
TypeInterner returns a stale cached `TypeId` for the old symbol. This is an ABA problem.

**Solutions (pick one)**:
- **Global unique DefIds**: Use `salsa::interned` for definitions (not types), ensuring
  DefIds are unique across the entire LSP session lifetime
- **Generation tagging**: Include a revision counter in `TypeKey::Ref(DefId, generation)`
- **Scoped interner**: Types containing DefIds go in a per-query scoped interner (MSB=1),
  global types (intrinsics, structural) go in the global interner (MSB=0). Drop scoped
  types when the query finishes.

The codebase already has comments about a scoped interner split (`intern.rs` line 527).
**This must be resolved before Phase 1 ships.**

### QueryCache: Complementary, Not Redundant

Salsa = L2 cache (file/declaration level: what to re-check).
QueryCache = L1 cache (hot-loop solver operations: is_subtype_of, evaluate_type).

**Design**: Create a fresh `QueryCache` per Salsa query. Drop it when the query returns.
Do NOT share across Salsa revisions — stale SymbolId mappings could cause correctness bugs.

### ExportSignature: Binder Already Has the Data

`BinderState` already tracks everything needed:
- `module_exports` — direct exports (name → symbol table)
- `reexports` — named re-exports (name → source module + original name)
- `wildcard_reexports` — `export * from` targets
- `module_augmentations` — `declare module` contributions
- `global_augmentations` — `declare global` contributions

`ExportSignature` should extract these fields, position-independently. The binder's
`populate_module_exports_from_file_symbols` is the starting point.

**Edge case**: `export * from './b'` means A's exports depend on B's exports. The
`file_exports(A)` query must read `file_exports(B)`, creating a Salsa dependency edge.

**Edge case**: `declare global { ... }` changes invalidate ALL files in the project.
Mark global augmentations carefully.

### Checker Splitting: Phase 1.5 Needed

Before Phase 2, refactor `CheckerContext` to stop using shared mutable state:
1. Make `check_statement` return `Vec<Diagnostic>` instead of pushing to `ctx.diagnostics`
2. Split `TypeEnvironment` building from eager (all symbols) to lazy (on-demand)
3. Extract global checks (`check_duplicate_identifiers`, `check_unused_declarations`)
   into standalone file-level passes

**Hard parts identified**:
- Diagnostic aggregation (currently push-based → must become return-value-based)
- TypeEnvironment is built eagerly per-file → must become lazy/per-declaration
- Global checks (duplicate identifiers, unused declarations) are inherently file-level
- Module-level bare statements are the file's "constructor" — treat as one declaration

### Cross-File: FS Must Be a Salsa Input

Module resolution uses `std::fs` calls (not pure). In Salsa queries, all FS access must
go through inputs:
- `file_exists(path) → bool` (input)
- `file_text(path) → String` (input)
- `compiler_options` (input — changing `paths` invalidates all resolution)
- `package_json(path)` (input — changing `exports` invalidates that package)

The current `merge_bind_results` (stop-the-world global merge) is incompatible with Salsa.
Replace with: `file_exports(id)` per-file lazy queries. Symbol identity becomes
`(FileId, LocalSymbolId)` instead of remapped global IDs.

### Global Search Features: Keep SymbolIndex

Find References and Rename are "global search" operations that touch every file. They
cannot be pure Salsa queries. Keep `SymbolIndex` as a separate structure, updated when
Salsa recomputes `file_symbols` queries.

### "Sub-millisecond" Goal Needs Adjustment

Parser + binder for a 5K-line file likely takes 5-20ms. The floor for Phase 1 is ~20ms
for body edits (full file reparse + rebind + recheck). True sub-millisecond requires
Phase 2 (declaration-level) + eventually incremental parsing.

**Adjusted targets**:
- Phase 1: ~20-50ms for body edits (vs current ~50-200ms)
- Phase 2: ~1-10ms for body edits (only recheck changed declaration)
- Phase 2 + incremental parsing: potentially sub-1ms

### Old Experiment: Safe to Remove, Preserve Two Insights

1. **SubtypeConfig** — fine-grained config grouping prevents irrelevant flag changes from
   invalidating type checks. Reuse this concept in the new design.
2. **Coinductive cycle recovery** — `is_subtype_of_recover` returns `true` (greatest fixed
   point). If Salsa ever sees recursive type queries, this recovery strategy is required.

### Overall Alignment: "Strongly Aligned" with NORTH_STAR

The hybrid approach (Salsa at the top, standard Rust below) creates less architectural
debt than "Salsa everywhere." The `tsz-incremental` crate structure is sound. The plan
correctly preserves the Solver-First and Thin Wrappers principles.

---

## Success Metrics

| Metric | Current | Phase 1 Target | Phase 2 Target |
|--------|---------|-----------------|-----------------|
| Comment edit → diagnostics | Full file re-check | Zero re-check | Zero re-check |
| Body edit → diagnostics | Full file re-check | Full file re-check | 1 decl re-check |
| Export edit → cross-file | All dependents re-check | Only dependents re-check | Only affected decls |
| Time to diagnostics (body edit, 5K line file) | ~50-200ms | ~50-200ms | ~1-10ms |
| Memory per open file | TypeCache + QueryCache | + Salsa memo tables | + per-decl caches |

---

---

## Critical Review Round 2 — "Tear It Apart" (2026-02-07)

Conducted 10 parallel Gemini Pro reviews with instructions to be brutally critical.
These findings fundamentally challenge the plan. Some require rethinking, others require
accepting trade-offs. Nothing here is insurmountable, but the plan as written underestimates
the difficulty by at least 3-5x.

### SHOWSTOPPER: SymbolId Instability Destroys the Firewall

The entire ExportSignature firewall concept is broken at a fundamental level.

**The problem**: `SymbolId` is a sequential u32 index into a vector. The binder assigns IDs
in AST traversal order. Adding a private variable BEFORE an export shifts every subsequent
SymbolId.

```
Version A: export const X = 1;       → X gets SymbolId(0)
Version B: const z = 0; export const X = 1;  → X gets SymbolId(1)
```

`module_exports` contains `HashMap<String, SymbolId>`. The SymbolId for X changed even
though the export didn't semantically change. Salsa sees the ExportSignature as "changed"
→ all importers re-check → firewall is useless.

**Also broken**: `Symbol.declarations` contains `Vec<NodeIndex>`, and NodeIndex values shift
when the AST changes. The binder output is saturated with position-dependent data.

**Required fix**: ExportSignature CANNOT be derived from BinderState directly. It must be a
**new, separate abstraction** that:
1. Canonicalizes identifiers by name (not SymbolId)
2. Strips all positions (no NodeIndex)
3. Filters out non-exported symbols entirely
4. Represents type shapes structurally (not via TypeId which depends on checking)

This is non-trivial. It's essentially a "public API fingerprint" that must be position-
and ID-independent. **This is the hardest design problem in the entire plan.**

### SHOWSTOPPER: Inferred Exports Break the Firewall Concept

TypeScript allows exports without type annotations. The export's type is *inferred from
the body*.

```typescript
export function foo() {
    return calculateSomethingComplex(); // return type inferred
}
```

To know the ExportSignature of `foo`, you must type-check its body. But the firewall's
purpose is to AVOID type-checking when only the body changed. This is circular.

**Impact**: For annotated exports (`export function foo(): string`), the firewall works —
the annotation is the signature. For inferred exports, any body change potentially changes
the signature, defeating the firewall for those declarations.

**Mitigation options**:
- Only use the firewall for explicitly-annotated exports (common in library code)
- Use the declaration AST shape (params + body structure hash) as a conservative fingerprint
  (re-check importers if anything in the function changes, not just the signature)
- Encourage `isolatedDeclarations` mode which requires explicit annotations on all exports

### CRITICAL: The LSP Currently Does NOT Do Multi-File Type Checking

The plan assumes it's "replacing" an existing multi-file system. **It's not.**

Current `ProjectFile.get_diagnostics()` creates a `CheckerState` with ONLY:
- The file's own parser arena
- The file's own binder
- A per-file QueryCache

**There is no cross-file resolution in the current LSP.** Each file is checked in complete
isolation. The `DependencyGraph` exists only for cache invalidation, not for type resolution.

**Implication**: The Salsa migration isn't a refactor — it's implementing multi-file type
checking for the first time. The plan's time estimates (1-2 weeks for Phase 1) assume
existing infrastructure that doesn't exist.

### CRITICAL: `check_source_file` Has 5 Mandatory Setup Steps

Per-declaration checking (Phase 2) cannot skip the file-level setup:

1. **Pragma parsing** — scans source for `@ts-nocheck`, mutates compiler_options
2. **Cache clearing** — clears per-file memoization from previous runs
3. **build_type_environment()** — eagerly resolves ALL symbols in the file (O(file_size))
4. **Flow analysis setup** — copies environment for FlowAnalyzer
5. **register_boxed_types()** — registers String/Number/Boolean interfaces from lib.d.ts

Without ALL of these, even simple code like `"hello".length` fails because the checker
doesn't know which TypeId corresponds to the `String` interface.

**Concrete failure example**: A file with `// @ts-nocheck` at the top. Per-declaration
checking without pragma parsing would report errors that should be suppressed.

**Required**: A `file_context(file_id)` query that runs these 5 setup steps. Every
`decl_diagnostics` query depends on it. This means the "100x less work" claim is wrong —
you always pay the O(file_size) setup cost.

### CRITICAL: TypeInterner Memory — Will Hit 5M Limit Mid-Day

Gemini calculated: at 60 WPM typing speed, ~10 ephemeral types per keystroke, an 8-hour
session generates ~14.4M types. The MAX_INTERNED_TYPES limit is 5M.

**What happens at 5M**: `intern()` returns `TypeId::ERROR`. The compiler enters a zombie
state. No crash, no recovery — just wrong results everywhere. User must restart the LSP.

**The interner has no GC, no eviction, no compaction.** Types from old revisions
(intermediate union normalization steps, old inferred types from previous edits) persist
forever.

**Estimated memory at capacity**: ~600MB just for type handles (120 bytes per type × 5M),
plus secondary data (object shapes, strings) pushing toward ~1GB.

### CRITICAL: QueryCache SHOULD Be Shared (Plan Gets This Wrong)

The plan says "create fresh QueryCache per Salsa query." Gemini argues this is a
**performance anti-pattern**.

`QueryCache` stores `is_subtype_of(A, B) → bool`. If TypeIds are globally stable (which
they are), these results are eternally valid. Throwing away the cache means re-proving
`React.Element <: React.Node` thousands of times per file.

`RelationCacheKey` already includes feature flags (strict_null_checks etc.) in the key,
so mixed-config projects don't poison the cache.

**Corrected design**: QueryCache should be a long-lived structure owned by the Salsa
Database, shared across all queries. Only clear it on major events (e.g., lib.d.ts change,
config change). SubtypeChecker (mutable, per-check) remains ephemeral.

### MAJOR: Module Resolution Has 20-50 FS Calls Per Import

`module_resolver.rs` makes ~20-50 `std::fs` calls per import resolution (file exists,
directory exists, read package.json, walk up directories). Modeling each as a Salsa input
creates a dependency graph explosion.

**The "walking up" problem**: Node resolution checks `./node_modules`, `../node_modules`,
`../../node_modules`, etc. Each non-existent directory is a "dependency on absence" —
creating node_modules in any parent directory invalidates resolution.

**The `npm install` problem**: Running `npm install` changes thousands of files in
node_modules. With fine-grained Salsa inputs, this triggers an "invalidation storm."
The plan has no strategy for this.

**Required**: Coarse-grained resolution inputs. Don't track individual file existence.
Track `ResolvePackage(name, from_dir) → path` as an opaque query. Need a Virtual File
System abstraction layer.

### MAJOR: Global Augmentations Invalidate Everything

`declare global { ... }` and ambient .d.ts declarations affect all files without being
imported. A "pull-based" Salsa model (B imports A, so B depends on A) fundamentally fails
for globals — file B depends on file A's globals without knowing A exists.

**Required**: A `global_scope(project_id)` query that aggregates ALL global augmentations.
Every file depends on it. Changing ANY file with global declarations invalidates ALL files.
This is correct behavior, but makes global augmentations a performance cliff.

### MAJOR: Barrel Files Are a Performance Cliff

`export * from './leaf'` means the barrel file's exports depend on the leaf's exports.
In large projects using barrel files extensively (very common), changing a leaf file
invalidates the barrel, which invalidates everything importing the barrel.

Salsa's "early cutoff" helps (if leaf's export types didn't change, barrel doesn't
invalidate) but only for non-structural changes. Changing an inferred return type in a
leaf function propagates through the entire barrel chain.

### The Alternative: "Don't Do Salsa At All"

Gemini's strongest critique: **The simpler approach might be better.**

```
1. Keep the batch compiler (reparse + rebind + recheck entire file — it's fast in Rust)
2. After checking, compute a structural hash of the file's exports
3. If the hash changed, re-check dependent files
4. If the hash didn't change, done
```

This is essentially the current `DependencyGraph` + `diagnostics_dirty` system, but with
a smarter invalidation check (structural hash instead of "any change = dirty").

**Advantages over Salsa**:
- Works with Arena-based architecture (no stable ID requirement)
- No query framework overhead
- No memory overhead from Salsa's memo tables
- Simpler to implement and debug
- 2 weeks instead of 3 months

**Disadvantages**:
- No declaration-level granularity (always re-check whole file)
- No caching of intermediate results across revisions
- Still O(file_size) for each changed file

**The question**: Is declaration-level granularity worth the 10x implementation complexity?
For most TypeScript files (< 2000 lines), whole-file re-checking in Rust might be fast
enough that declaration-level granularity has zero perceived benefit.

---

---

## Benchmark Results (2026-02-07)

Measured on synthetic TypeScript files with realistic declaration mix (functions, classes,
interfaces, type aliases, enums). No lib.d.ts loaded. Apple Silicon.

| File Size | Parse | Parse+Bind | Parse+Bind+Check | Check alone |
|-----------|-------|-----------|-------------------|-------------|
| 330 lines (25 decls) | 90µs | 140µs | 670µs | ~530µs |
| 660 lines (50 decls) | 180µs | 280µs | 1.3ms | ~1.0ms |
| **1,322 lines (100 decls)** | **360µs** | **549µs** | **2.5ms** | **~1.9ms** |
| **2,642 lines (200 decls)** | **765µs** | **1.2ms** | **4.6ms** | **~3.4ms** |
| **5,282 lines (400 decls)** | **1.6ms** | **2.5ms** | **8.6ms** | **~6.1ms** |

Cold check (pre-parsed/pre-bound, 1322 lines): **1.45ms**

**Key insight**: Checking is ~60-70% of time. Parse+bind is ~30-40%.
Full pipeline for a 5K line file: **8.6ms**. For 2.6K lines: **4.6ms**.
These are already under 16ms (60fps threshold) for most real-world files.

---

## Revised Recommendation (Post-Benchmark, Post-Review)

After 20 Gemini Pro reviews, benchmark data, and critical analysis, the consensus is:

### Don't do Salsa (yet). Do "Smart Brute Force."

**The data says**: Rust is fast enough to re-check entire files. 8.6ms for 5K lines.
The problem isn't intra-file speed — it's cross-file cascading re-checks.

### The Strategy: Raw Speed + Structural Hashing

**Months 1-2: Make the raw checker faster**
- Fix O(n²) canonicalizer, optimize subtype hot path
- Goal: get 5K line benchmark from 8.6ms to ~5ms
- This benefits BOTH CLI and LSP
- All work here is permanent value — never wasted regardless of future architecture

**Month 3: Solve the data structure prerequisites**
- Implement **StableDefId** — cross-file references must use `(FileId, Name)`, not SymbolId
- Implement **ExportSignature** — position-independent hash of a file's public API
- These are required for ANY incremental strategy (Salsa or manual)

**Months 4-5: "Phase 1.5" — Manual file-level incrementality**
- When file B changes: reparse + rebind + recheck B (takes <10ms, always pay this)
- Compute new ExportSignature, compare with old
- If identical: STOP. Don't re-check importers. (This is the 80/20 win)
- If changed: re-check dependent files using existing DependencyGraph
- No Salsa needed. Just structural hashing + the existing dependency graph.

**Month 6: Evaluate**
- Is the LSP responsive enough? If <100ms for all operations, stop here.
- If large monorepos still lag, NOW consider Salsa for the orchestration layer.
- By this point, StableDefId + ExportSignature make Salsa viable without a rewrite.

### Also: Scoped TypeInterner (Month 3-4)

The 5M type limit will crash the LSP in long sessions. Implement the scoped interner:
- Global interner (MSB=0): intrinsics, lib.d.ts types, exported types — permanent
- Local interner (MSB=1): ephemeral types from checking function bodies — dropped per query
- Code scaffolding already exists in `intern.rs` (line 527 MSB convention)

### The Philosophy

"Having time to do things right" means:
1. Make the foundation fast (raw checker performance)
2. Build the right abstractions (StableDefId, ExportSignature, ScopedInterner)
3. Layer on incrementality only when data proves it's needed
4. Keep Salsa as an option, not a commitment

---

## Open Questions

### Resolved by Gemini Review

1. **ExportSignature format**: ~~What exactly goes in it?~~
   **RESOLVED**: Extract `module_exports`, `reexports`, `wildcard_reexports`,
   `module_augmentations`, `global_augmentations` from `BinderState`. Must be
   position-independent. For inferred types (no annotation), include the declaration
   AST node structure (excluding bodies) as a structural fingerprint.

2. **Incremental parsing integration**: ~~Should parsed_file use it?~~
   **RESOLVED**: Full reparse for Phase 1. The parser is fast enough (~5-20ms for large
   files). Incremental parsing fights against Salsa's pure-function model. Revisit only
   if parsing becomes a measured bottleneck in Phase 2+.

3. **Salsa's `#[salsa::interned]`**: ~~Should we use it for TypeId/Atom?~~
   **RESOLVED**: No for types (DashMap is 5-10x faster for millions of operations). No
   for strings (existing ShardedInterner is fine). YES for `DefId`s — use
   `salsa::interned` for definitions to solve the ABA stability problem.

### Still Open

4. **DefId stability strategy**: Three options identified (global unique DefIds, generation
   tagging, scoped interner). Need to prototype and measure. The scoped interner approach
   has code scaffolding already (`intern.rs` line 527 MSB convention). This is blocking
   for correctness.

5. **Thread safety**: Salsa 3.x supports parallel query execution via `snapshot()`. Our
   `TypeInterner` already uses `DashMap` (thread-safe). But `CheckerState` is single-threaded
   (`&mut self`). Phase 1 runs queries serially. Parallel queries would require making checker
   state thread-safe or using per-thread instances.

6. **How to handle `lib.d.ts`**: Standard library types are loaded once and never change.
   Use Salsa's durability system: `db.set_file_text_with_durability(lib_file, text, Durability::HIGH)`.
   This tells Salsa to skip re-validating lib queries entirely.

7. **`declare global` invalidation scope**: Changes to global augmentations technically
   invalidate every file. Need a strategy to limit the blast radius (perhaps a separate
   `global_scope` query that aggregates all global augmentations, so files depend on the
   aggregate rather than individual contributor files).

8. **Module resolution purity**: `module_resolver.rs` uses `std::fs` calls. Must be
   replaced with Salsa inputs (`file_exists`, `package_json`). This is a significant
   refactor of the resolver. Scope it separately — possibly Phase 1.5.

9. **Parser benchmark suite**: Need to establish actual parse times for 1K/5K/10K line
   files to validate the "full reparse is fast enough" assumption. Create criterion
   benchmarks before starting Phase 1.
