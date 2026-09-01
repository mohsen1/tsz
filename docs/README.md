# TSZ Documentation

TSZ is in a clean-slate rewrite. The documentation here describes the new
compiler only. Git history is the archive for the retired implementation.

## Start Here

- [`plan/ROADMAP.md`](plan/ROADMAP.md) is the execution plan, compatibility
  target, milestone sequence, and R0 conviction gate.
- [`architecture/RESET.md`](architecture/RESET.md) defines the replacement
  architecture, semantic invariants, and public API boundary.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) explains how to prepare and submit a
  change.
- [`DEVELOPMENT.md`](DEVELOPMENT.md) covers setup, workspace layout, builds,
  and validation.
- [`HOW_TO_CODE.md`](HOW_TO_CODE.md) is the implementation checklist.
- [`development/TOOLING.md`](development/TOOLING.md) is the command reference.

## Compatibility Specifications

[`specs/`](specs/) contains behavior-facing notes for diagnostics, TypeScript
directives, library loading, root-file order, and `try-tsz`. The pinned
TypeScript 7.0.2 checkout and oracle output remain authoritative when a note and
the oracle disagree.

## Validation And Product Material

- [`site/`](site/) contains website-facing compatibility and benchmark prose.
- `scripts/conformance/`, `scripts/emit/`, and `scripts/fourslash/` exercise the
  retained black-box compatibility perimeter.
- `scripts/bench/` and `scripts/perf/` retain project and performance evidence.
- Legacy tests are porting specifications, not APIs for the replacement
  compiler.

The removed architecture guides and generated repository inventory described
the retired multi-crate compiler. Do not regenerate or copy them into the new
source tree.
