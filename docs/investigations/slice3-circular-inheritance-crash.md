# Slice 3: Circular Inheritance Crash Investigation

**Date**: 2026-02-12  
**Test**: `conformance/classes/classDeclarations/classHeritageSpecification/classExtendsItselfIndirectly3.ts`  
**Issue**: Test crashes with panic during compilation  
**Expected**: TS2506 "referenced directly or indirectly in its own base expression"

## Test Structure

Multi-file circular inheritance:
- File 1: `class C extends E`
- File 2: `class D extends C`  
- File 3: `class E extends D`

Creates cycle: C→E→D→C

## Investigation Findings

### Existing Protection (Two Layers)

**Layer 1: Pre-Resolution Check** (`class_inheritance.rs`)
- Function: `check_class_inheritance_cycle()`
- Uses: `InheritanceGraph` with DFS traversal
- When: BEFORE type resolution (during check_class_declaration)
- Emits: TS2506 error when cycle detected

**Layer 2: Runtime Protection** (`class_type.rs:550-560`)
- Tracks: `class_instance_resolution_set`  
- Purpose: Prevents infinite recursion during type resolution
- Action: Breaks loop if base class is currently being resolved

### Root Cause: Multi-File Timing Gap

Files are processed sequentially:
1. File 1 checked: C extends E (E not yet processed)
2. Inheritance Graph doesn't know E→D yet
3. Cycle check passes (incomplete graph)
4. File 2 checked: D extends C (graph now has C→E, D→C)
5. File 3 checked: E extends D (graph now has C→E, D→C, E→D)
6. Later type resolution triggers infinite recursion

The early cycle detection works for single-file or forward-declared cycles, but misses backward references in multi-file scenarios.

## Proposed Fix

### Option A: Two-Pass Cycle Detection

```rust
// After all files bound, re-check ALL classes for cycles
pub fn check_all_classes_for_cycles(&mut self) {
    for class_symbol_id in self.all_class_symbols() {
        if self.detects_cycle_from(class_symbol_id) {
            self.emit_ts2506_for_class(class_symbol_id);
        }
    }
}
```

Call this after file processing completes, when InheritanceGraph is fully populated.

### Option B: Lazy Cycle Detection

Keep existing early check, but enhance runtime protection:
- Add better error recovery in `class_instance_resolution_set` check
- Convert panic to TS2506 error emission
- Already partially implemented (lines 550-560 in class_type.rs)

## Testing Strategy

1. Create minimal unit test:
   ```rust
   #[test]
   fn test_multi_file_circular_inheritance() {
       // File 1: class C extends E
       // File 2: class D extends C  
       // File 3: class E extends D
       // Should emit TS2506, not crash
   }
   ```

2. Verify fix prevents panic
3. Confirm TS2506 error is emitted
4. Check conformance test passes

## Status

- **Investigated**: ✓ Root cause identified
- **Fix Designed**: ✓ Two approaches outlined  
- **Implementation**: ⏸️ Blocked by build environment issues
- **Testing**: ⏸️ Waiting for fix implementation

## Build Environment Note

Cargo builds are being killed (OOM or resource limits), preventing:
- Binary compilation
- Test execution  
- Fix verification

Need to resolve build environment before implementing/testing fix.
