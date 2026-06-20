# Checker

`crates/tsz-checker` is the largest crate because it is where source traversal,
source context, diagnostic policy, flow, and semantic requests meet. Its job is
to orchestrate checks, not to become a second solver.

## Checker Owns

- Which AST nodes to visit and in what order.
- Which source context applies to a question.
- How to construct `TypingRequest`, `RelationRequest`, and narrower boundary
  requests.
- Diagnostic priority, suppression, source spans, related information, and
  source-facing wording.
- Flow graph construction and flow-analysis orchestration.
- Syntax-family checkers for declarations, classes, calls, JSX, properties,
  generics, statements, expressions, and JSDoc.

## Checker Must Not Own

- Low-level type relation algorithms.
- Type inference kernels.
- Instantiation and conditional/mapped/template type evaluation.
- Semantic decisions based on rendered diagnostic strings.
- Semantic branches keyed by user-chosen identifier, alias, type-parameter,
  property, or file-name strings.

## Important Areas

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/lib.rs` | Public facade and test-module registry. |
| `crates/tsz-checker/src/context/` | Checker context, caches, compiler options, file/session state, resolver integration, and diagnostic queues. |
| `crates/tsz-checker/src/state/` | Whole-file/program checking state, type environment, type resolution, variable checking, and state checking. |
| `crates/tsz-checker/src/query_boundaries/` | Checker-facing API over solver semantics. |
| `crates/tsz-checker/src/assignability/` | Relation gateway, assignment checking, argument reports, display helpers, and assignability diagnostics. |
| `crates/tsz-checker/src/checkers/` | Syntax-family checkers such as calls, generics, JSX, properties, promises, iterables, enums, and accessors. |
| `crates/tsz-checker/src/classes/` | Class declarations, constructors, inheritance, implements checks, static/private/super behavior. |
| `crates/tsz-checker/src/declarations/` | Imports, exports, namespaces, modules, dynamic import, and declaration-level checks. |
| `crates/tsz-checker/src/dispatch/` | Expression dispatch helpers. |
| `crates/tsz-checker/src/error_reporter/` | Diagnostic rendering and elaboration. |
| `crates/tsz-checker/src/flow/` | Flow graph builder and flow analysis. |
| `crates/tsz-checker/src/jsdoc/` | JSDoc parsing/resolution integration and JSDoc-specific semantic surfaces. |
| `crates/tsz-checker/src/types/` | Type-node and type-computation orchestration on the checker side. |

## Assignability Path

Assignability diagnostics such as `TS2322`, `TS2345`, and `TS2416` should route
through the shared query-boundary assignability gateway:

```text
checker source site
  -> RelationRequest
  -> solver relation
  -> structured failure reason
  -> checker diagnostic mapping
```

The checker owns where the error attaches. The solver owns whether the relation
holds and why it failed.

## Flow

Flow is split between binder and checker:

- binder creates the flow skeleton and stable flow nodes;
- checker builds or consumes the graph for source-aware analysis;
- solver-backed queries answer semantic questions used by narrowing;
- diagnostics attach in checker-owned source context.

Files under `crates/tsz-checker/src/flow/` should not bypass query boundaries to
construct raw semantic types or inspect solver internals.

## Test Shape

`crates/tsz-checker/src/lib.rs` wires many small regression modules. That style
keeps individual files below the repo line ceiling and lets parity fixes add
adjacent cases near the behavior they protect. Good checker tests vary binder
names when names are involved and include positive, negative, alias/wrapper, and
generic/concrete cases when behavior risk warrants it.
