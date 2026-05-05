# Investigation: covariantCallbacks.ts conformance gap

- **Date**: 2026-05-05
- **Branch**: `claude/brave-thompson-CRvN8`
- **PR**: #2758
- **Status**: implemented
- **Workstream**: 1 (Conformance fixes)

## Intent

Reproduce, root-cause, and land
`conformance/types/typeRelationships/assignmentCompatibility/covariantCallbacks.ts`.
The test was a `fingerprint-only` failure (codes match, positions/counts
diverge): tsc emits 8 `TS2322` diagnostics, while tsz emitted 3. This PR
implements the missing `SignatureCheckMode.BivariantCallback` behavior and
the needed `isInstantiatedGenericParameter`-style guard for generic method
parameter slots.

## Root cause

`crates/tsz-solver/src/relations/subtype/rules/functions/mod.rs:378`
(`are_parameters_compatible_impl`) applies *method bivariance* uniformly to
every method parameter, including parameters that are themselves callable
(callbacks). tsc's `compareSignaturesRelated` instead branches on whether
the parameter is a callback: callback pairs go through a recursive
`compareSignaturesRelated` call with `SignatureCheckMode.Callback` set
(`TypeScript/src/compiler/checker.ts:21998-22004`), which:

1. Disables the bivariant covariant-retry for the cb's own params (they are
   strictly contravariant).
2. Allows the cb's *return* type to be checked **bivariantly** when the
   outer signature is a method (`BivariantCallback` mode, `checker.ts:22050`).
3. Skips the recursion entirely when either param is an
   `isInstantiatedGenericParameter` (`checker.ts:35758`) — i.e. the param
   slot in the *un-instantiated* signature was a generic type parameter.

The `isInstantiatedGenericParameter` check is what makes tsc accept
`Bivar1<...> = Bivar2<...>` assignments (test lines 86-87, 94-95): for
`type Bivar<T> = { set(value: T): void }`, the `value` parameter's
un-instantiated type is `T` (a generic), so tsc skips strict-callback
treatment and falls back to the normal method-bivariant compare, which
accepts both directions.

## Reproducer

```bash
.target/dist-fast/tsz \
    TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/covariantCallbacks.ts \
    --target es2015 --strict --noEmit
```

Now emits the same 8 `TS2322` diagnostics as tsc: lines 15, 20, 33, 46, 59,
72, 101, and 109.

## What shipped

1. **`is_instantiated_generic_parameter` equivalent in `tsz-solver`.**
   During method-property comparison, the checker records generic application
   receiver args. Callable method parameters that are exactly one of those args
   keep normal method bivariance, which preserves the `Bivar<T>` behavior.

2. **Callback-pair recognition in `are_parameters_compatible_impl`.**
   Detects `s_has_call && t_has_call` for method-bivariant outer slots, gated
   by (1) so instantiated generic-parameter slots do not take callback mode.

3. **`BivariantCallback` mode plumbing.** The callback-mode flag propagates
   exactly one signature comparison. Callback params are strict for that
   immediate comparison, while returned function types start fresh.

4. **Return handling.** Callback return bivariance uses the reverse raw subtype
   relation rather than `void`-target return compatibility, so `f12` still
   errors while `f14` keeps the accepted direction.

5. **Cache key partitioning.** New callback-mode flags are encoded in
   `make_cache_key` (`crates/tsz-solver/src/relations/subtype/helpers.rs:156`)
   and `RelationFlags` (`crates/tsz-solver/src/types.rs:285`) so callback-mode
   results don't poison non-callback slots.

6. **Suppress at the outer level.** When a callback-pair param is detected,
   skip the existing bivariant covariant-retry — the recursion's swapped
   `compareSignaturesRelated(targetSig, sourceSig)` call is the only
   direction that should run.

## Verified test-case decomposition

Each `function fNN` in the test file maps to a specific behavior the fix
must achieve. Confirmed against `_tsc.js` from the pinned submodule
(`TypeScript/lib/_tsc.js`, version 6.0.3):

| case | direction | tsc | tsz today | what fixes it |
| --- | --- | --- | --- | --- |
| f1 (P<A>/P<B>) | b=a | error | error | already works (variance shortcut) |
| f2 (Promise) | b=a | error | error | already works |
| f11 (AList1) | b=a | error | error | callback-pair detection + outer suppression |
| f12 (AList2) | b=a | error | error | reverse callback return check avoids `void` shortcut |
| f13 (AList3) | b=a | error | error | callback-mode arity mismatch surfaces |
| f14 (AList4) | b=a only | error | error | `BivariantCallback` return keeps a=b accepted |
| Bivar1/Bivar2 | both ok | ok | ok | generic-arg guard avoids false positives |
| sx=sy (same SetLike1) | error | error | error | already works (alias-variance shortcut) |
| s1=s2 (different aliases) | error | error | method return comparison stays strict |
| s2=s1 | ok | ok | generic-arg guard avoids false positive |

## Files Touched

- `docs/plan/claims/claude-brave-thompson-CRvN8.md` (this file)
- `crates/tsz-solver/src/relations/subtype/rules/functions/mod.rs`
- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-solver/src/relations/subtype/rules/objects.rs`
- `crates/tsz-solver/src/relations/subtype/rules/unions.rs`
- `crates/tsz-solver/src/relations/subtype/core.rs`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/tests/covariant_callbacks_tests.rs`

## Verification

- Manual reproducer above emits the 8 expected diagnostics.
- `cargo fmt --all --check`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver test_variance_`
- `cargo test -p tsz-solver relation_cache_config_tests`
- `cargo test -p tsz-checker strict_callback_param_method_tests::callback_parameter_check_is_strict_even_when_inner_signature_is_method_like -- --nocapture`
- `cargo test -p tsz-checker --test covariant_callbacks_tests`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter covariantCallbacks --verbose`
- Submodule initialized via `scripts/session/quick-pick.sh`'s
  `init_typescript_submodule` path (auto-fallback from shallow to full
  clone, see `scripts/session/pick.py:91-117`).

## Picker note

The user request asked for "a quick script to pick a random failure". The
canonical picker `scripts/session/quick-pick.sh` (delegating to
`scripts/session/pick.py`) already exists, and `.claude/CLAUDE.md` §20.25
plus `scripts/session/conformance-agent-prompt.md` explicitly forbid
creating duplicate picker scripts (PR #1957 deleted 11 such orphans). This
PR therefore reuses the canonical picker rather than introducing a ninth
wrapper. The picker correctly auto-initialised the `TypeScript` submodule
on first run.
