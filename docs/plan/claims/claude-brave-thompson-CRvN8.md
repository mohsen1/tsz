# Investigation: covariantCallbacks.ts conformance gap

- **Date**: 2026-05-05
- **Branch**: `claude/brave-thompson-CRvN8`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Reproduce, root-cause, and document the work needed to land
`conformance/types/typeRelationships/assignmentCompatibility/covariantCallbacks.ts`.
The test is a `fingerprint-only` failure (codes match, positions/counts
diverge): tsc emits 8 `TS2322` diagnostics, tsz currently emits 3. Closing
the gap requires implementing tsc's `SignatureCheckMode.BivariantCallback`
semantics plus an equivalent of `isInstantiatedGenericParameter`. Both are
non-trivial solver work that this claim documents but does not yet ship.

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

Currently emits 3 diagnostics (lines 15, 20, 101). Tsc expects 8
(adds lines 33, 46, 59, 72, 109).

## What needs to ship

1. **`is_instantiated_generic_parameter` equivalent in `tsz-solver`.**
   Requires either (a) preserving an `Option<FunctionShape>` "target" pointer
   on instantiated `FunctionShape` instances, or (b) recording per-`ParamInfo`
   which params originated as `TypeData::TypeParam`/`TypeData::Lazy(DefId)`
   pre-substitution. (a) is closer to tsc's design.

2. **Callback-pair recognition in `are_parameters_compatible_impl`.**
   Detect `s_has_call && t_has_call`, gated on (1) above so it does not fire
   for instantiated generic-parameter slots.

3. **`BivariantCallback` mode plumbing.** Add a flag (extending
   `IN_CALLBACK_PARAM_CHECK` or a new `IN_BIVARIANT_CALLBACK_RETURN`) that
   propagates exactly one level into the recursive
   `check_function_subtype_impl`. Inside that recursion, `check_return_compat`
   must allow either direction of subtype to satisfy the return relation.

4. **Cache key partitioning.** New mode flags must be encoded in
   `make_cache_key` (`crates/tsz-solver/src/relations/subtype/helpers.rs:156`)
   and `RelationFlags` (`crates/tsz-solver/src/types.rs:285`) so callback-mode
   results don't poison non-callback slots.

5. **Suppress at the outer level.** When a callback-pair param is detected,
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
| f11 (AList1) | b=a | error | **silent** | (2)+(5): callback-pair detection + outer suppression |
| f12 (AList2) | b=a | error | **silent** | (2)+(5): return-type mismatch surfaces under strict contravariant |
| f13 (AList3) | b=a | error | **silent** | (2)+(5): arity mismatch surfaces |
| f14 (AList4) | b=a only | error | **silent** | (2)+(3)+(5): needs `BivariantCallback` return so a=b stays accepted |
| Bivar1/Bivar2 | both ok | ok | ok | (1) is required: without it, naive (5) emits two false positives |
| sx=sy (same SetLike1) | error | error | error | already works (alias-variance shortcut) |
| s1=s2 (different aliases) | error | **silent** | (2)+(5): get() return contravariance surfaces |
| s2=s1 | ok | ok | (1)+(2)+(5): without (1), naive fix emits a false positive |

## Files Touched

- `docs/plan/claims/claude-brave-thompson-CRvN8.md` (this file)

## Verification

- Manual reproducer above confirms 3-vs-8 gap.
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
