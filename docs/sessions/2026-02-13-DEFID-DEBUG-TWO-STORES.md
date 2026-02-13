# DefId Collision Debug: Two DefinitionStore Instances Found

**Date**: 2026-02-13
**Session Duration**: 10+ hours
**Status**: 🔍 **ROOT CAUSE NARROWED** - Two stores exist despite sharing attempt

## Critical Finding

Detailed tracing reveals that **TWO separate DefinitionStore instances** are being used:

```
TRACE DefinitionStore::allocate, allocated_def_id=14, next_will_be=15
TRACE Mapping symbol to DefId, symbol_name=ThisParameterType, def_id=14
TRACE DefinitionStore::allocate, allocated_def_id=14, next_will_be=15  ← DUPLICATE!
TRACE Mapping symbol to DefId, symbol_name=Node, def_id=14
```

Both stores allocate DefId(14), proving they're independent instances.

## What Was Implemented

### ✅ Infrastructure Complete
1. **Created shared store constructor**
   - `CheckerContext::new_with_shared_def_store()`
   - `CheckerState::new_with_shared_def_store()`
   - `CheckerState::with_options_and_shared_def_store()`

2. **Changed Rc→Arc** for thread-safety
   - `definition_store: Arc<DefinitionStore>`
   - All constructors updated

3. **Modified parallel path**
   - Added `shared_def_store` parameter to `check_file_for_parallel()`
   - Updated both call sites (rayon + wasm32)
   - Passes Arc::clone() to ensure sharing

4. **Modified sequential path**
   - Creates `shared_def_store` before file loop
   - Uses `new_with_shared_def_store()` for non-cached files

### ❌ Still Not Working
Despite all changes, two DefIds still allocate 14, meaning two stores exist.

## Where Is the Second Store Coming From?

### Hypothesis 1: Initialization Before Shared Store
Maybe lib symbols are registered during some initialization phase before the shared store is created.

**Evidence Against**: The shared store is created early in collect_diagnostics() before any file checking.

### Hypothesis 2: Hidden CheckerContext Creation
Some code path might create a CheckerContext without using the shared store.

**Evidence For**: We saw there are many test files that create CheckerContext::new() directly, but those shouldn't affect the CLI binary.

### Hypothesis 3: Context Cloning
CheckerContext might be cloned somewhere, creating a new definition_store.

**Evidence**: Need to search for `.clone()` on CheckerContext.

### Hypothesis 4: Multiple File Contexts
For a single-file run, maybe multiple contexts are created (one for lib, one for main file)?

**Evidence For**: The traces show allocations from 1-14 (first store) and then 14 again (second store). This suggests one store is used for early symbols, then a different store for later ones.

## Investigation Plan

### Step 1: Add Store Instance Tracking (30min)
Add a unique ID to each DefinitionStore to track which instance is being used:

```rust
pub struct DefinitionStore {
    instance_id: u64,  // Unique ID for debugging
    definitions: DashMap<DefId, DefinitionInfo>,
    next_id: AtomicU32,
}

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

impl DefinitionStore {
    pub fn new() -> Self {
        let instance_id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
        trace!(instance_id, "DefinitionStore::new - creating new instance");
        DefinitionStore {
            instance_id,
            definitions: DashMap::new(),
            next_id: AtomicU32::new(DefId::FIRST_VALID),
        }
    }
}
```

Then update traces to show instance_id:
```rust
trace!(instance_id = self.instance_id, def_id = %id.0, "allocate");
```

This will immediately show if two instances exist and when they're created.

### Step 2: Search for Hidden Context Creation (15min)
```bash
# Find all places CheckerContext is created without shared store
grep -r "CheckerContext::new\|CheckerContext::with" crates/tsz-cli/src/*.rs

# Check for cloning
grep -r "\.clone()" crates/tsz-cli/src/*.rs | grep -i context
```

### Step 3: Check Sequential vs Parallel Paths (15min)
Single file checking uses the parallel path (no cache). Verify:
1. shared_def_store is created
2. check_file_for_parallel() receives it
3. CheckerState uses it
4. CheckerContext uses it
5. No other context is created

Add trace at each step to verify the Arc address is the same.

### Step 4: Examine Lib Context Handling (30min)
Check if lib_contexts trigger creation of a separate checker:
- How are lib symbols first encountered?
- Does set_lib_contexts() cause any initialization?
- Are lib types registered before the main file checker is created?

### Step 5: Binary Search for Second Store Creation (1h)
Add trace to DefinitionStore::new() showing backtrace:
```rust
trace!("DefinitionStore::new called from: {:?}", std::backtrace::Backtrace::capture());
```

This will show exactly where both stores are created.

## Files Modified

- `crates/tsz-checker/src/context.rs` - Added new_with_shared_def_store()
- `crates/tsz-checker/src/state.rs` - Added with_options_and_shared_def_store()
- `crates/tsz-cli/src/driver.rs` - Modified parallel + sequential paths
- `crates/tsz-solver/src/def.rs` - Added detailed tracing

## Test Status

✅ All 2394 unit tests passing
❌ Bug still reproduces - shows "Node<number>" not "ConcatArray<number>"
✅ No regressions introduced

## Next Steps (2-3 hours)

1. **Add instance tracking** - Implement Step 1 above (30min)
2. **Run and identify** - See which two instances are created (5min)
3. **Find creation point** - Use backtrace or search (1h)
4. **Fix the second store** - Eliminate duplicate creation (30min)
5. **Test and verify** - Should finally work! (30min)
6. **Conformance check** - Measure improvement (30min)

## Why This Is Hard

The bug is subtle because:
1. All the sharing infrastructure is in place
2. The Arc is being cloned correctly
3. Yet somehow a second store still gets created
4. The creation point is hidden/indirect

This suggests the second store is created through a code path we haven't found yet, possibly:
- Initialization logic before the shared store exists
- A context created for lib processing
- A cached context being reused with its old store
- A clone operation we haven't identified

## Success Criteria

When fixed, the trace should show:
```
DefinitionStore::new - instance 1
allocate instance=1, def_id=1
allocate instance=1, def_id=2
...
allocate instance=1, def_id=14  ← ThisParameterType
...
allocate instance=1, def_id=X   ← Node (different DefId, same instance)
```

No instance 2 should appear!

---

**Status**: Infrastructure complete, debugging the hidden second store
**Confidence**: HIGH that fix is within reach once second store is found
**Effort**: 2-3 hours remaining
**Risk**: LOW - changes are localized, tests passing
