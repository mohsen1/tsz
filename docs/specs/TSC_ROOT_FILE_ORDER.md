# tsc Root-File Order: Canonical Reference

How `tsc` orders the program's **root files**, and why that order is load-bearing
for tsz.

**Verified against:** TypeScript **7.0.2**
(`scripts/node_modules/@typescript/typescript-darwin-arm64/lib/tsc`), invoked
directly, including with `--singleThreaded --stableTypeOrdering true` — the flags
`generate-tsc-cache.rs` uses for TS7+.

## The rule

> Root files are bucketed by the index of the **`include` pattern** that matched
> them, concatenated in pattern order, and sorted **alphabetically within each
> bucket**. Files listed in `files` (or as CLI positionals) come first, in the
> order given.
>
> Extension family does **not** affect ordering.

`matchFiles` collects into `results[includeIndex]` and flattens in pattern order.
`readDirectory` receives an already-**flattened** extension list, so the
extension groups (`[.ts .tsx .d.ts]`, `[.cts .d.cts]`, `[.mts .d.mts]`,
`[.js .jsx]`, …) only feed `hasFileWithHigherPriorityExtension` — a
**same-basename dedup** rule (`foo.ts` beats `foo.js`), never an ordering rule.

## The measurement

Two files, identical across every row; only `include` changes. Both are global
scripts, so they merge and `TS2403` anchors on the **subsequent** declaration —
the file *not* reporting is the one that came first.

```ts
// a.js
var x = 1;
// b.ts
var x = "s";
```

| `include` | first file |
|---|---|
| `["*.js","*.ts"]` | **a.js** |
| `["*.ts","*.js"]` | **b.ts** |
| `["*"]` | **a.js** (alphabetical) |
| `["**/*"]` | **a.js** (alphabetical) |
| *absent (default)* | **a.js** (alphabetical) |

Swapping the two patterns swaps the order. No extension-family rule can produce
that. With the default single `**/*` pattern there is exactly one bucket, so the
result is pure alphabetical order.

## The refuted claim, and why it keeps recurring

> ~~"tsc visits the TypeScript extension group before the JavaScript group, so a
> `.ts` file always precedes a sibling `.js` file regardless of name."~~

This is **false**, and it has been proposed at least three times (#17410,
#17423, #17520), twice described as "oracle-verified". #17423 landed on it and
was reverted in #17428 after it inverted the `TS2403` anchor for every
default-`include` project.

The inference that produces it is always the same, and it is seductive because
each half is true:

1. Passing `a.js b.ts` versus `b.ts a.js` on the **command line** flips which
   file wins. *(True — program order decides the winner.)*
2. Therefore glob discovery must emit TS-family-first. *(Does not follow.
   Different mechanism.)*

A probe using `include: ["*.ts","*.js"]` appears to confirm it, because that
varies extension family and pattern order **together**. The discriminating row
is `["*.js","*.ts"]`: an extension bucket predicts `b.ts` first, pattern-index
predicts `a.js`. The oracle says `a.js`.

## How to verify an ordering claim

Invoke the pinned binary **directly** — it is a native Go binary, so `node <path>`
fails on it:

```bash
scripts/node_modules/@typescript/typescript-darwin-arm64/lib/tsc \
  --noEmit --pretty false <flags> a.js b.ts
```

`scripts/conformance/oracle.sh` accepts only one `FILE` positional and appends
it **last**, so a two-file invocation silently reorders. It now rejects a second
source positional (#17489) rather than producing a confidently wrong answer, but
prefer the direct binary for anything order-sensitive.

Use a real (non-symlinked) directory: on macOS the `/tmp` symlink can cause files
to resolve twice.

## tsz status

`discover_ts_files` (`crates/tsz-cli/src/project/fs.rs`) funnels every walk root
into a single `BTreeSet<PathBuf>`, i.e. purely alphabetical.

- **default / single-pattern `include`** — correct, because tsc's default is the
  single pattern `**/*`, which is also alphabetical.
- **multi-pattern `include` with `.ts` patterns listed first** — still wrong;
  tsz sorts alphabetically where tsc would honour pattern order. This is the
  real, open defect.

The correct fix is one bucket per **user include-spec index**, concatenated in
pattern order, alphabetical within each, with a file assigned to the first
pattern that matches.

**Trap:** `default_discovery_include_patterns`
(`crates/tsz-common/src/file_extensions.rs`) synthesizes a *multi-pattern
per-extension* list (`*.ts`, `*.tsx`, `*.mts`, `*.cts`, then the JS family),
while tsc's real default is the single `**/*`. Bucketing naively by
expanded-pattern index would therefore put TS ahead of JS in the **default**
case and reintroduce exactly the regression #17428 reverted. The synthesized
defaults must collapse into one bucket.

## Related: same-basename shadowing

Distinct from ordering, and genuinely extension-driven: when wildcard discovery
finds two files with the same stem, tsc keeps only the highest-priority
extension and drops the rest (`foo.ts` shadows `foo.js`). Verified with a
positive control — different stems admit both and conflict, same stems produce
silence. Implemented in `exclude_shadowed_js_files` (#17478).

So a same-stem `.ts`/`.js` pair raises no ordering question at all; the `.js` is
not in the program.
