# tsc Root-File Order: Canonical Reference

How `tsc` orders the program's **root files**, and why that order is load-bearing
for tsz.

**Verified against:** TypeScript **7.0.2**
(`scripts/node_modules/@typescript/typescript-darwin-arm64/lib/tsc`), invoked
directly, including with `--singleThreaded --stableTypeOrdering true` — the flags
`generate-tsc-cache.rs` uses for TS7+.

## The rule

> Root files are bucketed by the index of the **`include` spec** that matched
> them and concatenated in spec order. Within one bucket they follow tsc's
> **directory walk**: a directory's own files (sorted) come before its
> subdirectories (sorted), recursively — *not* a sort of whole paths. Files
> listed in `files` (or as CLI positionals) come first, in the order given.
>
> Extension family does **not** affect ordering.

Two independent layers, and a probe can satisfy one while violating the other:

| layer | keyed on | separated by |
|---|---|---|
| across buckets | user include-**spec** index | `["*.js","*.ts"]` vs `["*.ts","*.js"]` |
| within a bucket | directory walk, files before subdirectories | a root file whose name sorts *between* two subdirectory names |

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
result is the plain walk of one directory — which for a flat project is
alphabetical.

### Within one bucket: files before subdirectories

Every row above is a **flat** directory, where the walk and a whole-path sort
agree. They diverge as soon as a subdirectory is involved. Same setup, one
`include: ["**/*.ts"]` spec, three files:

```ts
// mmm.ts      var p = 1;  var q = 1;
// aaa/x.ts    var p = "s"; var r = "s";
// zzz/y.ts    var q = true; var r = true;
```

Each variable pairs two files, and its `TS2403` lands on the later one, so the
three errors pin the total order:

| order | result |
|---|---|
| **tsc** | `mmm.ts`, `aaa/x.ts`, `zzz/y.ts` |
| whole-path sort | `aaa/x.ts`, `mmm.ts`, `zzz/y.ts` |

tsc's `visitDirectory` emits the files of the directory it is visiting before
recursing into that directory's subdirectories, so `mmm.ts` precedes both
subdirectories even though `aaa/` sorts before it. A lexicographic sort of whole
paths interleaves a subdirectory's files among its parent's whenever a parent
file name sorts between two subdirectory names. The rule applies at every depth.

## The refuted claim, and why it keeps recurring

> ~~"tsc visits the TypeScript extension group before the JavaScript group, so a
> `.ts` file always precedes a sibling `.js` file regardless of name."~~

This is **false**, and it has been proposed at least four times (#17410, #17423,
#17520, #17545), repeatedly described as "oracle-verified". #17423 landed on it
and was reverted in #17428 after it inverted the `TS2403` anchor for every
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

`discover_ts_files` (`crates/tsz-cli/src/project/fs.rs`) implements both layers,
and matches the oracle on every row in this document:

- **bucketing by user include-spec index** — #17540. A file is assigned to the
  first spec that matches it, evaluated **relative to the tsconfig directory**;
  matching the absolute path first let a later spec's recursive glob
  (`**/*.ts`, which crosses `/`) claim a file that a directory-scoped earlier
  spec (`sub/*`) should own, collapsing the buckets.
- **walk order within a bucket** — `compare_discovery_order`, which orders a
  directory's own files ahead of its subdirectories rather than sorting whole
  paths.

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
