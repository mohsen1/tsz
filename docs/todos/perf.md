# Performance Follow-Ups

- `profiling/tooling (samply on .target-bench/dist/tsz)`: call stacks were unsymbolicated addresses only; needs build/profile configuration work (debug symbols + symbolization path) before flamegraph-level analysis is reliable.
- `crates/tsz-checker/src/state/state_type_environment.rs::build_type_environment`: still spends significant time in upfront symbol/type environment population; deeper redesign to defer more `get_type_of_symbol` work is architectural and not a small targeted optimization.
