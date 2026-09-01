# How to Code in TSZ

This is the implementation checklist for the clean-slate compiler. Read the
[`roadmap`](plan/ROADMAP.md) and
[`reset architecture`](architecture/RESET.md) before changing compiler code.

## Start From The Oracle

Do not infer TypeScript behavior from the retired TSZ implementation. The
pinned TypeScript 7.0.2 source and oracle output define the result and the order
of operations.

Before editing, write:

```text
When <structural condition>, TypeScript 7 does X; TSZ does X through <module/API>.
```

Use the smallest witness that exposes the rule, then add adjacent cases:
renamed binders, aliases or wrappers, nesting, generic and concrete forms,
positive behavior, and negative/fallback behavior when applicable.

Never branch on a fixture path, file name, user identifier, source fragment,
rendered type, or formatted diagnostic. Built-ins resolve through program-owned
declarations, not spelling checks.

## Put Work In One Owner

The active workspace is intentionally small:

- `tsz-core::syntax`: tokens, trivia, immutable syntax, parser recovery.
- `tsz-core::program`: sources, options, paths, root order, resolution,
  declaration and symbol identity.
- checker/semantic modules: binding, flow, types, inference, relations,
  contextual typing, queries, and structured failures.
- `tsz-core::emit`: transforms and printing from syntax and explicit checked
  summaries; no general semantic validation.
- `tsz-core::service`: the public compiler/project/language-service facade.
- `tsz-cli`: process and protocol adaptation only.
- `tsz-conformance`: external-process oracle comparison only.

Keep phases as modules. Do not restore the deleted crate-per-phase dependency
graph. Do not add browser/WASM bindings before R4.

## Identity And Semantic State

Use identities in their declared domains:

- `FileId` indexes the normalized program file table.
- `NodeId` is meaningful with its file arena.
- `DeclId` and `SymbolId` are program-owned.
- type-parameter identity comes from its declaration/binder, never its name.
- `TypeId` is local to one checker session and cannot cross service or worker
  boundaries.

Structural equality does not replace nominal declaration identity. Content
hashes may deduplicate immutable bytes; they do not elect declarations.

Avoid ambient request state, thread-local semantic fallbacks, mirrored stores,
global cache epochs, and whole-cache invalidation.

## Preserve Symbolic Forms

References and applications, type parameters, inference placeholders, indexed
access, `keyof`, conditionals, mapped types, and other deferred forms remain
symbolic until their owning operation requires a concrete view.

Semantic completion is explicit:

```text
Complete | Deferred | Cycle | Limit
```

Incomplete completion is not `any`, `unknown`, an error type, or a definitive
cache entry. Callers propagate or handle it according to the ported TypeScript
operation.

Interning canonicalizes a representation already chosen by the caller. It must
not silently force a deferred form, reduce a union, or erase alias provenance.
Union reduction policy belongs to the semantic call site.

## Relations, Diagnostics, And Speculation

Relations return structured facts such as missing property, incompatible
property, signature mismatch, literal mismatch, or incomplete completion.
Diagnostic code, message, span, and related information are selected after the
semantic result. Never use rendered diagnostic text as input to semantics.

Speculative work such as overload probing or contextual typing needs an
explicit transaction. Scratch types, inference state, diagnostics, and
request-sensitive memo entries commit or roll back together.

## Caches Need A Contract

Start uncached. Add a semantic-result cache only when the uncached behavior is
stable and the review can state:

- the exact question and typed key;
- every input that can change the answer;
- whether incomplete results are representable (normally they are not cached);
- dependency and reset behavior;
- session lifetime and residency bound;
- cold/warm, enabled/disabled, file-order, and repeated-run agreement tests.

Pure session-local type interning is distinct from a semantic-result cache, but
it still cannot perform hidden evaluation.

## Determinism Before Concurrency

The single-checker path is authoritative. Parallel file-local work writes only
to isolated storage and joins at deterministic phase barriers. Never share a
checker concurrently or move raw type handles between workers.

A parallel path graduates only after repeated one-thread/many-thread,
cold/warm, and reversed-root-order runs have identical diagnostic
fingerprints, output, and exit status with bounded memory.

## Rust Practices

- Prefer the narrowest visibility; public compiler behavior belongs at the
  service facade.
- Use compact explicit handles and deterministic ordering.
- Propagate expected failures with `Result`; use `expect` only for a documented
  invariant. `unwrap` is acceptable in tests.
- Avoid unnecessary allocation in measured paths, but measure before adding a
  cache or specialized representation.
- Keep functions and modules focused. No hand-written Rust, test, script, or
  generated shard may exceed 2,000 physical lines.
- Use tracing in compiler internals. Do not add `dbg!`, `println!`, `print!`, or
  `eprintln!` instrumentation.

Example:

```rust
tracing::trace!(file = ?file_id, node = ?node_id, "checking expression");
```

## Testing

Use public service behavior or native processes for compatibility tests. The
retained legacy corpus is disabled until a test is ported deliberately.

```bash
cargo nextest run -p tsz-core --test rewrite_foundation
cargo nextest run -p tsz-cli --test rewrite_process_contract
cargo nextest run --workspace
```

For a supported seed, compare exact diagnostic codes, normalized spans,
messages, exit status, and emit with TypeScript 7. Repeat ten times and reverse
root-file order when multiple files are involved.

Broad corpus output is observational in R0/R1. Preserve every unsupported,
failed, or crashed result rather than filtering it away.

## Before You Submit

- [ ] The structural TypeScript 7 rule and owning module/API are stated.
- [ ] No retired implementation was copied, wrapped, or restored.
- [ ] The change uses the three-package, modules-first architecture.
- [ ] Identity, completion, relation, diagnostic, and emit ownership stay clear.
- [ ] No semantic hardcoding or alternate semantics surface was introduced.
- [ ] Adjacent oracle cases cover the supported family.
- [ ] Determinism and cache agreement are tested where relevant.
- [ ] Focused nextest and retained-harness commands are recorded exactly.
- [ ] Performance evidence is attached only for oracle-green behavior.
- [ ] Formatting, Clippy, architecture, and file-size gates pass.
