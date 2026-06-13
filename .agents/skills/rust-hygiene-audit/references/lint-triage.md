# Clippy lint triage for a compiler codebase

`measure_clippy.py` runs `pedantic`+`nursery` and buckets the results. This is
the reasoning behind the default buckets — re-judge per repo, but the
compiler-specific calls below hold up well.

## Why promote these (signal > churn)

These catch real maintenance and (occasionally) correctness debt with low
false-positive rates:

- **Idiom / readability**: `manual_let_else`, `explicit_iter_loop`,
  `single_match_else`, `unnested_or_patterns`, `needless_continue`,
  `redundant_closure_for_method_calls`, `map_unwrap_or`. Each makes control flow
  read the way the compiler team already prefers.
- **Ownership / perf**: `needless_pass_by_ref_mut` (over-broad `&mut` hides
  aliasing intent), `needless_pass_by_value`, `implicit_clone`,
  `assigning_clones`, `format_push_string`. These shrink accidental copies.
- **Dead / over-broad surface**: `redundant_pub_crate`, `unused_self`. Both
  narrow API surface, which compounds with dead-code paydown.
- **Complexity / design**: `too_many_lines` (function-scoped; backs a file-size
  campaign), `struct_excessive_bools` (usually wants an enum/bitflags),
  `branches_sharing_code`, `useless_let_if_seq`, `default_trait_access`,
  `elidable_lifetime_names`, `wildcard_imports`.
- **Latent correctness**: `case_sensitive_file_extension_comparisons` — in a
  TypeScript/JS toolchain, `.ts`/`.TS`/`.d.ts` case handling can be a real bug,
  not style. Always read these hits.

## Why defer/allow these (mostly intentional in a compiler)

- **Numeric casts**: `cast_possible_truncation`, `cast_lossless`,
  `cast_precision_loss`, `cast_sign_loss`. A compiler with `u32` id-newtypes and
  arena indices casts deliberately and constantly; promoting these is pure
  churn. Allow with a one-line rationale.
- **API ergonomics pedantry**: `must_use_candidate`, `return_self_not_must_use`.
  High volume, low value unless the crate is a public library.
- **Doc pedantry**: `missing_panics_doc`, `missing_errors_doc`,
  `too_long_first_doc_paragraph`. Worth it only for published API crates.
- **Style coin-flips**: `items_after_statements`, `if_not_else`,
  `option_if_let_else` (the last is often *less* readable than the `if let`).

## Rollout shape

Promote one family per PR, allow-list first:

1. Land the allow-list (`allow` the defer set with rationale) so the floor can
   rise without a wall of warnings.
2. Raise the floor: `pedantic = { level = "warn", priority = -1 }`, then
   `nursery = "warn"`; fix or allow the residual per crate.
3. Add `unwrap_used`/`expect_used` at `warn` for non-test code if the repo
   already has an `allow-unwrap-in-tests = false` posture.
4. Re-measure after the derive/file-split campaigns — they erase thousands of
   `redundant_pub_crate`/`too_many_lines` hits, so promote those last.
