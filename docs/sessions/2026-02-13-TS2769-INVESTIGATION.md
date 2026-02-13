# TS2769 Investigation: Phantom "Node" Type Bug

**Date**: 2026-02-13
**Time Spent**: ~3 hours investigation
**Status**: Root cause narrowed down, ready for fix

## Critical Finding

The TS2769 "No overload matches" error shows a **phantom "Node" type** that doesn't exist in the source code.

### Test Case

```typescript
// @noLib: true
type Fn<T extends object> = <U extends T>(subj: U) => U

function test<T extends object, T1 extends T>() {
  let b: Array<Fn<T1>> = [];
  let a: Array<Fn<T>> = [];
  b.concat(a);
}
```

### Expected Behavior

TSC accepts this with no errors. The `concat` signature is:
```typescript
concat(...items: ConcatArray<T>[]): T[];
```

When called on `Array<Fn<T1>>`, the parameter type should be `ConcatArray<Fn<T1>>[]`.

### Actual Behavior (tsz)

```
error TS2769: No overload matches this call.
  Argument of type 'Fn<T>[]' is not assignable to parameter of type 'Node<Fn<T1>>'.
```

**The "Node<Fn<T1>>" type doesn't exist anywhere!**

## Key Discoveries

1. **Phantom type appears even with `@noLib: true`**
   - Not from DOM lib's `Node` interface
   - Generated internally during type checking

2. **Generic function subtyping works correctly**
   - `Fn<T>` vs `Fn<T1>` assignability is NOT the issue
   - Tested in isolation - works fine

3. **Array variance works correctly**
   - `Array<Fn<T>>` assignments work
   - Not an array covariance issue

4. **Bug is in overload resolution**
   - Error generated at `error_reporter.rs:1640`
   - Called from `type_computation_complex.rs:440`
   - Overload resolution creates "failures" with wrong types

## Code Locations

### Error Generation
```rust
// crates/tsz-checker/src/error_reporter.rs:1598
pub fn error_no_overload_matches_at(
    &mut self,
    idx: NodeIndex,
    related_diags: &[OverloadFailureDiagnostic],
) {
    // Formats failure diagnostics into error message
}
```

### Overload Resolution Call Sites
```rust
// crates/tsz-checker/src/type_computation_complex.rs:440
CallResult::NoOverloadMatch { failures, .. } => {
    self.error_no_overload_matches_at(idx, &failures);
}
```

## Hypotheses

### Hypothesis 1: Type Parameter Inference Bug
During overload resolution, when inferring type parameters for `concat<T>`, the inference algorithm might be:
- Picking up wrong type from environment
- Creating incorrect type parameter substitution
- Generating phantom "Node" as intermediate type

### Hypothesis 2: Type Formatting Bug
The error message formatter might be:
- Misformatting some intermediate type representation
- Showing internal type ID instead of proper name
- Confusion between type parameter names

### Hypothesis 3: ConcatArray Resolution
The `ConcatArray<T>` type might be:
- Incorrectly resolved to some other generic type
- Confused with another generic interface in the environment
- Having its type parameter incorrectly inferred

## Next Steps

### 1. Trace Overload Resolution (2-3 hours)
```bash
TSZ_LOG="tsz_checker::call_checker=trace" TSZ_LOG_FORMAT=tree \
  .target/dist-fast/tsz tmp/no-lib-test.ts 2>&1 | less
```

Look for:
- What overload candidates are being tried
- What types are inferred for each candidate
- Where "Node" appears in the trace

### 2. Check Type Printer (1 hour)
Find how types are formatted for error messages:
- Search for type-to-string conversion
- Check if "Node" is a placeholder for some internal type
- Verify type parameter printing

### 3. Debug Overload Matching (2-3 hours)
Add instrumentation to:
- `resolve_overloaded_call_with_signatures` in call_checker.rs
- Type parameter inference during overload matching
- Parameter type checking for each candidate

### 4. Fix and Verify (1-2 hours)
Once root cause is found:
- Implement fix
- Run all 6 affected tests
- Verify conformance improvements
- Check for regressions

## Files to Modify

**Primary suspects**:
- `crates/tsz-checker/src/call_checker.rs` - Overload resolution logic
- `crates/tsz-solver/src/application.rs` - Type parameter inference
- `crates/tsz-solver/src/infer.rs` - Generic inference

**Secondary suspects**:
- `crates/tsz-checker/src/error_reporter.rs` - Error formatting
- Type printer/formatter code (location TBD)

## Impact

**Tests affected**: 6 in sample of 300 → 20-30+ in full suite
**Pass rate improvement**: 90.3% → 92-93%+
**Priority**: HIGH (best ROI for time invested)

## Investigation Time

- Phase 1: Initial analysis (1 hour) ✅
- Phase 2: Isolation and reproduction (1 hour) ✅  
- Phase 3: Code location identification (1 hour) ✅
- Phase 4: Tracing and fix (4-6 hours) 🔄 Next session

**Total time**: ~3 hours spent, 4-6 hours remaining

---

**Status**: Ready for next session. Clear reproduction case, code locations identified, specific tracing steps documented.

## BREAKTHROUGH: Root Cause Mechanism Identified

**Time**: After 3.5 hours of investigation
**Status**: 60% complete - mechanism understood, exact fix location identified

### The Smoking Gun

The phantom "Node<Fn<T1>>" is a `TypeKey::Application` (generic type application):
- Format code: `crates/tsz-solver/src/format.rs:282-308`
- Structure: `Application(base_type, [Fn<T1>])`
- Base type resolves to "Node" string

### Three Possible Sources for "Node"

1. **Lazy(DefId) resolution** (lines 288-294):
   ```rust
   if let Some(TypeKey::Lazy(def_id)) = base_key {
       if let Some(def) = def_store.get(def_id) {
           self.atom(def.name).to_string()  // Returns "Node"
       }
   }
   ```

2. **Object with symbol** (lines 196-200):
   ```rust
   if let Some(sym_id) = shape.symbol {
       if let Some(sym) = arena.get(sym_id) {
           return sym.escaped_name.to_string();  // Returns "Node"
       }
   }
   ```

3. **Other formatting path** producing "Node"

### Most Likely Root Cause

**Hypothesis**: Overload resolution is creating an `Application` type where the base incorrectly references DOM's `Node` interface, even though test uses `@noLib: true`.

**Evidence**:
- Error persists even without lib files
- "Node" is a real type in DOM (dom.generated.d.ts)
- Suggests type cache pollution or incorrect type lookup

### Next Debugging Steps

#### Step 1: Add Instrumentation (30 min)
```rust
// In format.rs, line 288
let base_str = if let Some(TypeKey::Lazy(def_id)) = base_key {
    if let Some(def_store) = self.def_store {
        if let Some(def) = def_store.get(def_id) {
            let name = self.atom(def.name).to_string();
            eprintln!("DEBUG: Application base Lazy({}) resolves to '{}'", def_id.0, name);
            name
        } else {
            format!("Lazy({})", def_id.0)
        }
    } else {
        format!("Lazy({})", def_id.0)
    }
} else {
    self.format(app.base)
};
```

#### Step 2: Run Test and Capture DefId (15 min)
```bash
.target/dist-fast/tsz tmp/no-lib-test.ts 2>&1 | grep "DEBUG:"
```

#### Step 3: Trace DefId Creation (2-3 hours)
- Find where this DefId is created
- Check why it references "Node"
- Trace back through overload resolution to see where Application is generated

### Expected Fix

Once we find where the wrong Application type is created:
1. Fix type parameter inference during overload matching
2. OR fix type lookup that's finding wrong "Node" type
3. OR clear type cache that's polluted with DOM types

### Time Estimate

- Step 1-2: 45 minutes (instrumentation + initial trace)
- Step 3: 2-3 hours (find exact bug location)
- Step 4: 30-60 minutes (implement fix)
- Step 5: 30 minutes (verify, test, commit)

**Total remaining**: 3.5-4.5 hours

---

**Investigation Status**: 60% complete
- ✅ Reproduced bug
- ✅ Isolated to overload resolution
- ✅ Found formatting mechanism
- ✅ Identified 3 possible sources
- 🔄 Need: Add instrumentation
- 🔄 Need: Trace DefId
- 🔄 Need: Fix root cause

**Ready for**: Next session to complete instrumentation and fix
