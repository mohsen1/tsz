# investigate(checker): diagnostic type display over-expands aliases / Applications

- **Date**: 2026-05-02
- **Branch**: `investigate/diagnostic-type-display-alias-preservation`
- **PR**: TBD (hand-off doc, no code change)
- **Status**: claim
- **Workstream**: 1 (Conformance — multiple fingerprint-only tests share
  this pattern)

## Pattern

tsc preserves the *named / structured* form of types in diagnostic
messages — type alias names, type Application syntax (`Foo<X, Y>`),
recursive aliases, conditional types, mapped types. tsz frequently
over-evaluates these to their structural form (`{ a: number; }`,
`number | string`, `T extends X ? A : B`), losing the alias name and
making fingerprints diverge.

## Tests sharing this pattern (current main, 12345/12582)

| Test | Code | Expected (tsc) | Actual (tsz) |
| --- | --- | --- | --- |
| `strictOptionalProperties3.ts` | TS2375 | `'A'` / `'B'` (JSDoc @typedef) | `'{ value?: number; }'` |
| `conditionalTypeVarianceBigArrayConstraintsPerformance.ts` | TS2322 | `'Stuff<T>'` (alias of conditional) | `'T extends keyof IntrinsicElements ? IntrinsicElements[T] : any'` |
| `reverseMappedTypeContextualTypeNotCircular.ts` | TS2322 | `'Selector<unknown, {}>'` (subst.) | `'Selector<S, T["editable"]>'` (un-subst.) |
| `destructuringUnspreadableIntoRest.ts` | TS2339 ×11 | `'Omit<this, "...">'` | `'{ publicProp: string; }'` / `'{}'` |
| `intersectionsAndOptionalProperties.ts` | TS2322 | `'{ a: null; b: string; }'` (collapsed intersection) | `'{ a: null; } & { b: string; }'` (split) |

That's **15+ fingerprints across 5 tests** that share this same root —
fixing the formatter once is high leverage.

## Where to look first

- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
  - `format_assignability_type_for_message_internal` (line 950) and
    `format_top_level_assignability_message_types` already have a
    "prefer authoritative name" branch that calls
    `authoritative_assignability_def_name`.
  - `authoritative_assignability_def_name`
    (`error_reporter/core_formatting.rs:641`) returns the alias name
    when the type resolves to a Lazy(DefId) or
    `definition_store::find_def_for_type` finds the def. **It returns
    `None` for types that have already been resolved past their
    Lazy/Application form** — which is exactly the case for JSDoc
    `@typedef` resolved targets and Application-with-substitution
    targets like `Stuff<T>`.

- `display_alias` (in `crates/tsz-solver/src/intern/`):
  - `interner.get_display_alias(type_id)` returns a "preferred display
    form" when one was attached. Look at where this is set and where
    it's consulted; some paths consult it but the diagnostic-formatter
    paths (especially `format_exact_optional_target_type_for_message`
    in `assignability.rs:1110`) currently bypass it.

## Two surgical fixes that may flip tests

### A. Honor `display_alias` in `format_exact_optional_target_type_for_message`

```rust
fn format_exact_optional_target_type_for_message(&mut self, target: TypeId) -> String {
    if let Some(alias) = self.ctx.types.get_display_alias(target) {
        if let Some(name) = self.format_alias_name(alias) {  // helper TBD
            return name;
        }
    }
    // …existing formatter…
}
```

This would flip `strictOptionalProperties3.ts` if JSDoc `@typedef`
attaches a `display_alias`. (Verify by grepping for
`store_display_alias` calls in the JSDoc binder.)

### B. Make `authoritative_assignability_def_name` consult `display_alias`

Add a third case to `authoritative_assignability_def_name`'s direct-def
lookup so it picks up alias names from `display_alias` even when the
type has been evaluated past its Lazy form:

```rust
let direct_def_name = |state: &Self, candidate: TypeId| {
    // …existing path: lazy_def_id / find_def_for_type …
    // Add: if no def, try display_alias and recurse on the alias type.
    if let Some(alias) = state.ctx.types.get_display_alias(candidate) {
        if alias != candidate {
            return direct_def_name(state, alias);
        }
    }
    None
};
```

## Verification plan for the next agent

1. Build the test corpus list:

   ```bash
   for t in strictOptionalProperties3 conditionalTypeVarianceBigArrayConstraintsPerformance reverseMappedTypeContextualTypeNotCircular destructuringUnspreadableIntoRest intersectionsAndOptionalProperties; do
     .target/dist-fast/tsz-conformance --filter "$t" \
       --print-fingerprints --workers 1 --no-batch \
       --tsz-binary .target/dist-fast/tsz \
       --cache-file scripts/conformance/tsc-cache-full.json | head -5
   done
   ```

2. Add an `eprintln!` at the top of
   `format_exact_optional_target_type_for_message` printing both
   `target_id` and `interner.get_display_alias(target_id)`. Run the
   `strictOptionalProperties3` repro:

   ```ts
   /** @typedef {object} A
    *  @property {number} [value] */
   /** @type {A} */
   const a = { value: undefined }; // error
   ```

   If `get_display_alias` returns `Some(...)`, fix A is straight-forward.
   If it returns `None`, the JSDoc binder needs to call
   `store_display_alias`.

3. Once fix A lands, repeat for `conditionalTypeVarianceBigArraysPerformance`
   (TS2322 path) and verify fix B lights up the remaining tests.

## Why this is a hand-off

The previous iteration tried fix A (in
`format_exact_optional_target_type_for_message`, mirroring the TS2322
prefer-authoritative-name branch) and got `None` from
`authoritative_assignability_def_name` for the JSDoc target — so the
fix was a no-op. The next layer is `display_alias` plumbing, which
crosses crate boundaries (`tsz-solver` interner ↔ `tsz-checker`
formatter) and needs more care than fits one /loop iteration. This
doc captures the *exact* targets, code paths, and proposed shape so
the next iteration can pick up.
