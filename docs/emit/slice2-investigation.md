# Slice 2 Emit Investigation - Feb 12, 2026

## Current State

**Pass Rate**: 66.2% (290/438 tests passing)
**Baseline**: ~62% (as of previous reports)
**Improvement**: +4.2 percentage points

## Key Finding: Test Cache Issues

The emit test runner (`scripts/emit/run.sh`) uses a cache at `.cache/emit-cache.json`. Stale cache can show incorrect failures. **Always clear cache before accurate testing**:

```bash
rm -rf scripts/emit/.cache/* && ./scripts/emit/run.sh --max=500 --js-only
```

## Main Slice 2 Issue: Nested Namespace Indentation

### Problem Description

When an exported namespace follows an exported class with the same name inside a parent namespace, the namespace IIFE gets **8 spaces** of indentation instead of **4 spaces**.

### Example

**Input TypeScript**:
```typescript
namespace A {
    export class Point {
        static Origin: Point = { x: 0, y: 0 };
    }

    export namespace Point {
        var Origin = "";
    }
}
```

**Expected Output** (TSC):
```javascript
    A.Point = Point;
    (function (Point) {  // ← 4 spaces
        var Origin = "";
    })(Point = A.Point || (A.Point = {}));
```

**Actual Output** (TSZ):
```javascript
    A.Point = Point;
        (function (Point) {  // ← 8 spaces (incorrect!)
        var Origin = "";
    })(Point = A.Point || (A.Point = {}));
```

### Root Cause

This is a **known issue** documented in commit `b8eaac278` (2026-02-12):

> "Remaining issue: Nested namespaces that follow class exports within a parent namespace still get extra indentation. This appears to be an issue in how the NamespaceES5 IR transform handles the combination of class + nested namespace."

The problem is in the **IR layer** (Intermediate Representation):

1. `NamespaceES5Emitter` is told to use `indent_level=0` at line 566 of `declarations.rs`
2. The `IRPrinter` is supposed to generate output with only relative indentation
3. However, the IR transform produces output with unexpected leading whitespace for the first line
4. This happens specifically when a namespace follows a class export in the same scope

### Affected Tests

- `ClassAndModuleThatMergeWithStaticVariableAndExportedVarThatShareAName`
- `ClassAndModuleThatMergeWithStaticVariableAndNonExportedVarThatShareAName`
- `ClassAndModuleThatMergeWithStaticFunctionAndExportedFunctionThatShareAName`
- `ClassAndModuleThatMergeWithStaticFunctionAndNonExportedFunctionThatShareAName`

### Code Location

**Primary files**:
- `crates/tsz-emitter/src/emitter/declarations.rs` (lines 558-589) - where namespace ES5 transform is called
- `crates/tsz-emitter/src/transforms/namespace_es5.rs` - the ES5 namespace transformer
- `crates/tsz-emitter/src/transforms/ir_printer.rs` - the IR-to-JavaScript printer

**Key code** (`declarations.rs:566`):
```rust
// Set IRPrinter indent to 0 because we'll handle base indentation through
// the writer when writing each line. This prevents double-indentation for
// nested namespaces where the writer is already indented.
es5_emitter.set_indent_level(0);
```

### Investigation Attempts

1. **Tried**: Stripping leading whitespace from the first line of namespace output
   - Result: Didn't work - the extra indentation wasn't in the string, it was from the write position

2. **Tried**: Adding debug output to understand indent levels and line state
   - Result: Debug output never appeared (likely optimized out in release builds)

3. **Root cause identified**: The issue is in how `NamespaceES5Emitter` or `IRPrinter` generates the IR nodes for namespaces that follow class declarations. The IR structure itself contains the extra indentation.

### Fix Strategy

To fix this issue properly, someone needs to:

1. Investigate `NamespaceES5Emitter::emit_namespace()` to understand how it builds IR nodes
2. Check `IRPrinter` to see why it adds base indentation even with `indent_level=0`
3. Determine why the combination of "class export" + "namespace with same name" triggers this behavior
4. Likely need to modify the IR node structure or the way `IRPrinter` handles the first line

This requires deep understanding of the IR transform layer, which is beyond quick fixes.

## Other Slice 2 Issues

The session notes mentioned:
- **Object literals keeping short properties on same line** - not seen in test failures
- **Short function bodies staying on one line** - not seen in test failures

These may have already been fixed or don't occur in the first 500 tests.

## Non-Slice 2 Failures in Test Set

- **Slice 1** (Comments): `APISample_*` tests with comment preservation issues
- **Slice 3** (Destructuring): `ES5For-of*` tests with destructuring/variable renaming issues
- **Slice 4** (Helpers): Tests needing `__values`, `__read`, `__spread`, `_this` capture

## Recommendations

1. **For future Slice 2 work**: Focus on the nested namespace indentation bug - it's the main formatting issue blocking progress
2. **Approach**: Start by adding instrumentation to `NamespaceES5Emitter` to understand IR node generation
3. **Don't**: Try to fix it in `declarations.rs` - the problem is earlier in the pipeline
4. **Testing**: Always clear the cache before testing to get accurate results

## Test Commands

```bash
# Clear cache and run 500 tests
rm -rf scripts/emit/.cache/* && ./scripts/emit/run.sh --max=500 --js-only

# Test specific failing case
./scripts/emit/run.sh --js-only --verbose --filter="ClassAndModuleThatMergeWithStaticVariableAndExportedVarThatShareAName"

# Run unit tests
cargo nextest run --release
```

## Files to Review for Fixes

1. `crates/tsz-emitter/src/transforms/namespace_es5.rs` - Entry point for ES5 namespace transformation
2. `crates/tsz-emitter/src/transforms/namespace_es5_ir.rs` - IR node builder
3. `crates/tsz-emitter/src/transforms/ir_printer.rs` - IR to JavaScript converter
4. `crates/tsz-emitter/src/emitter/declarations.rs:558-589` - Where namespace emit is triggered
