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
hyperfine './target/release/tsz check test-files/large-project/'

# Detailed profiling (do NOT bind to port 3000)
samply record --no-open ./target/release/tsz check test-files/large-project/
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

## Don't Repeat Yourself

Before writing code, check if the pattern already exists. These are the most common traps.

### Arena node access

Don't scatter `if let Some(node) = arena.get(idx)` everywhere. Use or add helpers on the arena/context.

### Test setup

Solver tests should use shared setup helpers, not copy-paste `TypeInterner::new()` + `SubtypeChecker::new(...)` into every test. If a helper doesn't exist for your setup, create one.

### Diagnostics

Don't construct `Diagnostic::error(format!(...), code)` by hand in 50 places. Use the diagnostic builder and centralized message constants. Match TypeScript's `diagnosticMessages.json` structure.

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

Full list of abstraction opportunities: `docs/todo/abstraction-opportunities.md`

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
- [ ] Tests pass: `cargo nextest run -p <crate-you-changed>`
- [ ] `cargo clippy` clean on changed files
- [ ] If touching solver/checker hot paths: benchmarked before and after
