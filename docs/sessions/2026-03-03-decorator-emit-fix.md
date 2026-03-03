# Session: Decorator Emit Fix (2026-03-03)

## Changes Made

### Fix: ES5 Class Decorator IIFE Placement + ES2015+CommonJS Decorator Emit

**Commit**: `fix(emitter): emit __decorate calls for ES5 class decorator IIFE and ES2015+CommonJS`

**Files changed**:
- `crates/tsz-emitter/src/transforms/class_es5_ir.rs` - Added `emit_class_decorator_ir()` and `emit_member_decorator_ir()` for IIFE-internal decorator emission
- `crates/tsz-emitter/src/transforms/class_es5.rs` - Added `ClassDecoratorInfo` struct
- `crates/tsz-emitter/src/transforms/mod.rs` - Re-export `ClassDecoratorInfo`
- `crates/tsz-emitter/src/emitter/mod.rs` - Import `ClassDecoratorInfo`
- `crates/tsz-emitter/src/emitter/transform_dispatch.rs` - `create_es5_class_emitter_with_decorators()` helper
- `crates/tsz-emitter/src/emitter/declarations/class.rs` - ES5 decorator branch uses ES5 emitter with decorator info
- `crates/tsz-emitter/src/emitter/module_emission/exports.rs` - Fixed ES2015+CommonJS decorator `__decorate` call

**Root cause of ES2015+CommonJS issue**:
- `@dec export class C {}` is parsed as `EXPORT_DECLARATION` (kind=279) wrapping `CLASS_DECLARATION` (kind=264)
- The lowering pass does NOT create a `CommonJSExport` transform for this node in ES2015
- Instead, `emit_node` dispatches to `emit_export_declaration` → `emit_export_declaration_commonjs`
- In `exports.rs`, the CLASS_DECLARATION match arm (line 273) handles the class properly
- But the decorator code path (lines 372-389) was emitting the class and `exports.C = C;` without the `__decorate` call
- Fixed by adding `emit_legacy_class_decorator_assignment` after the export assignment

## Test Results

- **Conformance**: 9,867/12,570 (78.5%)
- **JS Emit**: 10,303/13,427 (76.7%) - up from 10,222/13,546 (75.5%)
- **Decorator tests**: 123/139 (88.5%)
- **decoratorOnClass tests**: 60/67 (89.6%)
- **Driver tests**: 317 passed, 0 failed

## Emit Test Analysis

Top failure categories for remaining 3,124 JS emit failures:
1. Missing helper functions (893 tests): `__esDecorate` (217), `__runInitializers` (215), `__addDisposableResource` (206), `__decorate` (135), `__classPrivateFieldGet` (92), `__importStar` (77), `__awaiter` (69), `__rest` (67), `__generator` (58), `__metadata` (56)
2. Module/CJS transform issues (726 tests)
3. JSX not transformed (142 tests)
4. Comment-only differences (118 tests)
5. Import not erased (49 tests)
6. Parenthesization differences (28 tests)
7. Semicolon-only differences (20 tests)
8. Const enum inlining edge cases (13 tests)

## Key Architectural Insight

The AST representation of `export class C {}` uses `EXPORT_DECLARATION` as the outer node, with `CLASS_DECLARATION` as the inner `export_clause`. The emitter has TWO paths for handling this:

1. **Transform path** (via lowering): `CommonJSExport` directive attached to EXPORT_DECLARATION → `apply_transform` → specific handlers in `transform_dispatch.rs`
2. **Default path** (no transform): `emit_node` → `emit_export_declaration` → `emit_export_declaration_commonjs` → handlers in `exports.rs`

For ES5+CommonJS, path 1 is used (Chain([ES5Class, CommonJSExport])).
For ES2015+CommonJS, path 2 is used (no transform on EXPORT_DECLARATION).

Both paths now correctly handle decorator emission.
