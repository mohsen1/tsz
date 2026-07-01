# Identity / heritage / termination regression boundaries (#14350)

This document is the triage contract for three regression *classes* that share a
single root (non-canonical, allocation-order identity — #14344) and its
downstream effects (no materialize-once + SCC — #14345; emergent termination —
#14346). Historically each new witness of these classes spawned its own
`green/*` branch ("whack-a-mole"), because the witnesses *look* independent.
They are not: they are re-witnesses of the same 2-3 roots (see
`docs/plan/ROADMAP.md` and issue #14351).

The fix for the whack-a-mole is **not** more branches — it is to ratchet each
class as a **boundary**: a new witness becomes one more case in an existing
floor test (or counter), not a new branch. When a root finally lands, these
boundaries are the parity floor that proves it did not regress the class.

## The three boundary classes and where each is ratcheted

### 1. Identity (non-canonical `SymbolId`/`DefId` collisions — #14344)
- **Floor tests** (diagnostic-based, both-file-orders, structure-not-spelling,
  with negative controls):
  - `crates/tsz-cli/tests/cross_file_local_callee_symbol_identity_tests.rs`
  - `crates/tsz-cli/tests/cross_file_imported_const_computed_key_identity_tests.rs`
- **Measurement layer** (the quantified witness, trends to zero as the flip
  lands): the `identity_collision_wrong_decl_suppressed` perf counter (#14520),
  with `symbol_def_index_lookup_hits`/`_misses` as the denominator. The composed
  flag stack is exercised by the committed gauge
  `scripts/bench/campaign-gauge/` and the `campaign-flag-lane` CI lane (#15317),
  which superseded the dormant `canonical-defid-harness` (it measured
  `TSZ_CANONICAL_DEFID`, a flag no crate reads post-#14558).
- **Triage a new identity witness here:** add a both-orders case to a floor
  test above; do not open a `green/*` branch.

### 2. Heritage (cross-arena class-instance member drop — #14345/#13255)
- **Floor:** the cross-file heritage / class-to-instance adoption guards in the
  CLI driver and checker cross-file paths (e.g. the relation-path store-wiring
  work, #14345 relpath). A member-drop FP on a cross-arena heritage instance is
  a witness of the same identity root (the published class instance read with
  `has_store=false`), not an independent bug.
- **Triage:** add the cross-file member-retention case to the heritage floor;
  do not branch per-project.

### 3. Termination (recursion/fuel/frame limits — #14346)
- **Floor tests:**
  - `crates/tsz-solver/tests/evaluate_tests_parts/iteration_exceeded_incomplete.rs`
    (the `IterationExceeded -> Termination::Incomplete` channel, #14346 stage 2).
  - The instantiation-cache limit-gate tests in
    `crates/tsz-solver/src/caches/instantiation_cache_test.rs` (a limit-tripped
    result — depth / union-too-complex / tuple-too-large / frame-curtailment /
    fuel / poison — must NOT be cached, so the diagnostic re-fires; #14345
    keystone, PR #14580).
- **Triage:** add the limit case to a termination floor; do not branch.

## The decomposition signal (file sizes — #14345/#14346)
`crates/tsz-solver/src/tests/file_size_baselines.txt` carries an annotation: the
over-ceiling solver engines (`evaluate`, `eval_conditional`, `instantiate`,
`subtype_core`, `generic_call_resolve`, `relations_compat`, `type_queries_flow`)
are monolithic *because the type model has no clean decomposition for recursive
/ cross-arena materialization* — each recursion/heritage fix accretes lines to
whichever engine the witness lands in. A rise there is a signal to ask "is this
another witness of the identity/materialize root?" before bumping a ceiling.
They decompose downstream of the roots landing, not by cosmetic splitting.

## Rule of thumb
A new FP/timeout that looks like one of these classes is almost certainly a
re-witness of #14344/#14345/#14346 (CLAUDE.md: "the reported repro is one
witness, not the scope"). Add a boundary case here and reference the root issue;
reserve a dedicated branch for an actually-novel root, not another witness.
