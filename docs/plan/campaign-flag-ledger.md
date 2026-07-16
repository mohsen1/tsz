# Campaign flag graduation ledger (#14344 / #14345 identity + materialize-once)

Goal: **hold**. This ledger is the human-readable companion to the machine-checked
channel list in
`crates/tsz-solver/src/def/core/campaign_channels.rs::CAMPAIGN_STORE_CHANNELS`.
It exists because the identity / materialize-once campaign accumulated a family of
default-OFF env flags whose flip verdicts, blockers, and interaction contracts
previously lived only across long comment threads on #14344 / #14345 — a new
contributor could not tell which flags are flippable, blocked, or superseded
(issue #15317).

Every flag below is **default-OFF and byte-parity when OFF**: with none set, the
pipeline is identical to the historical `main` construction. The flags are
activated only through their process-global env vars.

## How the flags are exercised

- **Committed gauge**: `scripts/bench/campaign-gauge/run.sh` composes the exact
  substrate stack in one place and is the reproducible replacement for the
  hand-run gauge numbers the campaign used to quote. Run it locally with
  `scripts/bench/campaign-gauge/run.sh` (see that dir's `README.md`).
- **Scheduled lane**: `.github/workflows/campaign-flag-lane.yml` runs the gauge
  nightly and on demand (and on PRs labeled `campaign-flag-lane`). It forces
  `TSZ_DETERMINISTIC_STORE_ELECTION=1`, runs the module-augmentation
  body-publication HKT tests, asserts run-to-run determinism, and prints a
  non-gating **census** of the full solver suite. The
  census is non-gating on purpose: the 2^13 flag composition space is not
  parity-clean (many unit tests encode flag-OFF expectations), so the lane
  prints the pass/fail envelope into the job log as a crash/hang smoke plus
  snapshot (no committed baseline is diffed yet) rather than gating on zero —
  while still hard-failing on a determinism flap, an hkt-parity regression, or a
  crash/hang.

## Substrate channels (behavior-affecting)

These are the members of `CAMPAIGN_STORE_CHANNELS`. Any one being active turns on
deterministic `DefId` store election (see "Determinism derivation" below).

| Flag | Landed | Read site | Flip gate / blocker |
|---|---|---|---|
| `TSZ_INST_RESOLVER_REREDUCE` | #15055 | `instantiation/instantiate/flags.rs` | Stays OFF until the materialize-once stages publish the fully-published bodies its re-reductions need; flipping earlier ships the unbounded cross-arena `URItoKindN` growth path guarded only by the cap-4 budget (adds an emergent-termination guard — see #14346). |
| `TSZ_OPTIONB_STORE_RESOLVER` | #15220 | `instantiation/instantiate/flags.rs` (+ gate `caches/.../query_cache.rs`) | Store-only resolver shim; requires the rereduce gate + an attached `DefinitionStore`. Blocked with the rereduce chain. |
| `TSZ_INFER_HKT_REDUCE` | #15220 | `instantiation/instantiate/flags.rs`, `generic_call/normalization.rs` | Requires rereduce + option-B + an attached `DefinitionStore`. Blocked with the rereduce chain. |
| `TSZ_TYPEPARAM_DECL_IDENTITY` | #14696 | 4 sites (lowering / checker / solver) | One of the historical flap-driving channels (`DECLID + XARENA_BASE + HERITAGE`). No individual flip verdict recorded; needs lane composition data. |
| `TSZ_XARENA_BASE_DECL` | #14950 | `inference/xarena_base.rs` | Flap-driving channel; needs composed lane data before a flip verdict. |
| `TSZ_XARENA_HERITAGE_TYPEARG` | #15099 | `types/interface_type.rs` | Flap-driving channel (read in `tsz-checker`); needs composed lane data. |
| `TSZ_TYPEOF_URI_SELFLOOP` | #15095 | `def/core.rs` | Publication channel; was already in the pre-#15317 election derivation. Needs lane data. |
| `TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT` | #15220 | `def/core.rs` | Publication channel; in the pre-#15317 election derivation. Interacts with `MODULE_AUG_BODY_PUBLISH`. |
| `TSZ_MODULE_AUG_SYMBOL_EDGE` | #15119 | `def/core/augmentation_symbols.rs` | Publication channel; in the pre-#15317 election derivation. |
| `TSZ_MODULE_AUG_BODY_PUBLISH` | #15131 | `def/core/augmentation_symbols.rs` | Opt-in demand-driven publication channel; in the pre-#15317 election derivation. The three `hkt_cross_file_augmentation_13653_repro.rs` body-publication tests now run in the default focused suite through Mode B priming. |
| `TSZ_ALPHA_NAME_PAIR` | #14933 | `relations/.../functions/checking/name_pairing.rs` | Alpha-renaming name pairing; no individual flip verdict recorded. |
| `TSZ_LAZY_REF_RELATION` | #14661 | `relations/subtype/rules/generics.rs` | Lazy-ref relation fast path; no individual flip verdict recorded. |

## Related channels (not in `CAMPAIGN_STORE_CHANNELS`)

| Flag | Landed | Kind | Why excluded from election derivation |
|---|---|---|---|
| `TSZ_REREDUCE_DEPTH_CAP` | #15257 | Counter-only knob (default 4) | Tunes the rereduce depth budget; does not change which type is elected. |
| `TSZ_DETERMINISTIC_STORE_ELECTION` | #15234 | Explicit override | Directly forces deterministic election; the derivation ORs it in. |
| `TSZ_DEF_PUBLICATION_CENSUS` | — | Measurement | Emits diagnostics only. |
| `TSZ_TYPEPARAM_DIVERGENCE_PROBE` | — | Measurement | Emits diagnostics only. |
| `TSZ_XARENA_BASE_DECL_DUMP` | — | Measurement | Emits diagnostics only. |

## Determinism derivation (closed #15317 hole)

`deterministic_store_election_enabled()`
(`crates/tsz-solver/src/def/core/semantic_construction.rs`) pins `DefId`
allocation to the sound home-decl order so the composed substrate cannot
reproduce the historical run-to-run election flap. Before #15317 it keyed off
only 4 of the publication channels, so a gauge composing
`TSZ_TYPEPARAM_DECL_IDENTITY + TSZ_XARENA_BASE_DECL + TSZ_XARENA_HERITAGE_TYPEARG`
kept hash-order election while measuring. It now derives from the full
`CAMPAIGN_STORE_CHANNELS` set (plus the explicit override), so **any** active
substrate channel forces deterministic election. Enabling election is a no-op
superset — it only fixes allocation order — so a channel that turns out not to
perturb election is safe to include; omitting one that does is the bug the
derivation guards against.

## Policy

1. **No new dormant campaign flag lands without a ledger row here and coverage in
   the scheduled lane.** A flag with zero CI exercise silently rots as solver/
   checker `main` moves; the campaign has already been burned by a channel
   (`TSZ_INST_RESOLVER_REREDUCE`) silently crashing composed gauges undetected.
2. **A behavior/substrate flag must be added to `CAMPAIGN_STORE_CHANNELS`** (and
   the `every_landed_behavior_flag_is_registered` guard test updated) so the
   determinism derivation stays complete. Measurement-only or counter-only flags
   are excluded and listed under "Related channels" above.
3. **Prefer the test-override-guard pattern** (as in
   `InstResolverRereduceFlagGuard` / `DeterministicElectionGuard`) over a bare
   process-global `OnceLock`, so the flag is unit-testable rather than latched on
   first read.
4. **Record the flip verdict and blocker in this ledger** when one is
   established, with links to the deciding #14344 / #14345 comments — do not leave
   it only in thread archaeology.


## Full `TSZ_` env-flag registry (non-campaign)

The campaign tables above cover the `#14344`/`#14345` substrate. Every OTHER
`TSZ_*` env flag read anywhere in `crates/*/src` is registered here so the
`flag_ledger_guard` scan (tsz-solver test target) can enforce the #15317
policy repo-wide: a new env flag does not land without a registry entry. One
line per flag is enough; promote a flag into the campaign tables above when it
joins the substrate.

### Adjacent opt-in experiments (default OFF)

`TSZ_UNION_LITERAL_DEFAULT` (#15809 union construction-mode campaign; solver
interner + evaluate layer — moves tsz toward tsc's `UnionReduction.Literal` vs
`.Subtype` discipline. Stage 2 drops the evaluate-layer `simplify_union_members`
blanket full-relation reduce and routes the evaluate-reachable `.Subtype`
construction sites through the derived interner query `subtype_reduced`; the
literal ladder's enum-merge/intersection-absorption steps in
`normalize_union_literal_only` are also gated here. Default-OFF, flag-off
byte-identical by construction; graduation gated on the Stage-3 Pathway-A
default flip plus tsc-ward corpus/conformance/emit deltas),

`TSZ_DECL_ORIGIN_REDUCTION` (#15334 WAVE-1 sound register-through-reduction
co-walk for cross-arena HKT alpha-equivalence; relation-layer, composes with
`TSZ_ALPHA_NAME_PAIR` — candidate for promotion into the campaign table if it
proves to perturb store election),

`TSZ_SHARE_INSTANTIATION_CACHES` (#13284/#9507 cross-file instantiation-cache
sharing, safety unproven), `TSZ_EAGER_WARM_LOCAL_CACHES` (rollback hatch for
lazy local-cache warming), `TSZ_ENABLE_LIB_DEF_FREEZE` /
`TSZ_ENABLE_LIB_DEF_DEFER_PUBLISH` (lib-def freeze/defer-publish experiments),
`TSZ_LAZY_OWN_MEMBERS` / `TSZ_LAZY_OWN_MEMBERS_VARPOS` (#14957 lazy lib-member
family; VARPOS is the known opt-in regression), `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK`
/ `TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK_TINY` / `TSZ_EXPERIMENT_NO_SHARED_QC`
(parallel-check experiments), `TSZ_FILE_SESSION_REUSE` (paired with its kill
switch below), `TSZ_CROSS_FILE_TYPE_PARAMS_CACHE` (cross-file type-param cache
experiment), `TSZ_PROMOTE_FIRST` (interner promotion-order probe),
`TSZ_EVAL_MEMO_PURITY_PANIC` (debug: panic on eval-memo purity violation,
#14347 tooling), `TSZ_RECORD_FILE_SESSION_RESET_CACHE_STATS` (stats channel).

### Kill switches (default ON semantics; `TSZ_DISABLE_*` turns the mechanism off)

`TSZ_DISABLE_APP_CANON_ARG_IDENTITY`, `TSZ_DISABLE_CALLBACK_MISMATCH_MEMO`,
`TSZ_DISABLE_CHECKER_POOL`, `TSZ_DISABLE_CLOSED_EVAL_CACHE`,
`TSZ_DISABLE_CONDITIONAL_BRANCH_CACHE`,
`TSZ_DISABLE_CONTEXTUAL_RETRY_CYCLE_GUARD`,
`TSZ_DISABLE_CROSS_EVAL_APPLICATION_SENTINEL`, `TSZ_DISABLE_DECL_LAZY_LIB`,
`TSZ_DISABLE_DEFER_LAZY_DEFAULT_CONDITIONAL`,
`TSZ_DISABLE_ENCLOSING_SCOPE_CACHE`, `TSZ_DISABLE_ENV_EVAL_SEED_CAP`,
`TSZ_DISABLE_EXPORT_EQUALS_FAST_PATH`, `TSZ_DISABLE_FILE_SESSION_REUSE`,
`TSZ_DISABLE_GENUINE_UNKNOWN_ALIAS_REDUCTION`,
`TSZ_DISABLE_GLOBAL_LAZY_RECV_PRESERVE`,
`TSZ_DISABLE_HERITAGE_BASE_MEMBER_INCORP`, `TSZ_DISABLE_INSTANTIATION_CACHE`,
`TSZ_DISABLE_LAZY_MEMBER_ACCESS`, `TSZ_DISABLE_LAZY_OWN_MEMBERS`,
`TSZ_DISABLE_LIB_DEF_FREEZE`, `TSZ_DISABLE_LIB_DEF_MONOTONE`,
`TSZ_DISABLE_LIB_GENERIC_PREWARM_DEFER`, `TSZ_DISABLE_LIMIT_RESULT_CACHE`,
`TSZ_DISABLE_NATIVE_TS`,
`TSZ_DISABLE_NESTED_INDEXED_ACCESS_CONSTRAINT_REDUCTION`,
`TSZ_DISABLE_ON_DEMAND_FORCING`, `TSZ_DISABLE_ORDER_INDEP_RESOLUTION`,
`TSZ_DISABLE_REEXPORT_TYPE_ONLY_CACHE`,
`TSZ_DISABLE_RELATION_CONDITIONAL_REDUCTION`,
`TSZ_DISABLE_SOURCE_POSITION_CYCLE_GUARD`,
`TSZ_DISABLE_TYPE_POSITION_RESOLUTION_CACHE`,
`TSZ_DISABLE_UNRESOLVED_DEF_CACHE_BACKSTOP`, `TSZ_DISABLE_VARIANCE_CACHE`.

### Infra / config / tracing / debug (not graduation candidates)

Config: `TSZ_CHECKER_POOL`, `TSZ_LIB_DIR`, `TSZ_TESTS_LIB_DIR`,
`TSZ_USE_EMBEDDED_LIBS`, `TSZ_MAX_EVAL_OPS`, `TSZ_LSP_MEMORY_BUDGET_BYTES`,
`TSZ_QUERY_RUN_ID`, `TSZ_FILE_SIZE_RATCHET_UPDATE`, `TSZ_TIMEOUT_SECS`,
`TSZ_TYPESCRIPT_PACKAGE_JSON`, `TSZ_WORKER_CONFIG_DEPRECATION` (+ its
`TSZ_WORKER_CONFIG_DEPRECATION_ENV_KEY` constant).
Perf/tracing: `TSZ_PERF`, `TSZ_PERF_COUNTERS`, `TSZ_LOG`, `TSZ_LOG_FORMAT`.
Debug dumps: `TSZ_DAA_DEBUG`, `TSZ_DEBUG_CONFORMANCE_OUTPUT`,
`TSZ_DEBUG_EMIT`, `TSZ_DEBUG_PREPARE_DIR`, `TSZ_DEBUG_RESOLVE`.
