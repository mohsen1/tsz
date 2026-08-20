# `try-tsz` Plan

> **Rewrite status:** This is a future product specification, not a description
> of the R0 binary. The clean-slate workspace preserves the native `try-tsz`
> process name, but package publication and project-adoption claims are disabled
> until broad project parity is trustworthy. There is currently no supported
> `npx try-tsz` release.

## Summary

The eventual `try-tsz` product is an adoption probe for TypeScript users who
already run `tsc`. Once publication resumes, the user will run:

```sh
npx try-tsz
```

The tool finds the relevant `tsconfig.json`, runs a TypeScript 7 oracle at
version `7.0.2` or newer and the latest published `tsz` in a no-emit check,
compares the complete diagnostic result, reports speed, and guides the user
through an interactive, local-only bug-report flow when `tsz` differs.

This is not switch/migration messaging. The user-facing promise is:

> `tsz` is not ready to replace `tsc` yet. This checks whether `tsz` already
> works on your project and helps report what is still missing.

## Product Decisions

- Audience: any project that uses `tsc` for type checking or emit.
- MVP scope: type-check parity only, always run both compilers with `--noEmit`.
- Compatibility bar: `tsz` matches TypeScript `tsc` `7.0.2` or newer exactly
  for exit status and diagnostics.
- Diagnostic comparison: compare file, line, column/span when available, code,
  message text, and order.
- Correctness wins the headline. If `tsz` is faster but wrong, report mismatch
  first and speed second.
- If `tsc` has diagnostics, still run `tsz`; the result is parity comparison,
  not a "project works cleanly" result.
- Emit comparison is explicitly out of scope for v1.
- Privacy default: no upload, no source sharing, and no source extraction until
  the user approves an interactive prompt.
- Reports go to GitHub Discussions for `tsz-org/tsz`, because issues are not
  user-opened today.
- Prefer interaction over secondary commands or submission flags. A single run
  should naturally ask whether to prepare, review, and submit/report.

## CLI UX

Default flow:

```text
$ npx try-tsz

try-tsz checks your project locally. It will not upload source code.

Found tsconfig.json
Running tsc --noEmit -p tsconfig.json ...
Running tsz --noEmit -p tsconfig.json ...

Result: tsz matched tsc on this project.
tsc: 18.42s
tsz:  6.31s
Speed: 2.9x faster

tsz works for your project!
```

Mismatch flow:

```text
Result: mismatch
tsc: 12 diagnostics in 18.42s
tsz: 14 diagnostics in  6.31s

Differences:
- 2 extra tsz diagnostics
- 0 missing tsc diagnostics
- 0 reordered diagnostics

Prepare a local report? This may copy small snippets around failing spans.
No files are uploaded. You can review everything before sharing. [y/N]
```

Interactive report prompts:

1. Ask whether to generate `.try-tsz/report.md`.
2. Ask whether to create snippet repro candidates under `.try-tsz/repros/`.
3. Ask whether to open the report in the user's editor or print the paths.
4. Ask whether to submit with GitHub CLI if authenticated.
5. If submission cannot run, print a prefilled Discussion URL and the Markdown
   file path.

Supported command surface:

- `npx try-tsz`: discover config and run the interactive flow.
- `npx try-tsz -p <path>` / `--project <path>`: use one explicit config.
- `npx try-tsz --all`: scan for multiple configs and check them one by one.
- `npx try-tsz --json <path>`: write machine-readable output for CI/harnesses.

Do not add a `--submit` workflow for v1; submission stays part of the
interactive mismatch flow.

## Config Discovery

- With `-p/--project`, accept either a config file or a directory containing
  `tsconfig.json`, matching `tsc`/`tsz` convention.
- Without `-p`, walk upward from the current directory and use the nearest
  `tsconfig.json`.
- If no config is found, explain that `try-tsz` needs a TypeScript project and
  exit with setup failure.
- If multiple configs are discovered only because the user requested `--all`,
  skip ignored/generated directories such as `node_modules`, `dist`, `build`,
  `coverage`, `.next`, `.turbo`, `.git`, and vendored dependency folders.
- Project references are first-class through the chosen root config. Do not
  invent a flat config or rewrite references in v1.

## Implementation Shape

Use a Rust-native CLI packaged for npm rather than a WASM-first CLI.

Reasoning: `try-tsz` must run the Node-based TypeScript oracle helper, invoke
`tsz`, inspect the filesystem, write local reports, optionally call `gh`, and
open URLs. Native Rust maps to that job cleanly. WASM remains useful for
browser/library surfaces, but it adds friction for process orchestration.

Repository layout:

- Add a Rust binary target named `try-tsz` to `crates/tsz-cli`.
- Add reusable modules under `crates/tsz-cli/src/try_tsz/` for discovery,
  runner, diagnostic comparison, snippets, report writing, and GitHub
  submission.
- Extend the npm assembly scripts so the public unscoped package `try-tsz`
  installs a launcher that selects the platform binary.
- Publish scoped native helper packages named
  `@mohsen-azimi/try-tsz-<platform>` so the public package can stay
  `try-tsz` while helper package names avoid npm's unscoped-name spam gate.

Compiler invocation:

- Use a TypeScript `7.0.2` or newer oracle. The npm `try-tsz` package pins
  TypeScript `7.0.2`; if a non-npm/local build cannot find the packaged oracle,
  a project TypeScript install at `7.0.2` or newer may be used.
- Collect the `tsc --pretty false --noEmit -p <config>` equivalent through
  TypeScript 7's `typescript/unstable/sync` API. The root `typescript` export
  supplies the version only; it no longer exposes the stable compiler API used
  by TypeScript 6 and earlier.
- Invoke the packaged latest `tsz` binary as an isolated worker with
  `tsz --pretty false --noEmit -p <config>`.
- Measure total wall-clock time for each compiler from process spawn through
  exit.
- Capture stdout, stderr, exit code, signal/termination state, and timeout.
- Keep the default timeout generous and visible in the report; classify timeout,
  OOM/killed, crash, nonzero, and diagnostic mismatch.

Structured diagnostics:

- Keep `tsz` diagnostics machine-readable rather than parsing rendered text.
- The Node oracle helper dynamically imports `typescript/unstable/sync`, opens
  the selected config with `API.updateSnapshot({ openProjects: [config] })`,
  runs binding, and collects config, syntactic, program, global, semantic, and
  (when applicable) declaration diagnostics from the resulting project. It
  always disposes the snapshot and closes the API session.
- TypeScript 7 reports source offsets in UTF-16 code units. The helper converts
  them to UTF-8 byte offsets before Rust compares them with `tsz` diagnostics.
- Compare the structured `tsc` diagnostics to structured `tsz` diagnostics.
- Include the raw command output in `.try-tsz/raw/` only when the user chooses
  to generate a report.

No-write JSONC overlay:

- Read the root `tsconfig` as JSONC with comments and trailing commas enabled.
- Refuse ambiguous or unsafe shapes: duplicate `compilerOptions`, duplicate
  `compilerOptions.noEmit`, a non-object `compilerOptions`, or a non-boolean
  `noEmit`.
- If `noEmit` is not already `true`, use `jsonc-parser` to create an in-memory
  edit that sets `compilerOptions.noEmit` to `true` while preserving the
  config's indentation, newline style, comments, and unrelated fields.
- Expose that edited text only through the sync API's virtual `readFile`
  callback for the root config. Returning `undefined` for every other path
  delegates to the real filesystem. The original config, source tree,
  lockfiles, dependency directories, and build-info files are not written.
- Map any config diagnostic span through the inverse edit before reporting its
  original line, column, and byte range.

## Report And Privacy

Report artifacts live under `.try-tsz/` and are not uploaded automatically:

```text
.try-tsz/
  report.md
  summary.json
  raw/
    tsc.txt
    tsz.txt
  repros/
    mismatch-001.ts
```

Default report contents:

- `try-tsz`, `tsz`, `typescript`, Node, OS, arch, and package-manager versions.
- Config path relative to the project root.
- Whether project references were present.
- File count and approximate TypeScript-family LOC, excluding dependency and
  generated directories.
- `tsc` and `tsz` command lines.
- Wall-clock timings.
- Failure classification.
- Diagnostic diff grouped into missing `tsc` diagnostics, extra `tsz`
  diagnostics, message/span/order mismatches, crashes, setup failures, and
  timeouts.
- Redacted paths by default: relative paths are shown; absolute home/project
  prefixes are not.

Do not include dependency lists, package name, lockfile content, full
`tsconfig`, environment variables, or full source files by default.

Snippet repro MVP:

- Only offer snippets after the user confirms report generation.
- For each mismatch with a source span, extract the nearest enclosing
  declaration/function/type/interface/class when it can be found cheaply.
- Include a small import/context header only when directly adjacent and obvious.
- Redact absolute paths.
- Write best-effort snippets and clearly label them as candidates.
- If extraction is uncertain, skip the snippet and leave the diagnostic summary.
- Do not implement project-wide delta debugging/reduction in v1.

GitHub Discussions:

- Target repository: `tsz-org/tsz`.
- Target category: `General`.
- Title prefix: `[try-tsz]`.
- Prefer authenticated submission through `gh api graphql` using
  `createDiscussion`.
- If `gh` is unavailable, unauthenticated, or lacks the needed permissions,
  write `report.md`, copy/print the body when possible, and print a prefilled
  Discussions URL:

```text
https://github.com/tsz-org/tsz/discussions/new?category=general&title=...&body=...
```

## Result Model

`try-tsz` should classify each checked config into one state:

- `matched-clean`: `tsc` and `tsz` both succeed with no diagnostics.
- `matched-diagnostics`: both report the same diagnostics.
- `mismatch`: both run, but diagnostics differ.
- `tsz-crash`: `tsz` aborts, panics, segfaults, or exits through a crash signal.
- `tsz-timeout`: `tsz` exceeds timeout.
- `tsz-oom`: `tsz` is killed in an OOM-like way.
- `setup-failure`: config, `tsc`, `tsz`, Node, or filesystem prerequisites are
  missing.

Exit code contract:

- `0`: all checked configs matched.
- `1`: `tsz` ran but did not match `tsc`, crashed, timed out, or was killed.
- `2`: setup/config/tooling failure prevented a valid comparison.

Speed is reported for every compiler that completed. Slower `tsz` is not a
failure when diagnostics match.

## Testing Plan

Rust unit tests:

- Config discovery: nearest config, explicit file, explicit directory, no
  config, ignored directories, and `--all`.
- Diagnostic comparator: exact match, extra, missing, span mismatch, message
  mismatch, order mismatch, and duplicate diagnostics.
- Result classifier: clean match, diagnostics match, mismatch, crash, timeout,
  OOM/killed, and setup failure.
- Path redaction and report serialization.
- Snippet extraction for type aliases, interfaces, classes, functions, nested
  declarations, and fallback skip cases.

Integration tests:

- Temp project with local `typescript` dependency and clean `tsc` result.
- Temp project where both compilers report the same TypeScript error.
- Temp project where fixture output simulates an extra `tsz` diagnostic.
- Interactive mismatch flow using scripted stdin.
- `gh` fallback when unavailable and GraphQL submission path behind a mocked
  command runner.

Packaging tests:

- Local npm pack installs `try-tsz` and exposes the `try-tsz` binary.
- Platform binary selection uses the matching
  `@mohsen-azimi/try-tsz-<platform>` helper package.
- `npx try-tsz -p tsconfig.json` works in a sample project without modifying
  source, lockfiles, or dependency folders.

Manual smoke:

```sh
cargo nextest run -p tsz-cli try_tsz
scripts/build/build-npm-packages.sh --local
cd <temp-ts-project>
npx <path-to-packed-try-tsz.tgz>
```

## Rollout

1. Land the Rust CLI and local report flow behind unpublished packaging.
2. Add npm assembly for the unscoped `try-tsz` package.
3. Publish an initial pre-release and test with selected real projects.
4. Publish latest once the interactive report flow and privacy copy are stable.
5. Link `npx try-tsz` from README/site with clear "not ready to switch"
   language.

## Open Follow-Ups After V1

- Emit comparison.
- More deterministic reducer/delta-debugger.
- Better structural snippet generation using TSZ/TypeScript AST APIs.
- CI-oriented noninteractive mode beyond `--json`.
- Optional project-corpus ingestion for user-submitted public repros.
