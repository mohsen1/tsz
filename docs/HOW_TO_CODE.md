# How to Code in tsz

Quick reference for writing code in this repo. Read once, follow always.

---

## Architecture Rules

These are non-negotiable. Violating them creates debt that blocks future work.

### Checker never inspects type internals

The Checker must not match on `TypeKey`. If you need to branch on what a type is, add a **classifier query** to the Solver/Judge and call that instead.

```rust
// WRONG — Checker matching on TypeKey
match db.type_key(type_id) {
    TypeKey::Array(elem) => { /* ... */ }
    TypeKey::Tuple(list) => { /* ... */ }
    _ => {}
}

// RIGHT — Checker calls a classifier
match db.classify_iterable(type_id) {
    IterableKind::Array(elem) => { /* ... */ }
    IterableKind::Tuple => { /* ... */ }
    IterableKind::NotIterable => {}
}
```

See: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md` §2.4, Phase 4.

### Solver owns all type logic

Subtyping, evaluation, narrowing, assignability — all live in `crates/tsz-solver/`. The Checker orchestrates AST traversal and emits diagnostics. It does not compute types.

### No cross-layer shortcuts

```
Scanner → Parser → Binder → Solver → Checker → Emitter → LSP
```

Each layer only depends on layers below it. The Solver never imports from Checker. The Emitter never imports from LSP. Crate boundaries enforce this.

---

## Performance

tsz must be faster than tsc. Performance is a feature, not an afterthought.

### Think about perf when designing

Every new data structure, algorithm, or abstraction should be evaluated for its performance characteristics. Prefer O(1) lookups (interning, arenas, hash maps) over repeated traversals. Prefer stack allocation over heap. Prefer `Copy` types over `Clone`.

### Measure before and after

If a change touches the solver, checker, parser, or binder hot paths, benchmark it:

```bash
# Quick before/after comparison
cargo build --release
hyperfine './target/release/tsz check benches/'

# Detailed profiling (do NOT bind to port 3000)
samply record --no-open ./target/release/tsz check benches/
```

If you don't have a large project handy, use the conformance suite or `benches/` as a proxy.

### Common perf pitfalls

| Pitfall | Fix |
|---------|-----|
| `format!()` / `.to_string()` in hot loops | Use `Atom` (interned) or `&str` |
| `.clone()` on `Vec<TypeId>` inside subtype checks | Borrow or use `SmallVec` |
| `HashMap` with bad key hashing | Use `FxHashMap` (already standard here) |
| Allocating `Vec` per call when size is small | Use `SmallVec<[T; 4]>` or stack arrays |
| Repeated type resolution in a loop | Cache the resolved result before the loop |
| Adding a new field to a hot struct | Check struct size with `std::mem::size_of` — keep cache-line-friendly |

### When to benchmark

- Any change to `crates/tsz-solver/` or `crates/tsz-checker/` hot paths
- New data structures or collection types
- Changes to interning, type evaluation, or subtype checking
- Refactors that change iteration order or add indirection

If the change is docs, tests, CLI flags, or LSP UI — no benchmark needed.

---

## Code Patterns

### Use tracing, never `eprintln!`

```rust
// WRONG
eprintln!("resolved type: {:?}", type_id);

// RIGHT
use tracing::trace;
trace!(type_id = %id.0, "Resolved type");
```

Run with: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts`

### Prefer `pub(crate)` over `pub`

Only use `pub` for items that are part of the crate's external API. Default to `pub(crate)`.

### Avoid `.unwrap()` in library code

Use `.expect("reason")` if you're certain, or propagate with `?`. `.unwrap()` is fine in tests.

### Minimize `.clone()` and `.to_string()`

Hot paths in the solver and checker are allocation-sensitive. Prefer `&str`, `Cow<str>`, or `Atom` (interned string) over `String`. Profile before adding clones.

### Keep functions short

Target: under 50 lines per function, under 2000 lines per file. Long match arms are a sign you need a helper, a visitor, or a classifier.

---

## Recursion Safety

All recursive algorithms must use the guards from `crates/tsz-solver/src/recursion.rs`. Never hand-roll depth tracking with raw `Cell<u32>` or manual counters.

### Two guard types

| Guard | When to use |
|-------|-------------|
| `RecursionGuard<K>` | Recursive algorithms where the same key can form cycles (subtype checking, type evaluation, variance). Provides depth limiting **and** cycle detection via a visiting set. |
| `DepthCounter` | Stack overflow protection where the same node may be legitimately re-visited (expression checking, type node resolution, call depth). Depth limiting only. |

### Use `RecursionProfile` — no magic numbers

```rust
// WRONG — what do 50 and 100_000 mean?
let guard = RecursionGuard::new(50, 100_000);

// RIGHT — intent is clear, limits are centralized
let guard = RecursionGuard::with_profile(RecursionProfile::TypeEvaluation);
let counter = DepthCounter::with_profile(RecursionProfile::ExpressionCheck);
```

Available profiles: `SubtypeCheck`, `TypeEvaluation`, `TypeApplication`, `PropertyAccess`, `Variance`, `ShapeExtraction`, `ShallowTraversal`, `ConstAssertion`, `ExpressionCheck`, `TypeNodeCheck`, `CallResolution`, `CheckerRecursion`. Add new profiles to the enum if none fit.

### Always pair `enter()` / `leave()`

```rust
// RecursionGuard pattern
match guard.enter(key) {
    RecursionResult::Entered => {
        let result = do_work();
        guard.leave(key); // MUST be called on every exit path
        result
    }
    RecursionResult::Cycle => handle_cycle(),
    _ => handle_exceeded(),
}

// DepthCounter pattern
if !counter.enter() {
    return TypeId::ERROR; // depth exceeded — do NOT call leave()
}
let result = do_work();
counter.leave(); // MUST be called on every exit path
result
```

In debug builds, forgotten `leave()` calls trigger a panic on drop. This catches bugs early.

### When a checker context inherits depth from its parent

Use `DepthCounter::with_initial_depth(max, parent_depth)` so the inherited depth is treated as the base level and doesn't trigger debug leak detection when the child context is dropped.

### Don't put limits in `limits.rs` for solver recursion

Solver recursion limits (subtype depth, evaluation depth, etc.) are owned by `RecursionProfile` in `recursion.rs`. The `limits.rs` file is for checker/parser/emitter/capacity constants only. This prevents the "centralized file that nobody actually imports" problem.

---

## Don't Repeat Yourself

Before writing code, check if the pattern already exists. These are the most common traps.

### Arena node access

Don't scatter `if let Some(node) = arena.get(idx)` everywhere. Use or add helpers on the arena/context.

### Test setup

Solver tests should use shared setup helpers, not copy-paste `TypeInterner::new()` + `SubtypeChecker::new(...)` into every test. If a helper doesn't exist for your setup, create one.

### Diagnostics

Never construct `Diagnostic { code, category, message_text, file, start, length, .. }` structs inline. Use `error_at_node()` or the diagnostic builder. Centralized message constants live in `types::diagnostics`.

```rust
// WRONG — 10 lines of manual struct construction
if let Some(loc) = self.get_source_location(node_idx) {
    self.ctx.diagnostics.push(Diagnostic {
        code: diagnostic_codes::SOME_ERROR,
        category: DiagnosticCategory::Error,
        message_text: diagnostic_messages::SOME_ERROR.to_string(),
        file: self.ctx.file_name.clone(),
        start: loc.start,
        length: loc.length(),
        related_information: Vec::new(),
    });
}

// RIGHT — 1 line
self.error_at_node(node_idx, diagnostic_messages::SOME_ERROR, diagnostic_codes::SOME_ERROR);
```

### Builder methods

If your struct has 3+ `with_*` methods, use a macro:

```rust
macro_rules! builder_setters {
    ($($field:ident: $ty:ty),* $(,)?) => {
        $(pub fn $field(mut self, $field: $ty) -> Self {
            self.$field = $field;
            self
        })*
    };
}
```

### `From` impls for enums

Use a macro when you have 3+ trivial `impl From<X> for MyEnum` blocks.

### Nearly-identical visitors

When multiple `TypeVisitor` implementors differ only in which field they extract or which argument index they read, parameterize instead of copying. For example, use `ApplicationArgExtractor::new(db, index)` instead of three separate `GeneratorYieldExtractor` / `GeneratorReturnExtractor` / `GeneratorNextExtractor` structs.

Likewise, if two visitors share identical helper logic (e.g. extracting a parameter type from a `&[ParamInfo]`), extract that logic into a free function rather than duplicating it.

Full list of abstraction opportunities: `docs/todo/abstraction-opportunities.md`

---

## Best Practices

Hard-won patterns from real bugs and refactoring sessions. Follow these to keep the codebase clean.

### Use named constants for bitmask flags

Never write `flags |= 1 << 3`. Use the named constants on `RelationCacheKey`:

```rust
// WRONG — what does bit 3 mean?
let mut flags: u16 = 0;
if self.strict_null_checks { flags |= 1 << 0; }
if self.strict_function_types { flags |= 1 << 1; }

// RIGHT — self-documenting
let mut flags: u16 = 0;
if self.strict_null_checks { flags |= RelationCacheKey::FLAG_STRICT_NULL_CHECKS; }
if self.strict_function_types { flags |= RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES; }
```

The checker has `CheckerContext::pack_relation_flags()` as the single source of truth for packing compiler options into a cache key bitmask. Use it instead of hand-packing flags.

### Extract shared logic when two functions differ only in direction

If two functions are identical except for an argument order or comparison direction, extract the shared body and pass a discriminant:

```rust
// WRONG — 80 lines of nearly identical code in simplify_union_members and simplify_intersection_members

// RIGHT — one shared function with a direction parameter
enum SubtypeDirection { SourceSubsumedByOther, OtherSubsumedBySource }
fn remove_redundant_members(&mut self, members: &mut Vec<TypeId>, direction: SubtypeDirection) { ... }
```

### Don't `.clone()` when `ref` already borrows

`if let Some(ref x) = self.foo.clone()` clones for no reason — `ref` already borrows the inner value. Drop the `.clone()`:

```rust
// WRONG — clones the Option<T> then immediately borrows the inner value
if let Some(ref info) = self.ctx.enclosing_class.clone() { ... }

// RIGHT — borrows directly, zero cost
if let Some(ref info) = self.ctx.enclosing_class { ... }
```

**Exception**: If code below the `if let` calls `&mut self` methods, the borrow through `self.ctx` conflicts. In that case the `.clone()` is necessary — add a comment explaining why:

```rust
// Clone needed: error_cannot_find_name_static_member_at() borrows &mut self
if let Some(ref info) = self.ctx.enclosing_class.clone() {
    self.error_cannot_find_name_static_member_at(name, &info.name, idx);
}
```

### Keep match arms thin

If a `match` arm is more than ~5 lines, extract it into a helper function. A 50-arm match where each arm is a single function call is readable; a 50-arm match where each arm is 15 lines is not.

```rust
// WRONG — 400-line match with inline logic
match node.kind {
    NUMERIC_LITERAL => {
        let literal_type = self.literal_type_from_initializer(idx);
        if let Some(literal_type) = literal_type {
            if self.ctx.in_const_assertion || self.contextual_literal_type(literal_type).is_some() {
                literal_type
            } else { TypeId::NUMBER }
        } else { TypeId::NUMBER }
    }
    // ... 49 more arms like this

// RIGHT — thin dispatch, logic in helpers
match node.kind {
    NUMERIC_LITERAL => self.resolve_literal(self.literal_type_from_initializer(idx), TypeId::NUMBER),
    // ... clean one-liners
}
```

### Prefer `let-else` and early returns over deep nesting

Flatten deeply nested `if let` / `match` chains with `let-else` (Rust 1.65+) and early returns:

```rust
// WRONG — 5 levels of nesting
if let Some(x) = foo() {
    if let Some(y) = bar(x) {
        if condition {
            // actual logic buried here
        }
    }
}

// RIGHT — flat, readable
let Some(x) = foo() else { return TypeId::ERROR; };
let Some(y) = bar(x) else { return TypeId::ERROR; };
if !condition { return TypeId::ERROR; }
// actual logic at top level
```

### Name magic thresholds

Any numeric literal used as a limit or threshold must be a named constant with a comment:

```rust
// WRONG
if members.len() > 25 { return; }

// RIGHT
const MAX_SIMPLIFICATION_SIZE: usize = 25;
if members.len() > MAX_SIMPLIFICATION_SIZE { return; }
```

For solver/checker recursion limits, use `RecursionProfile` (see Recursion Safety above). For capacity constants shared across crates, add them to `crates/tsz-common/src/limits.rs`.

---

## Testing

- Write a failing test first, then implement.
- Unit tests go in the same crate: `crates/tsz-solver/src/tests/`.
- Prefer `cargo nextest run` over `cargo test` — it runs tests in parallel per-test (not per-crate), gives better output on failures, and is significantly faster.
- Run a single crate's tests: `cargo nextest run -p tsz-solver`.
- Run a single test: `cargo nextest run -p tsz-solver test_name`.
- Use `#[cfg(test)]` guards in all test files.

---

## Debugging

1. Write a minimal `.ts` file that reproduces the issue.
2. Run with tracing: `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- test.ts 2>&1 | head -200`
3. Narrow the filter: `TSZ_LOG="wasm::solver::narrowing=trace"`
4. Find where actual behavior diverges from expected.
5. Compare against `tsc` output: `npx tsc --noEmit test.ts`

---

## Git

- Commit frequently with short semantic messages.
- Only stage files you touched — no `git add -A`.
- Rebase from main before and after committing.
- Push to remote after every commit.

---

## Quick Checks Before Submitting

- [ ] No `eprintln!` added
- [ ] No new `TypeKey::` matches in `crates/tsz-checker/`
- [ ] No `.unwrap()` in library code without a reason
- [ ] New public items are `pub(crate)` unless they need to be exported
- [ ] No raw `Cell<u32>` / manual depth counters — use `RecursionGuard` or `DepthCounter`
- [ ] Tests pass: `cargo nextest run -p <crate-you-changed>`
- [ ] `cargo clippy` clean on changed files
- [ ] If touching solver/checker hot paths: benchmarked before and after
