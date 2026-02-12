# 🎯 Path to 90% Conformance: Deep Analysis

> **Updated: February 2026**

## Executive Summary

| Current | Target | Gap | Effort Estimate |
|---------|--------|-----|-----------------|
| **~63%** (~7,900/12,583) | **90%** (~11,325/12,583) | ~3,400 tests | 15-30 weeks |

### Per-Slice Breakdown (Total: 12,583 tests)

| Slice | Pass Rate | Passing | Notes |
|-------|-----------|---------|-------|
| Slice 1 | **67.7%** | 2,125/3,139 | Latest run (Feb 12 2026) |
| Slice 2 | 57.9% → **69.5%** | 989/1,422* | *Partial re-run improved |
| Slice 3 | **56.3%** | 1,556/2,764 | Baseline documented |
| Slice 4 | **59.6%** | 789/1,324 | Baseline documented |

---

## 🔍 The Big Picture: Why ~63%?

The analysis reveals four fundamental categories of failures:

| Category | Tests | Root Cause | Fix Difficulty |
|----------|-------|------------|----------------|
| False Positives | 1,244 | Too strict / over-eager checking | Medium |
| Missing Errors | 1,737 | Features not implemented | High |
| Wrong Codes | 2,086 | Subtle differences in error choice | Medium |
| Close to Passing | 1,198 | Off by 1-2 error codes | Low-Medium |

---

## 📊 Top 10 False Positive Sources (Instant Wins)

These are tests where tsz emits errors but tsc doesn't — fixing these immediately passes tests:

| Error Code | Tests Affected | Root Cause | Fix Strategy |
|------------|---------------|------------|--------------|
| **TS2339** | 446 | Property access rejected too eagerly | Improve union narrowing, index signatures |
| **TS2322** | 406 | Type assignments rejected | Discriminated union assignability |
| **TS2345** | 362 | Argument types rejected | Conditional type inference |
| **TS1005** | 258 | Parser syntax strictness | Better error recovery |
| **TS2304** | 146 | Symbol resolution too strict | Scope/binding improvements |
| **TS1109** | 145 | Expression expected false positives | Parser recovery |
| **TS1128** | 142 | Enum/member declaration issues | Declaration validation |
| **TS2305** | 130 | Module export resolution | Import/export handling |
| **TS2344** | 127 | Conditional type checking | Type evaluation |
| **TS7006** | 107 | Implicit any warnings | noImplicitAny accuracy |

> **Potential Impact:** Fixing these 10 codes could pass **~2,200 tests** (57% of gap)

---

## 🔴 Top 10 NOT IMPLEMENTED Error Codes

These codes tsz never emits — implementing them is required for correctness:

| Error Code | Tests | Message | Feature Required |
|------------|-------|---------|-----------------|
| **TS2343** | 44 | Syntax `?` not allowed | Optional property syntax validation |
| **TS7026** | 33 | JSX element implicit any | JSX type checking |
| **TS2415** | 28 | Class method incompatible | Class inheritance checking |
| **TS2551** | 25 | Did you mean 'X'? | Spelling suggestions |
| **TS2538** | 25 | Cannot use `undefined` as index | Index access validation |
| **TS2493** | 23 | Tuple length mismatch | Tuple type checking |
| **TS1479** | 23 | `new` expression missing | Constructor validation |
| **TS1501** | 22 | Not a function | Function call validation |
| **TS2320** | 21 | Interface incompatible | Interface compatibility |
| **TS2630** | 20 | Cannot assign to `this` | This-type validation |

> **Potential Impact:** Top 10 alone affects **~280 tests**

---

## ⚡ Quick Wins: Single-Error Tests

Tests missing just **ONE** error code to pass:

| Fix This Code | Tests Would Pass |
|---------------|-----------------|
| TS2322 (partial fix) | **+103** |
| TS2339 (partial fix) | **+64** |
| TS2343 (implement) | **+42** |
| TS2304 (partial fix) | **+36** |
| TS2345 (partial fix) | **+33** |
| TS2411 (partial fix) | **+29** |
| TS2300 (partial fix) | **+20** |
| TS2320 (implement) | **+14** |
| TS2353 (partial fix) | **+14** |
| TS2415 (implement) | **+14** |

> **Total Quick Win Potential:** ~370 tests with focused fixes

---

## 🏗️ Major Feature Gaps

Based on test file analysis, these are the major architectural features missing or incomplete:

### 1. Discriminated Union Assignability (~200+ tests)

- **Current:** tsz rejects valid discriminated union assignments
- **Example:** `type S = { a: 0 | 2, b: 4 }; type T = { a: 0, b: 1|4 } | { a: 2, b: 3|4 }; s = t;` — tsz incorrectly rejects
- **Files:** `solver/subtype.rs`, `checker/type_computation_complex.rs`

### 2. Conditional Type Evaluation (~150+ tests)

- **Current:** Complex conditional types not fully evaluated
- **Example:** `T extends U ? X : Y` with nested generics
- **Files:** `solver/evaluate.rs`, `solver/evaluate_rules/conditional.rs`

### 3. Mapped Type Evaluation (~100+ tests)

- **Current:** Mapped types with keyof/indexed access incomplete
- **Example:** `{ [K in keyof T]: ... }` with constraints
- **Files:** `solver/evaluate_rules/mapped.rs`

### 4. Control Flow Analysis (~80+ tests)

- **Current:** CFA not fully integrated with type checking
- **Missing:** TS2454 (used before assigned), TS2564 (no initializer)
- **Files:** `binder/flow.rs`, `checker/flow_analysis.rs`

### 5. Template Literal Types (~50+ tests)

- **Current:** Template literal type operations incomplete
- **Example:** `` `prefix${T}suffix` `` type manipulation
- **Files:** `solver/evaluate_rules/template_literal.rs`

### 6. Parser Error Recovery (~258 tests)

- **Current:** Parser too strict, emits cascading errors
- **Missing:** Resynchronization after syntax errors
- **Files:** `parser/state.rs`, `parser/recovery.rs`

---

## 📈 Prioritized Roadmap to 90%

### Phase 1: False Positive Elimination (~63% → 72%)

**Timeline:** 4-6 weeks | **Tests:** +1,100

| Week | Focus | Expected Gain |
|------|-------|---------------|
| 1-2 | TS2339 property access false positives | +200 tests |
| 2-3 | TS2322 discriminated union assignability | +150 tests |
| 3-4 | TS2345 argument type inference | +150 tests |
| 4-6 | TS1005/TS1109 parser recovery | +100 tests |

### Phase 2: Missing Error Implementation (72% → 82%)

**Timeline:** 4-8 weeks | **Tests:** +1,200

| Week | Focus | Expected Gain |
|------|-------|---------------|
| 6-8 | Implement TS2343, TS2320, TS2415 | +100 tests |
| 8-10 | Conditional type evaluation improvements | +200 tests |
| 10-12 | Mapped type evaluation improvements | +150 tests |
| 12-14 | Control flow analysis integration | +150 tests |

### Phase 3: Edge Cases & Wrong Codes (82% → 90%)

**Timeline:** 6-12 weeks | **Tests:** +1,300

| Week | Focus | Expected Gain |
|------|-------|---------------|
| 14-16 | Template literal type support | +100 tests |
| 16-18 | Import/export resolution accuracy | +150 tests |
| 18-20 | Generic type parameter inference | +200 tests |
| 20-24 | Remaining error code implementations | +300 tests |
| 24-30 | Edge case fixes and refinements | +350 tests |

---

## 🎯 Highest ROI Actions (Do These First)

### 1. Fix Discriminated Union Assignability
- **Tests affected:** ~200+
- **Difficulty:** Medium
- **File:** `solver/subtype.rs`
- **Pattern:** `type S = { a: 0 | 2 }; type T = { a: 0 } | { a: 2 }; s = t;`

### 2. Improve Conditional Expression Type Checking
- **Tests affected:** ~150+
- **Difficulty:** Medium
- **File:** `checker/type_computation_complex.rs`
- **Pattern:** `cond ? expr1 : expr2` where branches have different but compatible types

### 3. Implement Parser Error Recovery
- **Tests affected:** ~250+
- **Difficulty:** Medium
- **File:** `parser/state.rs`
- **Pattern:** After syntax error, skip tokens and continue parsing

### 4. Fix Union Property Access
- **Tests affected:** ~100+
- **Difficulty:** Medium
- **File:** `solver/property.rs`
- **Pattern:** `obj.prop` where `obj` is union with common property

---

## 📋 Concrete Next Steps

1. **Week 1:** Run `./scripts/conformance.sh analyze --category false-positive --top 50` and fix the top 5 patterns
2. **Week 2:** Focus on TS2339 property access — identify why valid accesses are rejected
3. **Week 3:** Discriminated union assignability deep dive
4. **Week 4:** Parser error recovery investigation

> **Measurement:** Run full conformance suite weekly to track progress

---

## 📊 Summary Statistics

| Metric | Value |
|--------|-------|
| Total tests | 12,583 |
| Current passing | **~7,900 (~63%)** |
| Best slice (Slice 1) | 2,125/3,139 (67.7%) |
| Target passing | ~11,325 (90%) |
| Gap | ~3,400 tests |
| False positives | 1,244 (fixable) |
| Missing errors | 1,737 (implementable) |
| Close to passing | 1,198 (near-wins) |
| Not implemented codes | 662 (affects 2,255 tests) |
| Quick win potential | 1,380 single-error tests |
| Top FP (Slice 1) | TS2322:85, TS2345:81, TS2339:60 |

---

## Bottom Line

The path to 90% is clear but requires focused, sustained effort on:

1. **Reducing false positives** (immediate impact)
2. **Implementing missing error codes** (correctness)
3. **Improving type system features** (completeness)

The current ~63% is respectable for a new implementation — each percentage point requires significant compiler engineering work. Slice 1 at 67.7% shows recent improvements are working. Focus on bringing lagging slices (3, 4) up to match.
