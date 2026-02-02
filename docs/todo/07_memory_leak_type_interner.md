# Fix Memory Leak: TypeInterner GC

**Reference**: Architectural Review Summary - Issue #1  
**Severity**: 🔴 Critical  
**Status**: TODO  
**Priority**: Critical - LSP viability depends on this

---

## Problem

`TypeInterner` uses append-only strategy with `AtomicU32` indices and no garbage collection mechanism. In LSP (long-running process), ephemeral types accumulate indefinitely. Types are never freed without compacting entire array and remapping ALL live `TypeId`s.

**Impact**: LSP server will OOM over time, making it non-viable for production use.

**Location**: `src/solver/intern.rs`

---

## Solution: Scoped Type Interner

Implement generational arena or "local" interner for ephemeral types vs. "global" interner for declarations, with per-file/per-request reset mechanism.

### Design: TypeId Partitioning

Partition the `u32` space using the Most Significant Bit (MSB):
- **Global IDs** (`0x00000000` - `0x7FFFFFFF`): Managed by `TypeInterner`. Persist for the lifetime of the project/server.
- **Local IDs** (`0x80000000` - `0xFFFFFFFF`): Managed by a new `ScopedTypeInterner`. Valid only for the current operation/request.

### Invariant

- **Global types** can only reference other **Global types** or **Intrinsics**.
- **Local types** can reference **Global types** or **Local types**.
- This invariant is naturally enforced if declaration lowering uses the Global interner and expression checking uses the Local interner.

---

## Implementation Phases

### Phase 1: TypeId Partitioning & API Prep

**File**: `src/solver/types.rs`
1. Add helper methods to `TypeId`:
   ```rust
   impl TypeId {
       pub const LOCAL_MASK: u32 = 0x80000000;
       pub fn is_local(self) -> bool { (self.0 & Self::LOCAL_MASK) != 0 }
       pub fn is_global(self) -> bool { !self.is_local() }
   }
   ```

**File**: `src/solver/intern.rs`
2. Modify `TypeInterner::make_id` to ensure it only generates IDs within the global range (assert `local_index` fits in 31 bits).

### Phase 2: Implement ScopedTypeInterner

**File**: `src/solver/intern.rs`
1. Extract the storage logic from `TypeInterner` into a reusable `InternerStorage` struct or keep `TypeInterner` logic but allow `ScopedTypeInterner` to replicate the structure.
2. Implement `ScopedTypeInterner`:
   - Constructor: `pub fn new(parent: &'a TypeInterner) -> Self`
   - It initializes its own empty shards.
   - `intern(key)` logic:
       1. Check if `key` contains any Local `TypeId`s.
          - If YES: Must intern locally.
          - If NO: Check `parent.lookup(key)`. If found, return Global ID. Else, intern locally.
       2. **Optimization**: Always intern locally by default during checking to avoid polluting global cache, *unless* it's a declaration.
   - `lookup(id)` logic:
       - If `id.is_global()`: Delegate to `parent.lookup(id)`.
       - If `id.is_local()`: Lookup in local shards (masking out the MSB).

3. Implement `TypeDatabase` trait for `ScopedTypeInterner`.

### Phase 3: Integration with Checker

**File**: `src/checker/context.rs`
1. Update `CheckerContext` to hold the `ScopedTypeInterner` (or `&dyn QueryDatabase` which points to it).

**File**: `src/checker/state.rs`
2. Update `CheckerState::new`. It currently takes `&'a dyn QueryDatabase`.
3. In the driver/LSP loop, we need to construct the `ScopedTypeInterner` before creating `CheckerState`.

### Phase 4: Lifecycle Management (The Fix)

**File**: `src/lib.rs` or `src/driver.rs` (wherever the check loop is)
1. Instantiate `GlobalTypeInterner` once at startup.
2. Inside the loop/request handler:
   ```rust
   let global_interner = ...; // Long-lived
   // ... parse / bind ...
   
   // Create ephemeral interner for this check operation
   let local_interner = ScopedTypeInterner::new(&global_interner);
   
   // Run checker using local_interner
   let checker = CheckerState::new(..., &local_interner, ...);
   checker.check_source_file(...);
   
   // local_interner is dropped here, freeing all ephemeral types!
   ```

---

## Testing Strategy

### Unit Tests (`src/solver/tests/scoped_interner_tests.rs`)
1. **Scoping**: Create global interner, create scoped interner. Intern a type in scoped. Drop scoped. Verify type is gone (cannot be looked up in global).
2. **Visibility**: Intern type in global. Verify scoped can see it.
3. **ID Ranges**: Verify global IDs have MSB 0, local IDs have MSB 1.
4. **Cross-reference**: Create a local Union containing a global Object. Verify lookup works.

### Integration Test (Memory Leak Simulation)
1. Create a loop that runs 10,000 iterations.
2. In each iteration:
   - Create `ScopedTypeInterner`.
   - Create a complex type (e.g., deeply nested union/intersection).
   - Drop `ScopedTypeInterner`.
3. Measure RSS (Resident Set Size) before and after. It should remain stable, unlike the current implementation where it grows linearly.

---

## Migration Plan

1. **Refactor `TypeInterner`**: Move internal sharding logic to a generic `ShardedStorage` struct to allow code reuse between Global and Scoped interners without duplication.
2. **Implement `ScopedTypeInterner`**: Add the struct and trait impls.
3. **Update Call Sites**:
   - **Declaration Lowering** (Binder/Symbol creation): Must continue to use `GlobalInterner` (or `Scoped` but forcing global promotion, though usually declarations are lowered once).
   - **Expression Checking** (Checker): Switch to passing `ScopedTypeInterner`.
4. **Verify**: Run the conformance test suite to ensure no regressions in type identity logic.

---

## Acceptance Criteria

- [ ] `ScopedTypeInterner` implemented with proper ID partitioning
- [ ] Checker uses scoped interner for expression checking
- [ ] Memory leak test shows stable RSS after 10,000 iterations
- [ ] Conformance tests pass with no regressions
- [ ] LSP can run indefinitely without OOM
