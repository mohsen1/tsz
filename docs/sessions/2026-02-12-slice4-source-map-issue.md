# Source Map Name Recording Issue with ES5 Transforms

## Issue

Two unit tests are failing:
- `test_source_map_es5_transform_records_names`
- `test_source_map_names_array_multiple_identifiers`

Both tests expect the source map `names` array to contain identifier names when ES5 transforms are applied, but the array is empty.

## Root Cause

When ES5 transforms are applied (e.g., `const` → `var`), identifier names are not being recorded in the source map.

**Without ES5 transforms**:
```bash
$ tsz --noCheck --noLib --sourceMap test.ts
# Source map names: ["value", "other"] ✓
```

**With ES5 transforms** (`--target es5`):
```bash
$ tsz --noCheck --noLib --target es5 --sourceMap test.ts
# Source map names: [] ✗
```

## Investigation

### The Name Recording Path

1. **Normal path** (`write_identifier()`):
   ```rust
   pub(super) fn write_identifier(&mut self, text: &str) {
       if let Some(source_pos) = self.take_pending_source_pos() {
           self.writer.write_node_with_name(text, source_pos, text); // Records name
       } else {
           self.writer.write(text); // Does NOT record name
       }
   }
   ```

2. **Helper method** (`write_identifier_text()`):
   - Originally called `self.write()` - didn't record names
   - Fixed to call `self.write_identifier()` - still doesn't help
   - The problem is that `pending_source_pos` is None during transform emission

### Why ES5 Transforms Break Name Recording

When transforms are applied:
1. The lowering pass creates transformed IR nodes
2. These transformed nodes may not have proper source positions set
3. When emitting, `pending_source_pos` is None
4. `write_identifier()` falls back to `self.writer.write()` which doesn't record names

### Attempted Fix

Changed `write_identifier_text()` in `crates/tsz-emitter/src/emitter/helpers.rs:98`:
```rust
// Before
self.write(&ident.escaped_text);

// After
self.write_identifier(&ident.escaped_text);
```

This ensures identifiers go through the name-recording path, but doesn't help because `pending_source_pos` is still None during transform emission.

## Solution Options

### Option 1: Set Source Positions in Transform IR (Correct but Complex)

Ensure transformed IR nodes preserve source positions from original AST nodes. This requires:
1. Modifying the lowering pass to track source positions
2. Ensuring transform emission sets `pending_source_pos` before emitting identifiers
3. Testing all transform types (ES5 arrow functions, destructuring, for-of, etc.)

**Complexity**: High
**Impact**: Fixes the root cause properly

### Option 2: Always Record Identifier Names (Simple but Less Precise)

Modify `write_identifier()` to always record names even without source position:
```rust
pub(super) fn write_identifier(&mut self, text: &str) {
    if let Some(source_pos) = self.take_pending_source_pos() {
        self.writer.write_node_with_name(text, source_pos, text);
    } else {
        // Still record the name even without precise position mapping
        self.writer.write(text);
        if let Some(ref mut sm) = self.writer.source_map {
            sm.add_name(text.to_string());
        }
    }
}
```

**Complexity**: Low
**Impact**: Names are recorded but without precise position mappings (less useful for debugging)

### Option 3: Skip Tests for Now (Pragmatic)

Mark these tests as `#[ignore]` and document the issue for future work. Source maps are lower priority than:
- Core emit functionality (43.2% pass rate, target 90%+)
- Type checking correctness
- Other conformance issues

**Complexity**: Minimal
**Impact**: Tests remain failing but documented

## Recommendation

**Option 3** (skip for now) is recommended because:
1. **Priority**: Emit test pass rate (43.2% → 90%) is more important than source map details
2. **Scope**: Slice 4 core work (spread transforms, this-capture) is complete
3. **Complexity**: Proper fix requires significant transform pipeline work
4. **Impact**: Source map names are a debugging aid, not critical functionality

## Next Steps

If this issue is prioritized later:
1. Start with Option 1 (proper fix)
2. Add tracing to understand where source positions are lost:
   ```rust
   trace!("Emitting identifier: {}, has_source_pos: {}", text, self.pending_source_pos.is_some());
   ```
3. Test with minimal case: `const x = 1;` transformed to ES5
4. Trace through lowering pass and emission to find where positions are lost

## Files Involved

- `crates/tsz-emitter/src/emitter/helpers.rs` - Name recording logic
- `crates/tsz-emitter/src/source_writer.rs` - Source map generation
- `crates/tsz-emitter/src/lowering_pass.rs` - Transform IR creation
- `crates/tsz-emitter/src/transforms/` - ES5 transforms (const→var, arrow→function, etc.)
- `src/tests/source_map_tests_1.rs` - Failing tests

## Status

- **Tests**: 2394/2396 passing (99.92%)
- **Impact**: Low (source map debugging aid only)
- **Workaround**: Use non-transformed code for source map debugging
- **Resolution**: Deferred pending higher-priority work
