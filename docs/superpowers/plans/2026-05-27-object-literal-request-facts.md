# Object Literal Request Facts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split object-literal request/facts collection out of property typing for #9436 without behavior changes.
**Architecture:** Keep semantics in existing query helpers and solver APIs. The checker helper only packages current orchestration state and preserves current contextual side effects.
**Tech Stack:** Rust checker code in `crates/tsz-checker`, local docs in `docs/superpowers`, GitHub coordination through `gh`.

---

## Task 1: Capture Current Request Phase

- [ ] In `crates/tsz-checker/src/types/computation/object_literal/computation.rs`, identify the contiguous request/facts setup before the property loop.
- [ ] Confirm the extracted phase includes nullish contextual stripping, contextual target tracking, context-sensitive element detection, getter-name pre-scan, contextual union narrowing, marker `this` handling, contextual receiver `this`, base request creation, and partial initializer tracking.
- [ ] Leave `obj_all_method_names` and circular-return site collection in the top-level function unless moving them is required for borrow-checking; they support property typing, not the request boundary.

## Task 2: Add `ObjectLiteralRequestFacts`

- [ ] Add a private `ObjectLiteralRequestFacts` struct near the top of `computation.rs`.
- [ ] Include fields for `contextual_type`, `original_contextual_type`, `all_properties_context_sensitive`, `obj_getter_names`, `marker_this_type`, `contextual_receiver_this_type`, `base_request`, and `partial_initializer_stack_index`.
- [ ] Use existing types only: `TypeId`, `TypingRequest`, `NodeIndex`, and `rustc_hash::FxHashSet<String>`.

## Task 3: Extract the Facts Collector

- [ ] Add a private `collect_object_literal_request_facts` method on `CheckerState<'a>`.
- [ ] Inputs: object literal node index, original `TypingRequest`, and the object element slice.
- [ ] Move the existing request/facts setup into the method with no policy changes.
- [ ] Return `ObjectLiteralRequestFacts`.
- [ ] Keep the `tracing::trace!` message and payload unchanged.

## Task 4: Rewire the Main Function

- [ ] Replace the inlined setup block in `get_type_of_object_literal_with_request` with a call to `collect_object_literal_request_facts`.
- [ ] Destructure the returned facts into the same local names used by the existing property loop.
- [ ] Keep property assignment, method, accessor, spread, finalization, and cleanup logic unchanged.
- [ ] Verify the file stays below the 2000-line hard limit.

## Task 5: Verify Locally

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo nextest run -p tsz-checker object_literal`.
- [ ] Run `scripts/arch/check-checker-boundaries.sh`.
- [ ] If a verification fails, fix the smallest cause and rerun the failed focused command.

## Task 6: Coordinate and Publish

- [ ] Commit the design and plan docs.
- [ ] Commit the implementation separately.
- [ ] Push `codex/m4c-9436-object-literal-requests-20260527`.
- [ ] Open a draft PR for #9436 with AgentName `M4-C`, Track, Invariant, Scope, Project Corpus Impact, Verification, and Coordination Notes.
- [ ] Apply `agent:M4-C`.
- [ ] Verify the remote PR body contains all required sections with `gh pr view <number> --json body`.
