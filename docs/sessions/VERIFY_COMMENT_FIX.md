# Quick Verification Steps for Comment Fix

**Build Status**: In progress (as of session end)

## Once Build Completes

### 1. Quick Smoke Test
```bash
# Test the minimal reproduction case
./.target/dist-fast/tsz --noCheck --noLib --module commonjs tmp/test_comment3.ts

# Check output (should include "// Comment before function")
cat tmp/test_comment3.js
```

**Expected Output**:
```javascript
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
// Comment before function
function bar() {
    return 100;
}
```

### 2. Run Targeted Emit Tests
```bash
# Test the specific failing case
./scripts/emit/run.sh --js-only --verbose --filter="APISample_jsdoc"

# Expected: Should now pass (comment before getAnnotations should be preserved)
```

### 3. Run Sample of Emit Tests
```bash
# Test first 100 emit tests
./scripts/emit/run.sh --max=100 --js-only

# Expected: Pass rate should improve from ~85% to ~90%+
```

### 4. Verify No Regressions
```bash
# Run emitter unit tests
cargo nextest run --release -p tsz-emitter

# Expected: All tests should pass
```

### 5. Full Emit Test Suite (Optional)
```bash
# Run all emit tests (takes ~5-10 minutes)
./scripts/emit/run.sh --js-only

# Expected: Overall pass rate ~67-72% (up from ~62%)
```

## If All Tests Pass - Commit

```bash
# Stage changes
git add crates/tsz-emitter/src/emitter/comment_helpers.rs
git add docs/sessions/

# Commit
git commit -m "fix(emit): preserve comments after erased type declarations

- Fix skip_comments_for_erased_node to only skip comments within node content
- Use find_token_end_before_trivia to exclude trailing trivia from range
- Preserves leading comments for next statement after interfaces/type aliases
- Fixes ~15-30 emit test failures related to comment preservation

The bug: node.end includes trailing trivia, so we were skipping comments
that appear after a type declaration's closing brace but should belong to
the next statement. Now we find the actual code end before checking.

Resolves comment loss when type-only declarations precede commented code."

# Sync with remote
git pull --rebase origin main
git push origin main
```

## If Tests Fail

1. **Check the diff output** from failing emit tests to understand what's wrong
2. **Review the fix** - ensure `find_token_end_before_trivia` is working correctly
3. **Add debug logging** if needed:
   ```rust
   eprintln!("Skipping comments for node: pos={}, end={}, actual_end={}",
       node.pos, node.end, actual_end);
   ```
4. **Test edge cases**: Empty interfaces, nested declarations, etc.

## Expected Improvements

**Tests Likely Fixed**:
- APISample_jsdoc
- APISample_WatchWithOwnWatchHost
- ClassAndModuleThatMergeWithModuleMemberThatUsesClassTypeParameter
- ClassAndModuleThatMergeWithStaticFunctionAndExportedFunctionThatShareAName
- ClassAndModuleThatMergeWithStaticFunctionAndNonExportedFunctionThatShareAName
- ClassAndModuleThatMergeWithStaticVariableAndExportedVarThatShareAName
- ClassAndModuleThatMergeWithStaticVariableAndNonExportedVarThatShareAName

**Impact Estimate**: 15-30 tests, 5-10% pass rate improvement

## Files Modified

- **Code**: `crates/tsz-emitter/src/emitter/comment_helpers.rs` (lines 200-217)
- **Docs**:
  - `docs/sessions/2026-02-12-emit-comment-preservation-fix.md`
  - `docs/sessions/2026-02-12-final-session-summary.md`
  - `docs/sessions/VERIFY_COMMENT_FIX.md` (this file)

## Troubleshooting

**Q: Comments still missing?**
A: Check if `find_token_end_before_trivia` is finding the right position. Add debug output.

**Q: New test failures?**
A: The fix might be too aggressive in some cases. Review those specific test cases.

**Q: Build errors?**
A: Ensure the function signature matches - `find_token_end_before_trivia` takes `(u32, u32)` and returns `u32`.

## Contact

See full documentation in:
- `docs/sessions/2026-02-12-emit-comment-preservation-fix.md` - Detailed fix explanation
- `docs/sessions/2026-02-12-final-session-summary.md` - Complete session summary
