# tsz

<p class="subtitle"><code>tsz</code> is a clean-slate experiment to match the pinned TypeScript 7 compiler exactly in Rust, then make correct projects faster.</p>

<blockquote class="wip-warning">
<p><strong>Foundation validation only.</strong> The retired compiler was deleted. Releases, npm packages, installers, and the WASM playground are intentionally unavailable while the replacement is being proved.</p>
</blockquote>

## What exists today

The R0 rewrite is a small end-to-end validation slice: source text can pass through fresh scan, parse, bind, check, diagnostic, and JavaScript emit paths. Exact seed cases prove a narrow set of declarations, annotations, assignments, calls, returns, object properties, unions, and emit behavior against TypeScript 7.0.2.

That slice is not a general-purpose TypeScript compiler, language service, or drop-in replacement for `tsc`. Most grammar, project, module, library, declaration-emit, language-service, and ecosystem behavior remains unsupported.

## How progress is reported

The rewrite keeps three records separate:

1. the frozen legacy checkpoint, which describes the deleted compiler;
2. exact capabilities proved by the rewrite's seed tests;
3. full-corpus observations, including unsupported and failed cases.

Historical compatibility and performance numbers are evidence about the retired implementation. They are not current rewrite results. Speed comparisons count only after a project produces the same result as the pinned TypeScript 7 oracle.

Read the [clean-slate roadmap](https://github.com/tsz-org/tsz/blob/main/docs/plan/ROADMAP.md) for the milestones and conviction gates.
