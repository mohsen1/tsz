# Cross-File Circular Class Inheritance Detection

## Status: Not implemented

## Problem
When classes form circular inheritance across separate files (`class C extends E` in file1, `class D extends C` in file2, `class E extends D` in file3), the cycle is NOT detected and TS2506 errors are not emitted. Each file's checker has its own `InheritanceGraph` that only sees its own file's edges.

## Expected Behavior (tsc)
All classes in the cycle should emit TS2506 "X is referenced directly or indirectly in its own base expression."

## Current Behavior
- Single-file circular inheritance: correctly detected via `class_inheritance.rs`
- Cross-file circular inheritance: no cycle detected, no TS2506 emitted
- The classes silently get incorrect types

## Root Cause
- `class_inheritance.rs` uses `InheritanceGraph` which is per-`CheckerContext`
- In parallel mode, each file gets its own `CheckerState` with its own `InheritanceGraph`
- Each graph only knows about the current file's `extends` edge, not edges from other files
- The `class_instance_resolution_set` IS copied between delegated checkers, which prevents infinite recursion, but doesn't emit the diagnostic

## Fix Approach
1. Move `InheritanceGraph` to a shared structure (e.g., `Arc<Mutex<InheritanceGraph>>`) that all parallel checkers write to
2. Or perform a pre-check pass before parallel checking that builds the full inheritance graph from all files' heritage clauses
3. Then use this shared graph in `check_class_inheritance_cycle` for each file

## Files
- `crates/tsz-checker/src/class_inheritance.rs` — `check_class_inheritance_cycle`
- `crates/tsz-solver/src/inheritance.rs` — `InheritanceGraph`
- `crates/tsz-cli/src/driver.rs` — parallel checking setup
