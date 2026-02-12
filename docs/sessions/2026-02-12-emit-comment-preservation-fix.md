# Emit Comment Preservation Fix - Session 2026-02-12

## Assignment: Slice 1 - Comment Preservation

**Target**: Fix 52 comment-related failures (41 line-comment + 11 inline-comment)
**Current**: ~62% JS emit pass rate → Goal: 90%+

## Bug Identified

### Problem

Comments appearing after type-only declarations (interfaces, type aliases) are lost during emit.

**Minimal Reproduction**:
```typescript
export interface Foo {
    x: number;
}

// Comment before function
function bar() {
    return 100;
}
```

**Expected Output** (TypeScript):
```javascript
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
// Comment before function
function bar() {
    return 100;
}
```

**Actual Output** (tsz - before fix):
```javascript
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
function bar() {
    return 100;
}
```

The comment is completely dropped!

### Root Cause

**File**: `crates/tsz-emitter/src/emitter/comment_helpers.rs`
**Function**: `skip_comments_for_erased_node` (line 200)

**The Bug**:
```rust
pub(super) fn skip_comments_for_erased_node(&mut self, node: &Node) {
    while self.comment_emit_idx < self.all_comments.len() {
        let c = &self.all_comments[self.comment_emit_idx];
        if c.end <= node.end {  // ← BUG: node.end includes trailing trivia!
            self.comment_emit_idx += 1;
        } else {
            break;
        }
    }
}
```

**Why it fails**:
1. TypeScript's parser sets `node.end` to include trailing trivia (whitespace + comments)
2. A comment after the interface's `}` but before the next statement may have `c.end <= node.end`
3. So it gets skipped even though it should be preserved for the next statement

## Fix Implemented

**Solution**: Use `find_token_end_before_trivia` to find the actual end of the node's code, excluding trailing comments.

**Modified Code**:
```rust
pub(super) fn skip_comments_for_erased_node(&mut self, node: &Node) {
    // Find the actual end of the node's code content, excluding trailing trivia.
    // This prevents us from skipping comments that appear after the closing brace/token
    // but before the next statement (which should be emitted as leading comments for
    // that next statement).
    let actual_end = self.find_token_end_before_trivia(node.pos, node.end);

    while self.comment_emit_idx < self.all_comments.len() {
        let c = &self.all_comments[self.comment_emit_idx];
        // Only skip comments whose end is within the actual node content
        if c.end <= actual_end {  // ← FIXED: Use actual_end instead of node.end
            self.comment_emit_idx += 1;
        } else {
            break;
        }
    }
}
```

## Changes Made

**File Modified**:
- `crates/tsz-emitter/src/emitter/comment_helpers.rs` (lines 200-217)

**Change Summary**:
1. Added call to `find_token_end_before_trivia(node.pos, node.end)` to get actual code end
2. Changed comparison from `c.end <= node.end` to `c.end <= actual_end`
3. Added explanatory comments

## Testing Status

**Build**: In progress (as of session end)

**Next Steps**:
1. Wait for build to complete
2. Test minimal case: `tmp/test_comment3.ts`
3. Run emit test suite: `./scripts/emit/run.sh --max=50 --js-only`
4. Check APISample_jsdoc specifically: `./scripts/emit/run.sh --js-only --verbose --filter="APISample_jsdoc"`
5. Run full unit tests: `cargo nextest run --release`
6. If passing, commit and push

## Expected Impact

This fix should address:
- **Primary**: Comments after interfaces/type aliases (estimated 15-30 tests)
- **Secondary**: May help with other comment placement issues

**Estimated improvement**: 5-10% increase in emit pass rate

## Related Failures

This fix specifically targets the issue seen in:
- `APISample_jsdoc` - Missing comment before `getAnnotations` function
- Any test with comments between type-only and value declarations

## Verification Commands

```bash
# 1. Build
cargo build --profile dist-fast -p tsz-cli

# 2. Test minimal case
./.target/dist-fast/tsz --noCheck --noLib --module commonjs tmp/test_comment3.ts
cat tmp/test_comment3.js
# Should now show: "// Comment before function"

# 3. Run emit tests
./scripts/emit/run.sh --max=100 --js-only

# 4. Run unit tests
cargo nextest run --release -p tsz-emitter

# 5. Commit if passing
git add crates/tsz-emitter/src/emitter/comment_helpers.rs
git commit -m "fix(emit): preserve comments after erased type declarations

- Fix skip_comments_for_erased_node to only skip comments within node content
- Use find_token_end_before_trivia to exclude trailing trivia from range
- Preserves leading comments for next statement after interfaces/type aliases"
git push origin main
```

## Notes

- The `find_token_end_before_trivia` helper already existed and does exactly what we need
- This is a minimal, surgical fix - only changes the boundary check for comment skipping
- Should not affect other emit functionality

## Session Summary

- ✅ Identified systematic bug in comment preservation
- ✅ Created minimal reproduction
- ✅ Implemented targeted fix
- ⏳ Build in progress at session end
- ⏳ Testing pending
- ⏳ Commit pending

**Total Time**: ~1 hour of investigation + implementation
**Complexity**: Low - one-line logic change with helper function reuse
