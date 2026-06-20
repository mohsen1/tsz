# Query Boundaries And Diagnostics

The checker-to-solver boundary is the most important operational boundary in
`tsz`. It exists so source-aware checker code can ask semantic questions without
owning solver internals.

## Boundary Shape

The preferred shape is:

```text
checker source context
  -> stable ids and explicit request data
  -> query boundary
  -> solver semantic answer or structured failure
  -> checker diagnostic mapping and source spans
```

`docs/architecture/QUERY_BOUNDARY_INVENTORY.md` classifies the modules under
`crates/tsz-checker/src/query_boundaries/` as stable APIs, diagnostic adapters,
compatibility shims, or quarantine helpers.

## Stable Boundary Examples

Important modules include:

- `crates/tsz-checker/src/query_boundaries/assignability.rs`
- `crates/tsz-checker/src/query_boundaries/relation_request.rs`
- `crates/tsz-checker/src/query_boundaries/relation_types.rs`
- `crates/tsz-checker/src/query_boundaries/property_access.rs`
- `crates/tsz-checker/src/query_boundaries/flow_analysis.rs`
- `crates/tsz-checker/src/query_boundaries/state/type_environment.rs`
- `crates/tsz-checker/src/query_boundaries/type_computation/core.rs`

New checker code should prefer narrow stable boundary modules. Broad wrappers in
`common.rs` and other compatibility shims are migration surfaces, not a reason
to add more direct solver shape knowledge in checker code.

## Diagnostics

Diagnostics are source-facing products. The checker owns:

- diagnostic code selection when multiple source rules compete;
- source span and related-info ordering;
- suppression and deduplication;
- source-context-sensitive display;
- user-facing wording and compatibility with `tsc`.

The solver owns:

- whether the semantic relation succeeds;
- the semantic reason it failed;
- property/member presence facts;
- inferred/instantiated/evaluated type facts.

`crates/tsz-common/src/diagnostics/` provides shared diagnostic data. Checker
diagnostic orchestration lives across `crates/tsz-checker/src/context/`,
`crates/tsz-checker/src/error_reporter/`, and syntax-family checkers.

## Assignability Diagnostics

For assignability families such as `TS2322`, `TS2345`, and `TS2416`, the flow
should be:

```text
relation request
  -> solver relation
  -> relation outcome and failure reason
  -> checker diagnostic adapter
  -> source-anchored diagnostic
```

`crates/tsz-checker/src/assignability/assignability_diagnostics/` contains
argument reports, display helpers, explicit-any annotation handling, generic
argument suppression, and type-comparability logic.

## Anti-Hardcoding Rules

Do not drive compiler behavior from:

- user-chosen identifier strings;
- alias names;
- type-parameter names;
- property names except true builtins via stable builtin identity;
- file names or test names;
- source-text snippets;
- rendered type strings or diagnostic messages.

If a behavior seems to require one of those shortcuts, the real semantic
operation has not been identified yet.
