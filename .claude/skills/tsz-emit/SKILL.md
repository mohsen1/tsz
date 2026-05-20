---
name: tsz-emit
description: Write better tsz JavaScript and declaration emit code. Use when changing crates/tsz-emitter, emit transforms, DTS output, source maps, helper scheduling, temp/hoist planning, target/module output, or emit parity tests.
---

# TSZ Emit Engineering Skill

Use this skill for any `tsz-emitter` or DTS change. The goal is exact `tsc`
parity with cleaner emit architecture, not isolated baseline pokes.

## First Checks

1. Read `AGENTS.md` and `docs/plan/ROADMAP.md` before starting durable emit,
   DTS, target, module, or architecture work.
2. Inspect open PRs/issues for overlapping emit work before coding.
3. Identify the failure family: target gate, module wrapper/export schedule,
   helper requirement, temp/hoist planning, region transform, direct printer
   layout, DTS nameability/portability, or parser recovery.
4. State the rule structurally: "When this syntax/output context appears,
   `tsc` emits X; tsz should emit X through this owner."

## Architecture Rules

- Emit is `OUTPUT`. Do not add checker/solver semantic validation to emitter
  code.
- Prefer planned facts over discovery while writing text: target facts, helper
  needs, temp reservations, hoists, prologues, export bindings, and disposable
  regions should be known before or at the owner boundary.
- Keep direct-to-target emit. Do not introduce an `ESNext -> ... -> ES5`
  transform pipeline.
- Ban output surgery for semantics. Do not use `replace`, `replacen`, or
  `replace_range` to patch already-emitted JS/DTS for exports, wrappers,
  declarations, decorators, classes, or resource management.
- String cleanup is fine only when the string is data: escaping, source-map
  paths, numeric separator cleanup, and path normalization.
- DTS has the same bar: prefer declaration summaries and structured declaration
  emission over late string rewrites.

## Printer Hygiene

- Use structured writer helpers when available: `open_paren`, `close_paren`,
  `open_brace`, `close_brace`, token/source-map helpers, list emitters, and
  target/module helper APIs.
- Add new delimiter helpers when they prevent real classes of mistakes, such as
  unbalanced delimiters, source-map drift, or inconsistent spacing.
- Keep handwritten `write("(")`, `write(")")`, `write("{")`, and `write("}")`
  only when the local context needs exact custom formatting and no helper fits.
- Do not encode target or module policy in random printer branches. Route it
  through target/module facts or an emit-plan-like owner boundary.

## Temp And Hoist Planning

- Every function-like body needs an explicit temp scope and function-body
  hoist insertion strategy.
- Temps created while emitting body expressions must not leak to outer scopes or
  disappear when the function scope is popped.
- If a body path bypasses normal `emit_block`, it must still consume/reset the
  function-body flag and insert hoisted temps at the correct body point.
- Prefer preallocation/reservation when temp names affect earlier output. Late
  insertion is acceptable only when the insertion point is explicitly recorded
  before body emission.
- Check adjacent bodies: arrows, function declarations, function expressions,
  methods, accessors, constructors, async/generator wrappers, static blocks,
  namespace/module wrappers, and parameter prologue bodies.

## Target And Module Discipline

- Treat ES2015+ as the strategic primary lane. ES3/ES5 compatibility can exist,
  but keep legacy behavior explicit and quarantined.
- Do not let CommonJS export scheduling affect System/ES module output.
- Module/export folding should be structured schedule entries, not text patches.
- Resource-management (`using`/`await using`) belongs to region planning, not
  localized after-the-fact wrapping.

## Testing

- Do not run full emit, conformance, or fourslash locally. Let ready CI do that.
- Use targeted emit filters:
  ```bash
  scripts/emit/run.sh --filter=<family> --js-only --verbose --json-out=/tmp/<name>.json
  scripts/emit/run.sh --filter=<family> --dts-only --verbose --json-out=/tmp/<name>.json
  ```
- Use focused unit tests:
  ```bash
  cargo nextest run -p tsz-emitter <test-name-or-family>
  ```
- For every non-trivial emit fix, test the reported case plus adjacent shapes:
  block vs concise arrow, function declaration vs expression, method/accessor
  if relevant, ES5 vs primary ES2015+ target, and module mode if exports move.
- Keep PRs focused. Existing ready emit PRs should land before opening new
  overlapping emit PRs.

## PR Notes

Every emit PR body should include:

- AgentName.
- Failure family and structural rule.
- Owner layer: direct printer, lowering directive, IR/plan operation,
  declaration summary, target facts, or module/export schedule.
- Why the change avoids semantic validation and output surgery.
- Targeted verification commands and any known unsupported shapes.
