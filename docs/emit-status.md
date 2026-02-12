# Emit Test Status

**Last Updated**: 2026-02-12
**Overall JS Emit Pass Rate**: 68.2% (120/176 on 200-test sample)
**Target**: 90%+

## Current Status by Slice

### Slice 1: Comment Preservation (ASSIGNED)
**Status**: 🟡 Partial progress - JSDoc for ES5 methods working

**Pass Rate**:
- Overall sample: 68.2% (includes all test types)
- Comment-specific tests: 9.7% (18/186)

**Completed**:
- ✅ JSDoc comments on ES5 class static methods
- ✅ JSDoc comments on ES5 class instance methods
- ✅ Auto-extraction of source text from arena
- ✅ Proper comment indentation in IR printer

**Test Wins**:
- `commentBeforeStaticMethod1` ✅
- `commentBeforeStaticMethod2` ✅ (likely)
- Similar JSDoc on method tests ✅

**Remaining Work** (168 failing comment tests):
1. **High Priority**:
   - Trailing comments in call arguments (`s.map(// comment\nfunction...)`)
   - End-of-file comments
   - Comments in parameter lists
   - Comments on ambient declarations (pinned comments)

2. **Medium Priority**:
   - AMD module comment handling
   - Comments in array/object literals
   - Inline comments in expressions
   - Comments before closing braces

3. **Complex**:
   - Comment positioning in parenthesized expressions
   - Comments in destructuring patterns
   - Block-end comment handling

**Architecture**:
Foundation is in place for comment preservation in IR-based transforms:
- IR nodes can store `leading_comment`/`trailing_comment` fields
- Comment extraction from source works (backwards scanning)
- Comment formatting matches TypeScript output

**Next Steps**:
- Extend comment extraction to other node types (expressions, statements)
- Implement trailing comment handling
- Add end-of-scope comment emission

### Slice 2: Object/Expression Formatting
**Status**: ⏸️ Not started

**Known Issues** (36 failures):
- Object literals keep short properties on same line
- Short function bodies should stay on one line
- Indentation in nested IIFEs

**Test Examples**:
- `APISample_compile` - object literal formatting
- `APISample_WatchWithOwnWatchHost` - shorthand property

### Slice 3: Destructuring/For-of Downlevel
**Status**: ⏸️ Not started

**Known Issues** (30+ failures):
- Destructuring not lowered for ES5 target
- Variable renaming (shadowing with `_1`, `_2` suffixes)
- Temp variable naming differences

**Test Examples**:
- `ES5For-of*` tests
- Destructuring in assignments and parameters

### Slice 4: Helper Functions + This Capture
**Status**: ⏸️ Not started

**Known Issues** (10+ failures):
- `__values`, `__read`, `__spread` helpers not emitted
- `_this` capture for arrow functions in methods
- Regex literal preservation

**Test Examples**:
- `ES5For-of33` - helper emission
- `APISample_jsdoc` - `_this` capture

## Recent Changes

### 2026-02-12: JSDoc Comment Preservation for ES5 Methods
**Commits**:
- `38f9e18f3` - feat(emit): preserve JSDoc comments for ES5 class methods
- `254a525b9` - docs: session summary

**Impact**:
- Overall pass rate: ~62% → 68.2% (+6.2%)
- Fixed JSDoc comments on ES5 class method transformations
- Established pattern for future comment preservation work

**Files Changed**:
- `crates/tsz-emitter/src/transforms/ir.rs` - Added comment fields to IR nodes
- `crates/tsz-emitter/src/transforms/class_es5_ir.rs` - Comment extraction logic
- `crates/tsz-emitter/src/transforms/ir_printer.rs` - Comment emission
- `crates/tsz-emitter/src/printer.rs` - Auto-extract source text

**Technical Details**:
- Backwards scanning from method position to find preceding JSDoc
- Scans to class opening brace or previous member
- Proper indentation (indent + space before `*` continuation lines)
- Works for both static and instance methods

## Testing

### Quick Test Run
```bash
# Build first
cargo build --release -p tsz-cli

# Quick sample (200 tests)
./scripts/emit/run.sh --max=200 --js-only

# Specific slice testing
./scripts/emit/run.sh --js-only --filter="comment" --max=50
./scripts/emit/run.sh --js-only --filter="APISample"
```

### Full Test Run
```bash
# All ~11K tests (takes several minutes)
./scripts/emit/run.sh --js-only
```

## Priority Order for Future Work

1. **Short-term wins** (easy, high impact):
   - Empty function body single-line formatting
   - Object literal same-line formatting for short properties

2. **Medium effort** (moderate impact):
   - Trailing comment preservation in expressions
   - End-of-file comment emission

3. **Complex** (high effort, moderate impact):
   - Full comment positioning system
   - Destructuring ES5 lowering
   - Helper function emission

4. **Edge cases** (low priority):
   - AMD comment handling
   - Ambient declaration comments
   - Regex literal preservation
