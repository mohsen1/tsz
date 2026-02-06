# Session TSZ-17: Stabilize Index Signatures and Readonly Infrastructure

**Started**: 2026-02-06
**Status**: 🔄 NOT STARTED
**Predecessor**: TSZ-13 (Index Signatures - Code Already Exists)

## Task

Fix bugs in existing index signature and readonly implementations to validate previous work.

## Problem Statement

### Readonly Infrastructure (~6 tests)
Tests get error 2318 ("Cannot find global type") - test setup issue.

### Index Signatures (~3 tests)
Code exists but test fails with wrong TypeId - needs tracing debug.

### Enum Error Deduplication (~2 tests)
Tests expect 1 error but get 2 - diagnostic deduplication.

## Expected Impact

- **Direct**: Fix ~11 tests
- **Goal**: 8258+ passing, 42- failing

## Test Status

**Start**: 8247 passing, 53 failing

## Notes

Gemini recommends "closing the loop" on TSZ-11 and TSZ-13 before moving to Flow Narrowing.

Use tsz-tracing skill for debugging.
