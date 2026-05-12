# Claim: System legacy decorator helper placement assertion

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5717 follow-up (missed-review audit)

## Target

Strengthen System legacy-decorator regression coverage so helper placement is validated structurally, not only by helper presence.

## Plan

1. Keep the existing `__decorate` presence assertion.
2. Assert relative ordering: `System.register(...)` -> `"use strict";` -> `var __decorate ...`.

## Result

- Updated test:
  - `system_exported_legacy_decorated_class_exports_decorator_assignment`
- File:
  - `crates/tsz-emitter/src/emitter/module_wrapper/tests/system_emit.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-emitter system_exported_legacy_decorated_class_exports_decorator_assignment -- --nocapture
```
