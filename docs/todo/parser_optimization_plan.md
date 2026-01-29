# Parser Optimization Plan

**Status**: Phases 1-5 Complete - 53% Improvement Achieved
**Last Updated**: January 29, 2026
**Goal**: Improve parser throughput from ~70 MiB/s to 200 MiB/s
**Current**: ~107 MiB/s (53% improvement toward target)

---

## Progress Update

### Completed ✅
- **Phase 1**: Scanner Accessor Update (commit 30a2a5bf1)
  - Added source slice fallback to `get_token_value_ref()`
  - Enables zero-allocation token value access
  - Foundation for future optimizations

- **Phase 2**: Identifier Optimization (Already Applied)
  - `scan_identifier` uses `token_value.clear()` (line 1167)
  - Identifiers now use zero-copy source slices

- **Phases 3&4**: Numeric Literal Optimization (commit 85147f7d9)
  - Only allocate token_value for numbers with separators
  - Plain numbers (42, 123.45, 0xABCD) use zero-copy fallback
  - Applied to all numeric paths: decimal, hex, binary, octal
  - BigInt literals still allocate (must strip 'n' suffix)

### Remaining 🚧
- Phase 6: N/A - Operator tokens don't use token_value
- Phase 7: Parser IdentifierData (Future - requires AST changes)

### Phase 6: Operator Tokens ✅ (No Work Needed)
- Investigation complete: Operator tokens identified purely by SyntaxKind
- No `token_value` assignments for `+`, `*`, `===`, `->`, etc.
- Remaining `token_value` allocations are necessary:
  - Private identifiers (must contain `#` prefix)
  - Regex literals (flag validation)
  - Template literals (escape processing)
  - JSDoc tokens (text processing)

### Phase 5: JSX Identifier Optimization ✅ (Already Applied)
- `scan_jsx_identifier` already uses zero-allocation pattern
- Lines 1639-1643: `self.token_atom` interned, `self.token_value.clear()`
- No changes needed

---

## Problem Statement

Current benchmarks show:
- Scanner throughput: ~140 MiB/s
- Parser throughput: ~70 MiB/s (50% of scanner)
- Target throughput: 200 MiB/s

The parser is 2x slower than the scanner alone, indicating significant overhead in the parsing phase.

---

## Root Cause Analysis

### Primary Bottleneck: Redundant String Allocation

In `src/scanner_impl.rs:1165`, every identifier causes a `String` allocation:

```rust
fn scan_identifier(&mut self) {
    let text_slice = &self.source[start..self.pos];
    self.token = crate::scanner::text_to_keyword(text_slice).unwrap_or(SyntaxKind::Identifier);
    self.token_atom = self.interner.intern(text_slice);  // Already interned!
    self.token_value = text_slice.to_string();           // <-- REDUNDANT ALLOCATION
}
```

This is wasteful because:
1. We already have the text in `self.source` (Arc<str>)
2. We already have the interned `Atom` for O(1) comparison
3. The parser uses `get_token_value_ref()` which could use the source slice

### Secondary Issues

1. **41 `token_value` assignments** throughout scanner_impl.rs
2. **Numeric literals** allocate even when no processing needed
3. **Parser still allocates** when creating `IdentifierData`

---

## Optimization Strategy: "Lazy Source Fallback"

Instead of making `token_value` an `Option` (adds branching overhead), adopt a policy:
- **Keep `token_value` empty** for tokens that match their source text
- **Fall back to source slices** in the accessor

This maintains API compatibility while eliminating allocations.

---

## Implementation Phases

### Phase 1: Scanner Accessor Update

**File**: `src/scanner_impl.rs`

Update `get_token_value_ref()` to implement the fallback pattern:

```rust
#[inline]
pub fn get_token_value_ref(&self) -> &str {
    // 1. Fast path: interned identifier
    if self.token_atom != Atom::NONE {
        return self.interner.resolve(self.token_atom);
    }
    
    // 2. Processed value (strings with escapes, etc.)
    if !self.token_value.is_empty() {
        return &self.token_value;
    }
    
    // 3. Fallback: raw source slice
    &self.source[self.token_start..self.pos]
}
```

**Risk**: Low - accessor change is backwards compatible
**Impact**: Enables subsequent optimizations

### Phase 2: Identifier Optimization

**File**: `src/scanner_impl.rs` (line ~1165)

Change `scan_identifier()`:

```rust
// BEFORE:
self.token_value = text_slice.to_string();

// AFTER:
self.token_value.clear();
```

**Risk**: Low - `get_token_value_ref()` handles empty case
**Impact**: Eliminates ~30-40% of scanner allocations (identifiers are most common token)

### Phase 3: JSX Identifier Optimization

**File**: `src/scanner_impl.rs`

Apply same pattern to `scan_jsx_identifier()`:

```rust
// Replace:
self.token_value = self.substring(self.token_start, self.pos);

// With:
self.token_atom = self.interner.intern(&self.source[self.token_start..self.pos]);
self.token_value.clear();
```

**Risk**: Low
**Impact**: Eliminates JSX identifier allocations

### Phase 4: Numeric Literal Optimization (Optional)

**File**: `src/scanner_impl.rs`

Only allocate `token_value` for numbers with separators:

```rust
fn scan_numeric_literal(&mut self) {
    let text = &self.source[self.token_start..self.pos];
    
    if text.contains('_') {
        // Need to strip separators - allocate
        self.token_value = text.replace('_', "");
    } else {
        // Matches source exactly - no allocation
        self.token_value.clear();
    }
}
```

**Risk**: Medium - need to verify all numeric paths
**Impact**: Further allocation reduction

### Phase 5: Parser IdentifierData (Future)

**File**: `src/parser/node.rs`

Long-term optimization - store `Atom` instead of `String`:

```rust
// BEFORE:
pub struct IdentifierData {
    pub escaped_text: String,  // 24 bytes + heap
}

// AFTER:
pub struct IdentifierData {
    pub atom: Atom,  // 4 bytes, Copy, no heap
}
```

**Risk**: High - requires updating all IdentifierData consumers
**Impact**: Eliminates all parser identifier allocations

---

## Verification Plan

### Before Each Phase

1. Run parser benchmark: `cargo bench --bench parser_bench -- parser_throughput`
2. Record baseline throughput

### After Each Phase

1. Run same benchmark
2. Compare throughput improvement
3. Run full test suite: `./scripts/test.sh`
4. Run conformance: `./conformance/run.sh --server --max=1000`

### Success Criteria

| Phase | Expected Improvement |
|-------|---------------------|
| Phase 1 | ~0% (enables others) |
| Phase 2 | +20-30% |
| Phase 3 | +5% (JSX files only) |
| Phase 4 | +5-10% |
| Phase 5 | +20-30% (future) |

**Target**: Reach 120-150 MiB/s after Phases 1-4

### Benchmark Results (After Phases 1-5)

**Parser Throughput**: ~107 MiB/s (53% improvement from ~70 MiB/s baseline)

- 10c_10m: 102.27 MiB/s
- 20c_10m: 109.13 MiB/s
- 50c_5m: 107.93 MiB/s

**Gap to Target**: ~93 MiB/s (87% more improvement needed for 200 MiB/s)

**Next Steps**: Phase 7 (Parser IdentifierData) - requires AST changes (high-risk, marked as future work)

---

## Rollback Plan

Each phase is independent. If a phase causes issues:
1. Revert the specific change
2. Skip to next phase
3. Investigate root cause separately

---

## Files to Modify

| File | Phase | Changes |
|------|-------|---------|
| `src/scanner_impl.rs` | 1 | Update `get_token_value_ref()` |
| `src/scanner_impl.rs` | 2 | Modify `scan_identifier()` |
| `src/scanner_impl.rs` | 3 | Modify `scan_jsx_identifier()` |
| `src/scanner_impl.rs` | 4 | Modify numeric scanning |
| `src/parser/node.rs` | 5 | Change `IdentifierData` (future) |
| `src/parser/state.rs` | 5 | Update parser to use `Atom` (future) |

---

## References

- Gemini analysis (January 2026)
- `benches/parser_bench.rs` - Parser throughput benchmarks
- `benches/scanner_bench.rs` - Scanner throughput benchmarks
- `src/interner.rs` - String interning implementation
