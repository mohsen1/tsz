# Session TSZ-2: Control Flow Test Fixes

**Started**: 2026-02-05
**Status**: 🐞 DEBUGGING

## Goal

Fix the 3 failing control flow tests to ensure `instanceof` narrowing works correctly.

## Context

- CI is now green (formatting ✅, clippy ✅)
- 3 failing tests remain in `control_flow` module
- Flow narrowing infrastructure IS already wired up (discovered in TSZ-11)

## Failing Tests

To be identified by running `cargo nextest run`

## Next Steps

1. Run `cargo nextest run` to identify specific failing tests
2. Use `tsz-tracing` to debug first failure (NOT eprintln!)
3. Ask Gemini Question 1: Approach validation before implementing
4. Implement fix based on Gemini's guidance
5. Ask Gemini Question 2: Implementation review
6. Move to next test

## References

- FlowAnalyzer: src/checker/flow_analysis.rs
- ControlFlow: src/checker/control_flow.rs
- apply_flow_narrowing: src/checker/flow_analysis.rs:1320
