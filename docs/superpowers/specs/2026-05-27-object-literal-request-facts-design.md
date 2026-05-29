# Object Literal Request Facts Design

AgentName: M4-C

## Context

Issue #9436 asks for a behavior-preserving checker refactor that separates
object-literal request and facts collection from property typing. The current
`get_type_of_object_literal_with_request` entrypoint normalizes contextual type,
tracks object targets, scans object-literal elements, pushes contextual `this`
state, creates the base property request, and then immediately enters the
diagnostic-sensitive property loop.

The goal is to make that first phase explicit without changing TypeScript
compatibility, diagnostic wording or spans, emitted output, LSP output, or the
checker/solver ownership boundary.

## Proposed Boundary

Add an `ObjectLiteralRequestFacts` helper owned by
`crates/tsz-checker/src/types/computation/object_literal/computation.rs`.
The helper is checker orchestration only: it packages existing contextual
request state and preserves existing side effects. It must not compute new type
relations, bypass query boundaries, or introduce solver internals into the
checker.

The facts phase returns:

- normalized contextual type after nullish stripping and discriminant narrowing
- original contextual type before discriminant narrowing
- whether all object elements are context sensitive
- object getter-name pre-scan used by accessor processing
- optional `ThisType<T>` marker type
- optional contextual receiver `this` type
- base `TypingRequest` for property, method, accessor, and spread handling
- partial-initializer stack index for recursive object-literal tracking

The phase also preserves these current side effects:

- records contextual object targets when a real target is present
- traces the object-literal entry contextual type
- pushes evaluated marker `this` onto `this_type_stack`
- pushes partial initializer state for variable initializers

## Non-Goals

- Do not change property assignment typing, spread handling, accessor handling,
  diagnostic rendering, or final object construction.
- Do not add a new public API or move semantic logic into checker code.
- Do not change conformance snapshots or project-corpus behavior.
- Do not update `docs/plan/ROADMAP.md`; this is routine refactor state.

## Validation

Use focused local checks only:

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker object_literal`
- `scripts/arch/check-checker-boundaries.sh`

Ready-for-review CI remains responsible for broad conformance, emit, fourslash,
and WASM verification.
