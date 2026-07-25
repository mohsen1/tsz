# TypeScript 7 emit baselines (overlay)

Baselines regenerated against **TypeScript 7.0.2** — the same compiler version the
conformance oracle (`scripts/conformance/tsc-cache-full.json`) is pinned to.

## Why this directory exists

`scripts/emit/run.sh` compares tsz's JS and `.d.ts` output against the baselines
checked into the `TypeScript/` submodule. Those are TypeScript 6.0-era artifacts,
so the emit suite and the conformance suite were pinned to *different* compiler
versions. That inconsistency is not cosmetic: TS 7.0.2 rejects Closure-style
`function(...)` JSDoc types with TS1005 and emits `Function` for them, while the
6.0 baselines still expect a reconstructed `(this: Object, ...args: any[]) => any`
signature. Matching one suite meant failing the other.

The submodule is read-only — `scripts/githooks/pre-commit` rejects any commit that
touches it — so regenerated baselines live here instead.

## How it works

`scripts/emit/src/runner.ts` prefers a file in this directory over the submodule
copy of the same name, and falls back to the submodule for everything else. Names
match the submodule exactly, including option suffixes such as
`foo(target=es2015).js`. The overlay may only *replace* baselines, never add or
remove them — the runner hard-fails on an overlay name with no submodule
counterpart, because a new name would change the pass-count denominator and
re-slice the emit shards.

## Regenerating

    node scripts/emit/dist/regen-baseline.js --filter=<substring> [--dry-run] [--verify]

The generator splices: it copies the existing baseline's header and input-echo
sections byte-for-byte and replaces only the emitted output sections. That avoids
having to re-derive the harness's echo format (BOM handling, unit ordering,
trailing-newline conventions), which is where faithful regeneration is hardest.

Validation rule: a regenerated baseline whose content TS6 and TS7 agree on must
come out byte-identical to the submodule copy. If a generator cannot reproduce
the agreement cases exactly, its disagreement cases mean nothing.

## Status: mechanism landed, first slice held

The overlay is empty on purpose. The first slice is generated and verified, but
landing it would publish a regression, so it is held until the tsz-side emit gaps
below are fixed.

Reproduce the slice with:

    npx tsc -p scripts/emit/tsconfig.json
    node scripts/emit/dist/regen-baseline.js --filter=jsDeclarationsRestArgs --verify
    node scripts/emit/dist/regen-baseline.js --filter=jsDeclarationsMissingTypeParameters --verify
    node scripts/emit/dist/regen-baseline.js --filter=jsDeclarationsReusesExistingNodesMappingJSDocTypes --verify

Measured effect (full emit run, with those five files present):

    JS  11562 / 11563   unchanged — every JS section regenerates byte-identically
    DTS  1372 -> 1369 / 1390

The three DTS rows that move are the three retargeted tests. They fail because tsz
still emits the TS 6.0 shape, not because the new baselines are wrong. Those are
real gaps that the stale baselines were masking:

| test | TS 6.0 baseline (what tsz emits) | TypeScript 7.0.2 |
| --- | --- | --- |
| `jsDeclarationsRestArgsWithThisTypeInJSDocFunction` | `export class` + `(this: Object, ...args: any[]) => any` | `export declare class` + `Function` |
| `jsDeclarationsMissingTypeParameters` | `func: (arg0: any[]) => any` | `func: Function` |
| `jsDeclarationsReusesExistingNodesMappingJSDocTypes` | `export const`, `unknown`, callable types, `{[x: string]: number}` | `export declare const`, `any \| null`, `Function`, `Record<string, number>` |

So the tsz-side work is: emit `declare` in JS declaration output, render an
unparseable Closure `function(...)` JSDoc type as `Function`, render `?` as
`any | null`, and render `Object.<K, V>` as `Record<K, V>`. Land those together
with the slice so no published number goes backwards — `scripts/refresh-readme.py`
deliberately refuses to lower the README emit metrics.

## Do not

- Write into `TypeScript/` or bump the submodule pointer; the pre-commit hook
  blocks both, and moving the pointer would change the test population and both
  pass-count denominators.
- Regenerate the whole corpus in one change. An independent census puts the
  change set near 286 tests, worth roughly JS -69 and DTS -199 currently-passing
  rows. That is a gated campaign that needs tsz fixes first, not a baseline swap.
- Repoint `build-baseline-blob.ts` at this directory. The blob is a read cache of
  submodule bytes; correctness comes from the runner checking the overlay first.
