# Capability And Completion Review

Read this reference when changing capability/nonclaim derivation, scoped
checking, completion aggregation, emit planning, or service publication.

## Structural Invariant

When a semantic producer is nonclaimed or incomplete, every consumer that
depends on its declaration, merged group, syntax model, flow state, or query
result receives typed incompleteness before a definitive diagnostic or product
is published. Independent demands remain checkable.

The smallest *lexical* node is not necessarily the smallest sound scope. Use
the smallest dependency-closed semantic owner. Do not recover soundness by
promoting a local gap to a file or program skip unless the dependency is truly
that broad.

## Decision Path

Trace one path for each affected operation:

```text
authored fact
  -> immutable capability analysis
  -> nonclaimed producer and dependency/group closure
  -> typed semantic demand
  -> local completion tracker
  -> diagnostic/product/service publication
  -> aggregate process result
```

Name the owner at every arrow. Capability derivation may use syntax and frozen
program/binder facts. Checker, emit, services, and exit selection query the
result; they do not reconstruct the policy.

## Prerequisites Versus Semantic Work

- Keep every declaration identity, binder group, syntax model, and source fact
  the frontend actually constructed available when semantic evaluation is
  nonclaimed. If construction is incomplete, record that producer nonclaim.
- Do not encode a nonclaim by omitting a producer from a model/cache/side table;
  absence is then indistinguishable from a real missing declaration.
- Treat `not found`, empty members/signatures, and relation failure as
  definitive answers. Publish or cache them only after every dependency is
  Complete; an incomplete producer yields Deferred.
- Guard the semantic demand before materialization, relation, lookup, or
  diagnostic publication.
- Close merged declarations and overload groups as one dependency. Do not pick
  or filter a claimed peer from a partially nonclaimed group.
- Attribute incomplete cross-file work to both the producer owner and the
  active consumer demand. Unrelated files keep their own completion.
- Navigation and quick-info claims include origin, resolved definition/group,
  and every published result location.

## Public Test Matrix

For each new nonclaim family, pin all applicable cells through the public
compiler/service surface:

1. producer alone: typed reason, scope, completion, products, and exit;
2. dependent consumer in the same file: Deferred with no fabricated diagnostic;
3. dependent consumer in another file: producer and consumer Deferred;
4. independent same-file sibling: its exact diagnostic remains;
5. independent cross-file sibling: Complete with its exact diagnostic;
6. merged/overload/group dependency: the whole semantic demand defers;
7. renamed binders plus wrapper/nesting variants;
8. reversed roots, repeated runs, and noCheck when applicable;
9. service queries and navigation at both claimed and nonclaimed locations;
10. emit/product and process-exit agreement with the same analysis.

Assert the full diagnostic identity: normalized path, code, UTF-16 span,
category, message chain, and related information. A test that only searches for
one expected code can miss a new false diagnostic.

## Artifact Evidence

Compare the immediate prior and current broad artifacts by stable key. Report:

- key-set/duplicate audit;
- complete JS/DTS/conformance/fourslash status matrices;
- every changed diagnostic sequence and product payload;
- aggregate additions/removals by code;
- exact pinned-oracle evidence for every addition.

An unchanged status is not evidence of unchanged behavior. In emit artifacts,
`TSZ_NONZERO_OUTCOME` is an oracle-clean invocation, so any newly added TSZ
diagnostic is a regression even if the row remains `incomplete`.

## Temporary Nonclaims

Each temporary nonclaim records a typed structural reason and a deletion
condition that names the missing owner or algorithm. The PR body states the
current scope, why it is dependency-closed, the public fallback tests, and what
must become true before the nonclaim is removed.
