# 2026-05-13 — `compute_type_of_symbol` Interface Simple-Local-Object Fast Path (monorepo-006)

Follow-up to `2026-05-13-compute-type-of-symbol-interface-callsite-outcomes.md`.

Goal: reduce root interface lowering cost by short-circuiting trivial local
interface declarations into direct object-shape construction.

## Change

In the `compute_type_of_symbol` interface branch, add a narrow early path for
single-declaration local interfaces that are safe to lower directly:

1. local declaration only (no out-of-arena / cross-file same-index collisions),
2. no `extends`, no computed property names, no type parameters,
3. all members are `PROPERTY_SIGNATURE` with resolvable property names,
4. the member list is non-empty,
5. annotated member types are primitive keyword type nodes.

When eligible, build `PropertyInfo` rows directly and return
`object_with_symbol(..., Some(sym_id))` without entering the heavier interface
lowering path.

## Safety correction

The original branch admitted empty interfaces and member annotations that need
the normal hybrid type-lowering resolvers. That regressed JSX, cross-module
optional-property, and conditional-infer tests by turning unresolved aliases or
empty object shapes into overly-permissive types. The replayed branch now falls
back to the full interface lowering path for empty interfaces, type references,
unions, conditional types, type literals, arrays, and other non-primitive member
annotations.

The measurement below is the original broad shortcut run and is kept only as
historical evidence for why the shortcut was investigated. It must not be quoted
as the current guarded fast-path result until the guarded branch is remeasured.

Guarded rerun now recorded at:
`2026-05-13-compute-type-of-symbol-interface-simple-local-object-guarded-rerun.md`.

## Reproducer

| Item | Value |
| --- | --- |
| Raw artifact | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-simple-local-object-fastpath-monorepo-006.json` |
| Baseline raw | `docs/plan/perf-runs/raw/2026-05-13-compute-type-of-symbol-interface-callsite-outcomes-monorepo-006.json` |
| `tsz` build | `cargo build -p tsz-cli --bin tsz --release` |
| Fixture | `scripts/bench/scale-cliff/fixtures/monorepo-006` |
| Counter mode | `TSZ_PERF_COUNTERS=1` |
| Command | `.target/release/tsz --noEmit -p <fixture>/tsconfig.json --extendedDiagnostics --pretty false` |

## Result

Timing deltas vs the callsite-outcomes baseline:

- total: `95.75s -> 84.24s` (`-11.51s`, `-12.02%`)
- check: `94.06s -> 82.46s` (`-11.60s`, `-12.33%`)
- parse+bind: `1.16s -> 1.35s` (`+0.19s`)
- I/O read: `0.33s -> 0.26s` (`-0.07s`)

Correctness / bucket stability:

- diagnostics unchanged: `10,198`
- `compute_type_of_symbol.total_calls` unchanged: `26,379`
- `compute_type_of_symbol.cache_hits` unchanged: `252,043`
- `compute_type_of_symbol.kind.interface` unchanged: `24,796`
- interface call-site split unchanged: `root=24,782`, `parent_interface=14`

Expected branch-shape shift from the new early-return path:

- `interface_fastpath.skip_all_three`: `24,767 -> 7`
- `interface_fastpath.skip_computed_name_map_and_local_heritage_merge`: `16` (unchanged)
- `interface_fastpath.skip_computed_name_map`: `1` (unchanged)
- `interface_fastpath.full_path`: `1` (unchanged)

## Decision

1. Keep only the guarded simple-local-object shortcut; correctness-sensitive
   member annotations must stay on the full interface lowering path.
2. Keep future interface work focused on safe root-call shortcutting and cheap
   direct-lowering cases, not additional tuning of the older interface fast-path
   gate matrix.
3. On current guarded mainline behavior for monorepo-006, the shortcut is
   inactive (`simple-object hits=0`, `success=0`), so the next step is either
   conformance-proven guard relaxation or dead-path cleanup.
