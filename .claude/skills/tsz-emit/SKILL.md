---
name: tsz-emit
description: Write better tsz JavaScript and declaration emit code. Use when changing crates/tsz-emitter, emit transforms, DTS output, source maps, helper scheduling, temp/hoist planning, target/module output, or emit parity tests.
---

# TSZ Emit

Use for JS/DTS emit, transforms, helpers, temp/hoist planning, target/module
output, source maps, and emit parity tests.

## Start

1. Read `AGENTS.md` and `docs/plan/ROADMAP.md` for durable emit/DTS work.
2. Inspect overlapping emit PRs/issues.
3. Name the failure family: target gate, module/export schedule, helper, temp/
   hoist, region transform, printer layout, DTS nameability/portability,
   source maps, or parser recovery.
4. State: `When <syntax/output context>, tsc emits X; tsz emits X through
   <owner>.`

## Architecture

- Emit is `OUTPUT`: no checker/solver semantic validation in emitter code.
- Prefer planned facts before writing text: target/module facts, helper needs,
  temp reservations, hoists, prologues, exports, resource/disposable regions.
- Keep direct-to-target emit; do not introduce an `ESNext -> ... -> ES5`
  pipeline.
- No semantic output surgery with `replace`, `replacen`, or `replace_range`.
  Data cleanup is fine for escaping, source-map paths, numeric separators, and
  path normalization.
- DTS should use declaration summaries/structured emission, not late string
  rewrites.

## Hot Checks

- Use writer/list/token/source-map helpers; add delimiter helpers when they
  prevent real mistakes.
- Target/module policy belongs in target/module facts or emit-plan-like owners,
  not random printer branches.
- Every function-like body needs temp scope and hoist strategy. Bypasses of
  `emit_block` must still handle function-body flags and insertion points.
- Check adjacent shapes: arrows, declarations, expressions, methods, accessors,
  constructors, async/generator wrappers, static blocks, namespaces/modules,
  parameter prologues, ES2015+ and relevant legacy targets.

## Verification

No full emit/conformance/fourslash locally.

```bash
scripts/emit/run.sh --filter=<family> --js-only --verbose --json-out=/tmp/<name>.json
scripts/emit/run.sh --filter=<family> --dts-only --verbose --json-out=/tmp/<name>.json
cargo nextest run -p tsz-emitter <test-or-family>
```

PR body: `Goal: hold`, failure family, structural rule, owner layer, why no
semantic validation/output surgery, targeted verification, unsupported shapes.
