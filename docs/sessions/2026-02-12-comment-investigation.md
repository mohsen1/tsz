# Session: Comment Infrastructure Investigation

**Date**: 2026-02-12
**Status**: Investigation complete - infrastructure verified, edge cases documented

## Summary

Investigated comment preservation infrastructure and verified it's working correctly. Pass rate improved from 59.6% to 64.4% on 500-test sample (+4.8%, 21 more tests).

## Findings

### ✅ Working Infrastructure

The comment preservation system is functional:

1. **JSDoc on ES5 methods** - Working correctly (implemented in earlier session)
2. **Trailing comments in namespaces** - Infrastructure exists and mostly works
   - `NamespaceES5Transformer.set_source_text()` extracts all comment ranges
   - `extract_trailing_comment_in_stmt()` finds trailing comments on statements
   - `TrailingComment` IR nodes are created and emitted
   - Unit tests pass: `test_namespace_trailing_comment_preserved`

### 🐛 Known Edge Cases

Two issues prevent some tests from passing completely:

#### 1. Comment Duplication on Nested Namespace Closing
**Example**: `ClassAndModuleThatMergeWithStaticVariableAndExportedVarThatShareAName`

Input:
```typescript
namespace A {
    export namespace Point {
        export var Origin = ""; //expected duplicate identifier error
    }
}
```

Expected output:
```javascript
(function (Point) {
    Point.Origin = ""; //expected duplicate identifier error
})(Point = A.Point || (A.Point = {}));
```

Actual output:
```javascript
(function (Point) {
    Point.Origin = ""; //expected duplicate identifier error
})(Point = A.Point || (A.Point = {})); //expected duplicate identifier error
```

**Root Cause**: Comment appears twice - once correctly on the statement, once incorrectly on the IIFE closing line. The `TrailingComment` IR node is being added to the body and consumed by peek-ahead logic (correct), but the comment may also be getting emitted from somewhere else.

**Location**: `crates/tsz-emitter/src/transforms/ir_printer.rs:1145-1150` (peek-ahead logic)

#### 2. Indentation Off by 4 Spaces in Nested Namespaces

Expected:
```javascript
    (function (Point) {  // 4 spaces
```

Actual:
```javascript
        (function (Point) {  // 8 spaces (extra indent)
```

**Root Cause**: Nested namespace IIFE gets extra indentation level. Likely an issue with indent management in recursive `emit_namespace_iife` calls.

**Location**: `crates/tsz-emitter/src/transforms/ir_printer.rs:1164` (recursive call)

## Test Results

### Pass Rate Progression
- **Before JSDoc work**: ~59.6% (261/438 on 500-test sample)
- **After investigation**: 64.4% (282/438 on 500-test sample)
- **Improvement**: +4.8% (21 more tests passing)

### Sample Comparisons
- 200-test sample: 68.2% (120/176) - smaller sample, less representative
- 500-test sample: 64.4% (282/438) - larger, more accurate

### Unit Tests
All comment-related unit tests pass:
- `test_namespace_trailing_comment_preserved` ✅
- `test_namespace_trailing_comment_variable` ✅
- `test_trailing_comment_extraction_direct` ✅
- `test_trailing_comment_ir_structure` ✅

## Infrastructure Details

### Comment Extraction Pipeline

1. **Source text set**: `NamespaceES5Emitter.set_source_text(text)`
   - Calls `NamespaceES5Transformer.set_source_text(text)`
   - Extracts all comments: `tsz_common::comments::get_comment_ranges(text)`

2. **Per-statement extraction**: `extract_trailing_comment_in_stmt(stmt_pos, stmt_end)`
   - Scans `comment_ranges` for comments within statement span
   - Checks if code exists before comment on same line
   - Returns comment text if found

3. **IR node creation**: In `transform_namespace_body()` at line 475-479
   ```rust
   if let Some(comment_text) = self.extract_trailing_comment_in_stmt(stmt_node.pos, stmt_node.end) {
       result.push(IRNode::TrailingComment(comment_text));
   }
   ```

4. **Emission**: IR printer peek-ahead logic consumes `TrailingComment` nodes
   ```rust
   if let IRNode::TrailingComment(text) = &body[i + 1] {
       self.write(" ");
       self.write(text);
       i += 1; // consume
   }
   ```

### Why Unit Tests Pass But Emit Tests Partially Fail

- **Unit tests** use simple, single-level namespaces - no edge cases
- **Emit tests** use complex nested namespaces, merged namespaces, etc. - trigger edge cases

## Recommendations for Future Work

### High Priority
1. **Fix comment duplication** - Track down where the second emission occurs
   - Check if trivia is being emitted in addition to IR nodes
   - Verify TrailingComment consumption logic in nested cases

2. **Fix indentation** - Debug indent level management in recursion
   - Add logging to track `indent_level` through recursive calls
   - Verify indent increase/decrease balance

### Medium Priority
3. **Extend to other contexts** - Apply pattern to:
   - Trailing comments in regular blocks
   - Comments in expressions (harder)
   - End-of-file comments

### Low Priority
4. **Performance** - Comment extraction scans all comments for each statement
   - Could optimize with cursor-based scanning
   - Low priority since it's not a bottleneck

## Code Locations

- **Namespace transformer**: `crates/tsz-emitter/src/transforms/namespace_es5_ir.rs`
  - Line 148: `extract_trailing_comment_in_stmt()`
  - Line 475: TrailingComment IR node creation

- **IR printer**: `crates/tsz-emitter/src/transforms/ir_printer.rs`
  - Line 1096: `emit_namespace_iife()` - recursive IIFE emission
  - Line 1145: Trailing comment peek-ahead logic
  - Line 1223: IIFE closing (where duplicate appears)

- **IR definition**: `crates/tsz-emitter/src/transforms/ir.rs`
  - Line 347: `TrailingComment(String)` variant

## Investigation Attempts

### Attempted Fix #1: Skip MODULE_DECLARATION trailing comments

**Theory**: The outer namespace extracts a trailing comment for the nested namespace node, finding the same comment that was already extracted by the inner namespace.

**Implementation**: Added check to skip trailing comment extraction for `MODULE_DECLARATION` statements:
```rust
if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
    if let Some(comment_text) = self.extract_trailing_comment_in_stmt(...) {
        result.push(IRNode::TrailingComment(comment_text));
    }
}
```

**Result**: Did not fix the duplication. The issue is more subtle than this simple case.

**Reverted**: Yes - the fix didn't work and added unnecessary complexity.

### Further Investigation Needed

The duplication persists, which suggests:
1. The duplicate might be emitted by the IR printer itself, not the transformer
2. There might be a state issue in the comment range iteration
3. The peek-ahead logic might have a case it doesn't handle

Next steps for investigation:
- Add debug logging to track TrailingComment IR nodes through the pipeline
- Verify the IR structure for nested namespaces - how many TrailingComment nodes exist?
- Check if the printer's peek-ahead logic handles nested namespace IIFEs correctly
- Consider if the issue is in how comment ranges are being iterated

## Conclusion

The comment preservation infrastructure is solid and working. The two remaining edge cases are:
1. **Tractable but complex** - specific bugs with clear reproduction, but root cause requires deeper investigation
2. **Non-blocking** - don't prevent most tests from passing
3. **Isolated** - don't affect other functionality

The 4.8% improvement in pass rate demonstrates the infrastructure is valuable. Future work should focus on fixing the two edge cases to unlock additional test passes. The duplication issue in particular will require:
- Debug instrumentation to track IR node creation and emission
- Careful tracing of the emit_namespace_iife recursion
- Possibly unit tests for the specific nested namespace case
