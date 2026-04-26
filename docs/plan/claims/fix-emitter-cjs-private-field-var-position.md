# fix(emitter): position CJS hoisted assignment temps before __esModule preamble

- **Date**: 2026-04-26
- **Branch**: `fix/emitter-cjs-private-field-var-position`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS Emit pass rate)

## Intent

In CommonJS module mode, tsc emits hoisted temp `var` declarations (e.g. `var _Foo_field;` for class private field WeakMaps, `var _a;` for assignment destructuring) BEFORE the `Object.defineProperty(exports, "__esModule", { value: true })` preamble and any `exports.X = void 0;` initializations. tsz currently inserts these after the preamble for the `hoisted_assignment_temps` channel (used by class private field lowering), causing diff failures such as `importHelpersNoHelpersForPrivateFields`. This PR routes `hoisted_assignment_temps` and `hoisted_for_of_temps` through the existing pre-preamble insertion offset (`cjs_destr_hoist_byte_offset`) when CommonJS preamble was emitted, matching tsc layout.

## Files Touched

- `crates/tsz-emitter/src/emitter/source_file/emit.rs` (~10 LOC change to the hoisted-vars insertion block)
- New regression unit test under `crates/tsz-emitter/tests` covering the private-field + CJS scenario.

## Verification

- `cargo nextest run -p tsz-emitter`
- `./scripts/emit/run.sh --filter importHelpersNoHelpersForPrivateFields`
- Targeted no-regression: `./scripts/emit/run.sh --filter privateName` and `./scripts/emit/run.sh --filter exportEmptyArrayBindingPattern`
