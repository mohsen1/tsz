# How TSZ Works

This guide is the wide map of the `tsz` repository. It is intentionally broader
than the architecture contracts: it explains the pipeline, the crate graph, the
query boundaries, the product surfaces, the tests, the CI harnesses, and the
generated inventory that mentions every repository file.

The shortest mental model is:

```text
source text
  -> scanner
  -> parser
  -> binder
  -> checker
  -> solver
  -> emitter
  -> CLI / LSP / WASM / website / benchmarks / conformance gates
```

The most important rule is that each layer owns one kind of truth. Syntax is
not semantics. Diagnostics are not a semantic predicate. Emit produces output;
it does not discover new type facts.

## Reading Order

1. [`00-roadmap-and-repo-map.md`](00-roadmap-and-repo-map.md) - why this repo is
   shaped around `green`, `fast`, `grow`, and `hold`.
2. [`01-end-to-end-pipeline.md`](01-end-to-end-pipeline.md) - the compiler flow
   from text to diagnostics and output.
3. [`02-workspace-crates.md`](02-workspace-crates.md) - every workspace crate
   and what it owns.
4. [`03-data-identity-and-caches.md`](03-data-identity-and-caches.md) - stable
   ids, arenas, interning, cache ownership, and why identity mistakes hurt.
5. [`04-front-end-scanner-parser-binder.md`](04-front-end-scanner-parser-binder.md)
   - tokens, AST, symbols, scopes, modules, and flow skeletons.
6. [`05-checker.md`](05-checker.md) - checker orchestration, source context,
   diagnostics, flow, declaration checking, JSX, classes, and generic checks.
7. [`06-solver.md`](06-solver.md) - relations, inference, instantiation,
   narrowing, operations, type construction, and compatibility policy.
8. [`07-query-boundaries-and-diagnostics.md`](07-query-boundaries-and-diagnostics.md)
   - how semantic answers cross into checker-owned diagnostics.
9. [`08-emitter.md`](08-emitter.md) - JS emit, declaration emit, transforms,
   source maps, and no-semantic-validation rules.
10. [`09-cli-project-lsp-wasm-site.md`](09-cli-project-lsp-wasm-site.md) -
    project orchestration, command-line entrypoints, editor features, WASM, and
    website integration.
11. [`10-tests-ci-benchmarks.md`](10-tests-ci-benchmarks.md) - local checks,
    CI gates, conformance, emit, fourslash, benchmark rows, and quality tools.
12. [`11-agent-and-workflow-surfaces.md`](11-agent-and-workflow-surfaces.md) -
    repo-local agent skills, hooks, worktree hygiene, PR requirements, and
    context hygiene.
13. [`12-maintaining-this-guide.md`](12-maintaining-this-guide.md) - how to
    regenerate and verify the generated inventory.
14. [`internals/README.md`](internals/README.md) - the deep-dive tier: twenty
    code-grounded chapters that walk the actual checker and solver modules, data
    shapes, entry functions, caches, fuel limits, and `tsc` parity edge cases
    behind the map chapters above. Read a map chapter first, then drop into the
    matching internals chapter for the exact mechanism.
15. [`file-inventory/README.md`](file-inventory/README.md) - generated index of
    inventory pages. The inventory pages mention every repository file path.

## File Coverage Contract

The generated inventory is not a substitute for understanding the architecture,
but it is the audit trail for the request that every file be mentioned here.
Run this after changing repository files or the guide:

```bash
node scripts/docs/generate-how-tsz-inventory.mjs
node scripts/docs/check-how-tsz-docs-coverage.mjs
```

The checker scans `docs/how-tsz-works` and verifies that every file reported by
`git ls-files --cached --others --exclude-standard` appears as a literal path in
the guide. That includes this file, `docs/README.md`, the generator script, and
the checker script.
