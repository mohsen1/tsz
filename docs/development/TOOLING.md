# TSZ Tooling Reference

This page lists the tools that remain useful during the clean-slate rewrite.
Commands target the three-package workspace: `tsz-core`, `tsz-cli`, and
`tsz-conformance`.

## Setup And Intake

```bash
./scripts/setup/setup.sh
scripts/setup/disk-worktree-guard.sh
git worktree list
```

The setup script installs hooks and initializes the pinned TypeScript checkout.
Use the disk guard before new worktrees or memory-heavy builds. Preserve useful
Rust and TypeScript caches when reclaiming space.

## Fast Compiler Loop

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo nextest run -p tsz-core --test rewrite_foundation
cargo nextest run -p tsz-cli --test rewrite_process_contract
```

Use `cargo nextest`, not `cargo test`. A full strict rewrite check is:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
python3 scripts/arch/arch_guard.py
python3 -m unittest discover scripts/arch -p 'test_arch_guard*.py' -v
```

The architecture guard enforces the workspace graph, dependency direction,
anti-hardcoding rules, retired-surface removal, and the 2,000-line file limit.

## Native Binaries

Build the four R0 adapters together:

```bash
cargo build -p tsz-cli \
  --bin tsz --bin tsz-server --bin tsz-lsp --bin try-tsz
```

Useful smoke launches:

```bash
cargo run -p tsz-cli --bin tsz -- --help
cargo run -p tsz-cli --bin try-tsz -- --help
```

`tsz-server` uses the retained framed protocol, and `tsz-lsp` is an honest
adapter over the service API. Exercise protocol behavior through process tests
instead of importing CLI internals. There is no browser/WASM build in R0; that
surface returns in R4.

## TypeScript 7 Oracle

The pinned TypeScript 7.0.2 checkout is the behavioral source of truth. Oracle
evidence should include the exact input and all applicable outputs:

- diagnostic code, file, normalized start/length, and message chain;
- process exit status;
- JavaScript and declaration output;
- root-file order and compiler options;
- oracle commit and declared threading mode.

Do not edit the submodule. If its test corpus or libraries are missing, rerun
setup rather than substituting a different TypeScript installation.

## Conformance

Launch one narrow case:

```bash
./scripts/conformance/conformance.sh run \
  --filter '<case>' --max 1 --workers 1 --verbose
```

Inspect existing result artifacts without launching the compiler:

```bash
python3 scripts/conformance/query-conformance.py
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --close 2
```

Never run two conformance invocations concurrently in one worktree. Full-corpus
results are observations during R0/R1; keep unsupported and crashed cases in
the result. A seed capability becomes a floor only when the roadmap declares
it supported.

## Emit

```bash
./scripts/emit/run.sh --filter='<case>' --max=1 --verbose
./scripts/emit/run.sh --filter='<case>' --max=1 --js-only
./scripts/emit/run.sh --filter='<case>' --max=1 --dts-only
```

Use `--json-out=<path>` for a machine-readable observation. JavaScript emit
uses syntax; declaration emit uses explicit checked summaries. An emit test
must not be made to pass by reparsing or patching rendered output.

## Fourslash And Server

```bash
cargo build --release -p tsz-cli --bin tsz-server
./scripts/fourslash/run-fourslash.sh \
  --filter='<case>' --max=1 --sequential --skip-cargo-build
```

Use `--json-out=<path>` when preserving an observation. Fourslash is broad
legacy evidence until each language-service behavior is ported through the
public service API.

## Project And Performance Harnesses

Project definitions and measurement infrastructure remain under
`scripts/bench/` and `scripts/perf/`. Use their checked-in metadata validators
before changing rows or fixtures:

```bash
node scripts/bench/validate-project-metadata.mjs
node scripts/bench/test-project-rows.mjs
```

Performance is meaningful only for an oracle-green row with its real dependency
graph. Build once, record the binary hash, and time an immutable optimized
binary. Keep CPU, wall time, peak RSS, diagnostics fingerprint, fixture hash,
and oracle version together. Wrap heavy measurements:

```bash
scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh \
  --quick --filter '<row>' --json-file /tmp/tsz-bench.json
```

Do not call a red, yellow, gray, stubbed, or unsupported row a speed win.

## Tracing

Compiler internals use tracing:

```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree \
  cargo run -p tsz-cli --bin tsz -- path/to/file.ts
```

Do not add temporary print or `dbg!` instrumentation. Prefer source reading and
focused traces; remove task-specific trace configuration from committed tests.

## Memory-Guarded Commands

Wrap long or memory-heavy work with the repository guard:

```bash
scripts/safe-run.sh --limit 8192 -- cargo build --profile dist -p tsz-cli
```

The guard reports and terminates excessive RSS. Do not run broad recursive disk
scans or discard caches without first identifying ownership.

## Documentation And Context Checks

After changing `.codex/`, `.claude/`, `AGENTS.md`, repo skills, or startup
hooks, run:

```bash
python3 scripts/agents/llm-context-audit.py
```

Use `rg` and `rg --files` for repository navigation. The retired generated
repository inventory is intentionally gone; current architecture is documented
in `docs/architecture/RESET.md`.

## Result Language

Keep three classes of evidence distinct in output, issues, and PR bodies:

1. frozen legacy checkpoint;
2. exact rewrite capability result;
3. broad rewrite corpus observation.

Record exact commands and failures. Never turn an observation green by
filtering unsupported cases or lowering a historical metric.
