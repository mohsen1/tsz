# #14344 content-addressing flip — end-to-end validation harness

Validates the #14344 canonical-`DefId` flip (issue #18 / dualenv brick-3 election
wiring) against the #13862 cross-arena wrong-decl identity-collision witness.

## What it measures

Per `RAYON_NUM_THREADS` in {1,2,4,8,16}, with `TSZ_CANONICAL_DEFID` both OFF and ON:

1. The **#14520 counter** `identity_collision_wrong_decl_suppressed` (printed by the
   `--extendedDiagnostics` perf-counter dump as `wrong-decl collisions`). It increments
   in `tsz-solver/src/def/resolver.rs::raw_symbol_fallback_def` whenever a
   store-registered `DefId(N)`, reread as a `SymbolId`, would resolve to a
   **different-named** def — the #13862 `HTMLDivElement(218) → FileSystemEntry(symbol 218)`
   class of collision (the guard defers instead of resolving wrong).
2. The **md5 of the sorted diagnostic output** (the observational behavior).

## Witness fixture

`fixtures/dom_multi.tsconfig.json` — a 3-file project (`widgets.ts`/`forms.ts`/`app.ts`)
that indexes `HTMLElementTagNameMap[...]` (div/span/a/input/button) and threads those
DOM types across module boundaries. The DOM lib drives the collision: lib symbols keep
the `u32::MAX` declaration-file sentinel, so the symbol→def index is first-writer-wins
across lib binders, and `HTMLElementTagNameMap["div"]` resolution hits the colliding
fallback. (A pure user-type cross-module fixture — `fixtures/{shared,mod_a,mod_b,entry}.ts`
— is kept as a NEGATIVE control: it fires the counter 0 times, confirming the collision
is lib-symbol-id-space specific, not generic cross-module.) `fixtures/dom_tagname.ts` is
the minimal single-file variant (counter 36).

`fixtures/div_fsentry_isolation.tsconfig.json` — smallest single-file witness that
keeps both colliding names live: `HTMLElementTagNameMap["div"]` (resolves
`HTMLDivElement`) plus a `FileSystemEntry` reference (the documented
`HTMLDivElement(218) → FileSystemEntry(symbol 218)` collision in
`raw_symbol_fallback_def`). Counter 44, md5 `d41d8cd9…` (clean). Use this for
focused flag-ON two-pass debugging.

## Baseline (flag-OFF, current main `a3ee589afe`, 2026-06-22)

```
threads | OFF_counter | OFF_md5                          | ON_counter | ON_md5
1..16   | 44          | d41d8cd98f00b204e9800998ecf8427e | 44         | d41d8cd98f00b204e9800998ecf8427e
```

- Counter is a **stable 44** on every thread count (single-file variant: 36).
- md5 `d41d8cd9…` is the empty-string md5 — **0 user diagnostics** (the collision is
  internally suppressed by the #13862 defer guard; it corrupts identity, not output).
- flag-ON == flag-OFF today: `TSZ_CANONICAL_DEFID` is not recognized yet (PR1/brick-1
  hasn't landed), so the flip is a no-op. **Expected.**

## Flip success criteria (what brick-3 must achieve)

- **flag-ON `counter == 0`** on every thread count — canonical content-addressed identity
  removes the collision entirely (no def resolves to a different-named def).
- **flag-ON `md5 == flag-OFF md5`** (`d41d8cd9…`) on every thread count — the flip fixes
  identity only; it must be observationally byte-identical on diagnostics.
- flag-OFF md5 stays stable across threads (regression guard).

## Run

```
scripts/bench/canonical-defid-harness/measure.sh [fixture-tsconfig] [tsz-bin]
# defaults: fixtures/dom_multi.tsconfig.json + .target/release/tsz
```

Build the release binary first (`cargo build --release -p tsz-cli`). Release only —
a checker-test build ENOSPCs the shared volume.
