# Solver

`crates/tsz-solver` owns type semantics. If a question is about what a TypeScript
type means, whether two types relate, how inference proceeds, or what a type
operator evaluates to, the answer belongs in the solver or behind a solver-backed
query boundary.

## Solver Owns

- `TypeId` construction and interning.
- Type relations, subtype checks, assignability, and compatibility policy.
- Type evaluation for indexed access, mapped types, conditional types, template
  literal types, `keyof`, `typeof`, and other type operators.
- Generic inference, inference sites, and candidate collection.
- Instantiation of generic aliases, signatures, mapped types, and object shapes.
- Narrowing and type-predicate facts.
- Property/member lookup semantics.
- Semantic caches and structured relation failure reasons.

## Important Areas

| Path | Role |
| --- | --- |
| `crates/tsz-solver/src/lib.rs` | Public solver facade, type handles, query/computation/construction modules, observability exports, and tests. |
| `crates/tsz-solver/src/types/` | Type data and type construction vocabulary. |
| `crates/tsz-solver/src/intern/` | Type interning and canonical identity. |
| `crates/tsz-solver/src/relations/` | Relation algorithms and subtype/assignability behavior. |
| `crates/tsz-solver/src/evaluation/` | Type evaluation rules and orchestrators. |
| `crates/tsz-solver/src/inference/` | Inference machinery. |
| `crates/tsz-solver/src/instantiation/` | Generic/type instantiation logic. |
| `crates/tsz-solver/src/operations/` | Semantic operations such as calls, constraints, and object operations. |
| `crates/tsz-solver/src/narrowing/` | Narrowing and predicate behavior. |
| `crates/tsz-solver/src/type_queries/` | Structured type query helpers. |
| `crates/tsz-solver/src/diagnostics/` | Structured failure data and display support consumed by checker diagnostics. |
| `crates/tsz-solver/src/caches/` | Cache data structures and relation/evaluation cache support. |

## Judge And Lawyer

The compatibility model separates strict structural logic from TypeScript's
legacy compatibility behavior:

- The "judge" is strict structural/set-theoretic subtype logic.
- The "lawyer" is compatibility policy: `any`, variance, excess/freshness,
  weak types, void-return exceptions, and other TypeScript quirks.

The default preference is that `any` should not silence structural mismatches
unless compatibility mode requires it. When a PR changes compatibility behavior,
the structural rule and the compatibility exception should both be named.

## Failure Reasons

Solver failures should be structured. The checker can then decide how to render,
where to anchor, and which diagnostics to suppress or group. A rendered type
string is not a semantic predicate; it is presentation.

## Performance Constraints

Solver work is often on hot paths. A solver performance fix should identify:

- the repeated operation;
- the stable semantic cache key;
- cache invalidation and residency expectations;
- whether cache-enabled and cache-disabled runs agree;
- why the optimization is not fixture-name-specific.

Performance claims should be measured with the benchmark tooling described in
`docs/how-tsz-works/10-tests-ci-benchmarks.md`, not inferred from one local run.
