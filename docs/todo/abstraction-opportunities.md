# Abstraction Opportunities

Tasks to reduce repetitive manual patterns across the codebase. Companion to `code-quality.md`.

See also: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md` (Phase 4: Checker Cleanup).

---

## 1. Eliminate TypeKey Match Statements in Checker

**Priority**: High
**Impact**: Architecture compliance, maintainability
**Occurrences**: 75+ `TypeKey::` matches in Checker, ~30+ across 18 files in Solver
**Status**: Todo — blocked on Judge classifier API (Phase 4 of Solver Refactoring Proposal)

### Problem

The architecture mandates: *"Checker NEVER inspects type internals."*

Reality: 75+ instances of `TypeKey::` matching in the Checker, plus ~30 large exhaustive match statements across 18 files in the Solver. Every new `TypeKey` variant requires touching all of them. The Checker is doing the Judge's job — branching on type structure instead of asking the Judge to classify types.

### Affected files (Checker)

- `crates/tsz-checker/src/error_reporter.rs` — match on type structure for diagnostics
- `crates/tsz-checker/src/state_checking_members.rs` — match on member types
- `crates/tsz-checker/src/type_computation.rs` — match on type for property access
- `crates/tsz-checker/src/type_computation_complex.rs` — match on type for resolution
- `crates/tsz-checker/src/type_checking.rs` — match on type for validation
- `crates/tsz-checker/src/declarations.rs` — match on declaration types
- `crates/tsz-checker/src/statements.rs` — statement kind dispatch (~25 arms)

### Affected files (Solver)

- `crates/tsz-solver/src/visitor.rs` — `for_each_child` with ~30 arms
- `crates/tsz-solver/src/subtype.rs` — multiple large matches
- `crates/tsz-solver/src/type_queries.rs` — multiple matches
- `crates/tsz-solver/src/format.rs` — type formatting match
- `crates/tsz-solver/src/evaluate.rs` — evaluation dispatch

### Solution

Per the Solver Refactoring Proposal, expose **classifier queries** from the Judge so the Checker never needs to match on `TypeKey`:

```rust
// Instead of matching TypeKey in Checker:
enum IterableKind { Array(TypeId), Tuple, String, IteratorObject, AsyncIterable, NotIterable }
fn classify_iterable(db: &dyn Judge, t: TypeId) -> IterableKind;

enum CallableKind { Function, Constructor, Overloaded(u32), NotCallable }
fn classify_callable(db: &dyn Judge, t: TypeId) -> CallableKind;

fn classify_primitive(db: &dyn Judge, t: TypeId) -> PrimitiveFlags;
fn classify_truthiness(db: &dyn Judge, t: TypeId) -> TruthinessResult;
```

### Steps

1. Audit all `TypeKey::` matches in `crates/tsz-checker/src/` — categorize by what they branch on
2. Design classifier enums that cover all Checker branching needs
3. Implement classifier queries in the Judge/Solver
4. Replace each Checker `TypeKey::` match with a classifier call
5. Gate remaining Solver matches behind a visitor trait or macro (for `for_each_child`, `format`, etc.)

### Target

Zero `TypeKey::` matches in Checker. Solver matches contained to visitor/format infrastructure only.

---

## 2. Arena Node Access Helpers

**Priority**: High
**Impact**: Code volume reduction, readability
**Occurrences**: ~300 across LSP, emitter, transforms, checker

### Problem

The pattern `if let Some(node) = arena.get(idx) { ... }` is repeated hundreds of times across the codebase. Often combined with kind checking, child extraction, and fallthrough on `None`.

### Affected areas

- `src/lsp/definition.rs` — ~15 occurrences
- `src/lsp/completions.rs` — ~15 occurrences
- `src/lsp/signature_help.rs` — ~10 occurrences
- `src/emitter/module_emission.rs` — ~15 occurrences
- `src/transforms/async_es5_ir.rs` — ~15 occurrences
- `crates/tsz-checker/src/state_checking_members.rs` — ~20 occurrences
- And 20+ more files

### Solution

Extension trait on `NodeArena`:

```rust
trait NodeArenaExt {
    fn with_node<F, T>(&self, idx: NodeIndex, f: F) -> Option<T>
        where F: FnOnce(&Node) -> T;

    fn with_node_of_kind<F, T>(&self, idx: NodeIndex, kind: SyntaxKind, f: F) -> Option<T>
        where F: FnOnce(&Node) -> T;

    fn get_child_text<'a>(&'a self, idx: NodeIndex, child: ChildField) -> Option<&'a str>;
}
```

---

## 3. Test Setup Boilerplate

**Priority**: High (easy win)
**Impact**: Developer velocity, consistency
**Occurrences**: ~200 across solver test files

### Problem

Nearly every test in `crates/tsz-solver/src/tests/` starts with the same `TypeInterner::new()` + `SubtypeChecker::new(...)` or `InferenceContext::new(...)` setup.

### Affected files

- `tests/subtype_tests.rs` — 100+ occurrences
- `tests/infer_tests.rs` — 50+ occurrences
- `tests/operations_tests.rs` — 30+ occurrences
- `tests/narrowing_tests.rs` — throughout
- `tests/union_tests.rs` — throughout

### Solution

Test fixture macro:

```rust
macro_rules! solver_test {
    ($name:ident, |$db:ident| $body:block) => {
        #[test]
        fn $name() {
            let $db = TestDb::new();
            $body
        }
    };
}
```

Or a `TestDb` struct that bundles `TypeInterner`, `SubtypeChecker`, `InferenceContext`, and common assertion helpers.

---

## 4. Diagnostic Emission Patterns

**Priority**: High
**Impact**: Maintainability, tsc error parity
**Occurrences**: ~50 across checker and solver

### Problem

Error construction is verbose and repetitive. Two major locations have near-identical large match arms mapping `SubtypeFailureReason` variants to diagnostics. Error message strings are duplicated across 13+ files (see also `code-quality.md` § Diagnostic Message Deduplication).

### Affected files

- `crates/tsz-solver/src/diagnostics.rs` (lines 710–1268) — `to_diagnostic` with ~20 arms
- `crates/tsz-checker/src/error_reporter.rs` (lines 200–578) — `render_failure_reason` with ~30 arms
- `crates/tsz-checker/src/state_checking_members.rs` — 50+ `error_at_node` calls

### Solution

Fluent diagnostic builder + centralized message constants:

```rust
// Centralized messages (mirrors TypeScript's diagnosticMessages.json)
pub const TS2322: DiagnosticTemplate = DiagnosticTemplate {
    code: 2322,
    category: DiagnosticCategory::Error,
    message: "Type '{0}' is not assignable to type '{1}'.",
};

// Builder API
self.diagnostic(TS2322)
    .arg(source_name)
    .arg(target_name)
    .at(node_idx)
    .emit();
```

### Steps

1. Create `DiagnosticTemplate` constants for all used error codes
2. Build a `DiagnosticBuilder` with `.arg()`, `.at()`, `.related_info()`, `.emit()` methods
3. Migrate checker `error_at_node` calls to builder pattern
4. Merge the duplicated `SubtypeFailureReason` → diagnostic conversion logic

---

## 5. Type Resolution / Unwrapping

**Priority**: Medium-High
**Impact**: Correctness, maintainability
**Occurrences**: ~50 across checker and solver

### Problem

Resolving `Lazy(DefId)`, `Ref(SymbolRef)`, and handling `Application` types is done ad-hoc everywhere with similar loops, visited-set tracking, and recursive unwrapping.

### Affected files

- `crates/tsz-checker/src/type_computation_complex.rs` — `resolve_ref_type`
- `crates/tsz-checker/src/state_type_environment.rs` — `resolve_lazy_type` with visited set
- `crates/tsz-checker/src/type_computation.rs` — `resolve_type_for_property_access`
- `crates/tsz-checker/src/type_checking_utilities.rs` — `resolve_type_for_property_access_simple`
- `crates/tsz-solver/src/compat.rs` — Lazy/Ref resolution in visitor

### Solution

A single `resolve_fully` helper:

```rust
fn resolve_fully(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    // Handles Lazy -> DefId body resolution
    // Handles Ref -> SymbolRef resolution
    // Handles Application -> instantiation
    // Tracks visited set to prevent infinite loops
    // Returns the fully concrete type
}
```

This will largely be superseded by the Judge query architecture, but is valuable as an interim cleanup.

---

## 6. LSP Handler Initialization

**Priority**: Medium
**Impact**: Maintainability, reduced LSP boilerplate
**Occurrences**: ~15 handlers

### Problem

Every LSP handler repeats the same setup: position conversion → node finding → symbol resolution → checker creation with cache handling.

### Affected files

- `src/lsp/hover.rs` — `get_hover_internal`
- `src/lsp/signature_help.rs` — `get_signature_help_internal`
- `src/lsp/completions.rs` — `get_completions`
- `src/lsp/definition.rs` — `get_definition`
- `src/lsp/references.rs` — `find_references`

### Solution

An `LspContext` struct that encapsulates common initialization:

```rust
struct LspContext<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    interner: &'a TypeInterner,
    source_text: &'a str,
    file_name: String,
}

impl<'a> LspContext<'a> {
    fn resolve_at_position(&self, pos: Position, cache: &mut Option<TypeCache>) -> Option<SymbolId>;
    fn get_type_at_position(&self, pos: Position, cache: &mut Option<TypeCache>) -> Option<TypeId>;
    fn create_checker(&self, cache: Option<TypeCache>) -> CheckerState;
}
```

Also consolidate the `ScopeWalker` resolve pattern into:

```rust
impl ScopeWalker {
    fn resolve_node_auto(&mut self, root: NodeIndex, idx: NodeIndex,
                         cache: Option<&mut ScopeCache>) -> Option<SymbolId>;
}
```

---

## 7. WASM API Flag Getters

**Priority**: Medium (easy win)
**Impact**: Code volume, maintainability
**Occurrences**: ~70 trivial methods

### Problem

`src/wasm_api/types.rs`, `type_checker.rs`, `source_file.rs`, and `program.rs` are full of trivial `pub fn is_*` / `pub fn get_*` methods that just delegate to an inner field or check a flag.

### Solution

Macro for flag-based checks:

```rust
macro_rules! wasm_flag_getters {
    ($($name:ident => $flag:expr),* $(,)?) => {
        $(
            #[wasm_bindgen(getter)]
            pub fn $name(&self) -> bool {
                self.flags.contains($flag)
            }
        )*
    };
}
```

---

## 8. AST Kind Predicate Functions

**Priority**: Medium (easy win)
**Impact**: Code volume
**Occurrences**: ~40 functions

### Problem

`src/wasm_api/ast.rs` has 30+ `pub fn is_*` functions that all do `kind == SyntaxKind::X as u16`.

### Solution

```rust
macro_rules! define_kind_predicates {
    ($($name:ident => $kind:ident),* $(,)?) => {
        $(
            #[wasm_bindgen]
            pub fn $name(kind: u16) -> bool {
                kind == SyntaxKind::$kind as u16
            }
        )*
    };
}

define_kind_predicates! {
    is_identifier => Identifier,
    is_string_literal => StringLiteral,
    is_function_declaration => FunctionDeclaration,
    // ...
}
```

---

## 9. Builder `with_*` Methods

**Priority**: Low-Medium
**Impact**: Boilerplate reduction
**Occurrences**: ~50 methods across 10+ structs

### Problem

Many structs have 3–10 `pub fn with_X(mut self, x: T) -> Self { self.x = x; self }` methods.

### Affected files

- `crates/tsz-solver/src/subtype.rs` — 5+ methods
- `crates/tsz-solver/src/def.rs` — 5+ methods
- `crates/tsz-solver/src/diagnostics.rs` — 4+ methods
- `crates/tsz-solver/src/tracer.rs` — 3 methods
- `src/lsp/completions.rs` — 10+ methods

### Solution

```rust
macro_rules! builder_setters {
    ($($field:ident: $ty:ty),* $(,)?) => {
        $(
            pub fn $field(mut self, $field: $ty) -> Self {
                self.$field = $field;
                self
            }
        )*
    };
}
```

---

## 10. `From` Impls for Enum Variants

**Priority**: Low
**Impact**: Boilerplate reduction
**Occurrences**: ~20

### Problem

`crates/tsz-solver/src/diagnostics.rs` has 6 identical `impl From<T> for DiagnosticArg` blocks. Similar patterns elsewhere.

### Solution

```rust
macro_rules! impl_from_variants {
    ($target:ty; $($source:ty => $variant:ident),* $(,)?) => {
        $(impl From<$source> for $target {
            fn from(v: $source) -> Self { Self::$variant(v) }
        })*
    };
}

impl_from_variants! {
    DiagnosticArg;
    TypeId  => Type,
    SymbolId => Symbol,
    Atom    => Name,
    String  => Text,
    usize   => Count,
}
```

---

## 11. Checker Symbol/Scope Lookup Chains

**Priority**: Medium
**Impact**: Readability, error handling
**Occurrences**: ~100 across checker files

### Problem

Repeated patterns of `arena.get()` → `get_*()` → extract data → check for `None` with early returns, forming deeply nested chains.

### Affected files

- `crates/tsz-checker/src/state_checking_members.rs` — ~40 occurrences
- `crates/tsz-checker/src/type_checking.rs` — ~20 occurrences
- `crates/tsz-checker/src/declarations.rs` — ~15 occurrences
- `crates/tsz-checker/src/error_reporter.rs` — ~25 occurrences

### Solution

Helper methods on `CheckerContext`:

```rust
impl CheckerContext<'_> {
    fn with_member_node<F, R>(&self, idx: NodeIndex, f: F) -> Option<R>
        where F: FnOnce(&Node) -> Option<R>;

    fn get_member_name_text(&self, idx: NodeIndex) -> Option<&str>;

    fn get_modifiers(&self, idx: NodeIndex) -> Option<&NodeList>;

    fn has_modifier(&self, idx: NodeIndex, kind: SyntaxKind) -> bool;
}
```

---

## Summary Table

| # | Pattern | Occurrences | Priority | Effort | Blocked On |
|---|---------|-------------|----------|--------|------------|
| 1 | Eliminate TypeKey matches in Checker | 75+ | High | Large | Judge classifier API |
| 2 | Arena node access helpers | ~300 | High | Medium | — |
| 3 | Test setup boilerplate | ~200 | High | Low | — |
| 4 | Diagnostic emission patterns | ~50 | High | Medium | — |
| 5 | Type resolution unwrapping | ~50 | Medium-High | Medium | Judge queries (partial) |
| 6 | LSP handler initialization | ~15 | Medium | Medium | — |
| 7 | WASM API flag getters | ~70 | Medium | Low | — |
| 8 | AST kind predicates | ~40 | Medium | Low | — |
| 9 | Builder `with_*` methods | ~50 | Low-Medium | Low | — |
| 10 | `From` impls for enums | ~20 | Low | Low | — |
| 11 | Checker symbol/scope lookups | ~100 | Medium | Medium | — |

### Recommended execution order

**Quick wins first** (items 3, 7, 8, 9, 10): Low effort, high confidence, no blockers.

**Core infrastructure** (items 2, 4, 11): Medium effort, high payoff across the whole codebase.

**Architecture-aligned** (items 1, 5, 6): Align with the Solver Refactoring Proposal; item 1 is the north star goal.
