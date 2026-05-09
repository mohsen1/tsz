# fix(emitter): preserve re-export clause comments

- **Date**: 2026-05-07
- **Branch**: `fix/preserve-reexport-comments`
- **PR**: #4378
- **Status**: ready
- **Workstream**: emitter parity (issue #3897)

## Intent

ES re-export emission reconstructs the export declaration from the clause
and module specifier, but never asks for comments attached to the source
range between the clause/star token and the `from` keyword, nor between
the open `{` and the first specifier. As a result, all three comment
positions in #3897 were silently dropped:

```ts
export { foo } /* after clause */ from "./b";    // dropped
export { /* before name */ bar } from "./b";    // dropped
export * /* star */ from "./b";                 // dropped
```

This PR walks the source text to locate the relevant tokens (`}`, `*`,
`{`) and calls `emit_comments_in_range` so the comment-emit cursor
visits each gap. The named-clause path uses `rfind('}')` because the
NamedExports node's `.end` extends past the `from` keyword in our AST,
so the AST end isn't usable as the close-brace position.

## Files Touched

- `crates/tsz-emitter/src/emitter/module_emission/core/mod.rs` (~50 LOC)
- `crates/tsz-emitter/tests/comment_tests.rs` (3 new regression tests)

## Verification

- `cargo nextest run -p tsz-emitter --test comment_tests -E 'test(re_export)'` — 3/3 pass
- `cargo nextest run -p tsz-emitter` — 2201/2201 still pass
- Manual repro from #3897 emits identically to tsc 6.0.3.
