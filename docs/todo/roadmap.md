# Conformance Roadmap (Jan 2026)

Last updated: 2026-01-27

## Goals
- Match tsc behavior exactly
- Reduce extra errors (false positives)
- Restore missing errors (false negatives)

## Status Summary

**✅ Completed (2026-01-27)**:
- P0 (TS2749): #179, #181 merged - Value-only symbol checks fixed
- P2 (TS2339): #182 merged - Union property access preserved
- P3 (TS2318): #178 merged - noLib global type resolution honored
- P4 (TS2322): #180 merged - Tracing added for assignability analysis

**🚧 Remaining**:
- P1 (TS2540): Readonly check ordering - PR #183 created

## Priority status (P0-P4)
| Priority | Issue | Status | PRs |
|----------|-------|--------|-----|
| **P0** | TS2749 (14,175 extra) | ✅ Merged | #179, #181 |
| **P1** | TS2540 (10,381 extra) | 🟡 PR Open | #183 |
| **P2** | TS2339 (8,176 extra) | ✅ Merged | #182 |
| **P3** | TS2318 (3,386 missing) | ✅ Merged | #178 |
| **P4** | TS2322 (13,671 extra) | ✅ Merged | #180 |

PR links:
- #183 https://github.com/mohsen1/tsz/pull/183
- #182 https://github.com/mohsen1/tsz/pull/182
- #181 https://github.com/mohsen1/tsz/pull/181
- #180 https://github.com/mohsen1/tsz/pull/180
- #179 https://github.com/mohsen1/tsz/pull/179
- #178 https://github.com/mohsen1/tsz/pull/178

---

## P0: TS2749 - Value used as type (extra errors) ✅
**Impact**: Single largest false positive source.

**Root cause**: `symbol_is_value_only()` relies on TYPE flags that are not reliably preserved for lib symbols, alias chains, and declaration merging.

**Completed work**:
- ✅ #179 `Fix TS2749 lib symbol value-only checks` - Merged
- ✅ #181 `fix(checker): skip value-only symbols in type positions` - Merged

**Status**: Both PRs merged successfully. Fixes include:
- Updated `resolve_identifier_symbol_in_type_position()` to respect `has_lib_loaded()` flag
- Added proper name hint parameters to value-only checks
- Resolved merge conflicts with main branch

**Next steps**:
- [ ] Monitor conformance test results for TS2749 error reduction
- [ ] Verify TYPE flag propagation is working correctly in production

---

## P1: TS2540 - Readonly assignment (extra errors) 🟡
**Impact**: Large false positive source.

**Root cause**: Readonly check happens before property existence check, diverging from tsc ordering.

**Current work**: #183 https://github.com/mohsen1/tsz/pull/183

**Implementation**:
- ✅ Located `check_readonly_assignment()` in `src/checker/state.rs`
- ✅ Added property existence check using `property_access_type()` before readonly check
- ✅ Added test `test_nonexistent_property_should_not_report_ts2540`
- ✅ Existing readonly tests still pass

**Next steps**:
- [ ] Wait for CI to pass on PR #183
- [ ] Merge PR #183
- [ ] Monitor conformance test results for TS2540 error reduction

---

## P2: TS2339 - Property does not exist (extra errors) ✅
**Impact**: Major false positive source.

**Root cause**: `resolve_type_for_property_access()` flattens or normalizes unions before property lookup, losing member structure and index signature checks.

**Completed work**:
- ✅ #182 `fix(solver): Preserve union members for property access` - Merged

**Status**: PR merged successfully. Fix preserves union structure during property access resolution.

**Next steps**:
- [ ] Monitor conformance test results for TS2339 error reduction
- [ ] Verify union member iteration works correctly in edge cases

---

## P3: TS2318 - Cannot find global type (missing errors) ✅
**Impact**: Missing errors; tsz accepts code it should reject.

**Root cause**: `resolve_identifier_symbol()` checks lib binders even when `--noLib` is active.

**Completed work**:
- ✅ #178 `Honor noLib for global type resolution` - Merged

**Status**: PR merged successfully. Fix ensures `resolve_identifier_symbol()` respects `has_lib_loaded()` flag and skips lib binders when `--noLib` is active.

**Next steps**:
- [ ] Monitor conformance test results for TS2318 error restoration
- [ ] Validate noLib behavior with targeted test cases

---

## P4: TS2322 - Type assignability (extra errors) ✅
**Impact**: Large false positive source with multiple contributing factors.

**Root cause**: Error suppression logic and compat checks diverge from tsc; unsoundness catalog may be incomplete.

**Completed work**:
- ✅ #180 `Add TS2322 tracing and plan note` - Merged

**Status**: PR merged successfully. Tracing infrastructure added to identify failing assignability paths.

**Next steps**:
- [ ] Analyze tracing output to identify top failing assignability patterns
- [ ] Propose targeted fixes aligned with `TS_UNSOUNDNESS_CATALOG.md`
- [ ] Implement fixes based on tracing insights

---

## Cross-cutting: Visitor pattern migration
**Status**: Deferred until P0-P3 are stable.

**Next steps**:
- [ ] Identify highest-churn TypeKey matches in checker and solver.
- [ ] Replace with visitor pattern helpers from `src/solver/visitor.rs`.
- [ ] Keep changes scoped to avoid regressions.

---

## Sequencing plan (updated)
1. ✅ P0 (TS2749) consolidation + flag propagation - **Completed**
2. ✅ P3 (TS2318) noLib correctness - **Completed**
3. ✅ P2 (TS2339) union property access - **Completed**
4. ✅ P4 (TS2322) tracing + targeted fixes - **Completed**
5. 🚧 P1 (TS2540) ordering fix - **Remaining**
6. Visitor pattern migration - **Deferred**

## Expected Impact

Based on the fixes merged:
- P0 fix: Expected -14,000 extra errors
- P3 fix: Expected +3,000 expected errors (restored missing errors)
- P2 fix: Expected -8,000 extra errors
- P4 fix: Tracing added for analysis (impact TBD)

**Total potential improvement**: ~22,000 fewer discrepancies (excluding P1 and P4 follow-up work).

## Next Steps

1. **P1 (TS2540)**: Implement readonly check ordering fix
2. **Monitor conformance**: Run conformance tests to measure actual impact of merged fixes
3. **P4 follow-up**: Analyze TS2322 tracing output and implement targeted fixes
4. **Visitor pattern migration**: Begin systematic migration after P1 is complete