# Session tsz-2

## Current Work

**Understanding conformance testing infrastructure and identifying improvement opportunities.**

### What I've Learned

**Conformance System Architecture:**
- Two-phase testing: (1) Generate TSC cache, (2) Run tsz and compare
- Cache file: `tsc-cache-full.json` (2.3MB, 12,399 entries)
- Test directory: `TypeScript/tests/cases/` (compiler, conformance, fourslash, etc.)
- Runner: Rust-based with tokio parallel execution (16 workers default)

**Current Test Results (50 tests sample):**
- Pass rate: 32% (16/50)
- Top error mismatches:
  - TS1005: missing=11 ("{0}" expected - Parser error recovery)
  - TS2695: missing=10 (Namespace no exported member - Module resolution)
  - TS1068: missing=1 (Continuation statement not within loop)
  - TS2307: missing=1 (Cannot find module)
  - TS2511: missing=1 (Cannot create instance of abstract class)

**Key Conformance Gaps (from docs/walkthrough/07-gaps-summary.md):**

False Positives (we report, TSC doesn't):
1. TS2322 (11,773x) - Type not assignable
2. TS2694 (3,104x) - Namespace no exported member
3. TS1005 (2,703x) - '{0}' expected
4. TS2304 (2,045x) - Cannot find name
5. TS2571 (1,681x) - Object is 'unknown'
6. TS2339 (1,520x) - Property doesn't exist
7. TS2300 (1,424x) - Duplicate identifier

False Negatives (TSC reports, we don't):
1. TS2318 (3,386x) - Cannot find global type
2. TS2307 (2,139x) - Cannot find module
3. TS2488 (1,749x) - Must have Symbol.iterator
4. TS2583 (706x) - Change target library?
5. TS18050 (680x) - Value cannot be used here

### Conformance Improvement Opportunities

Based on the gaps and error frequencies, here are the highest-impact areas:

**1. Parser Error Recovery (TS1005)**
- Impact: 2,703 false positives
- Files: `src/parser/`, especially `state.rs`
- Issue: Parser doesn't recover from syntax errors as well as TSC

**2. Module Resolution & Lib Loading (TS2307, TS2318, TS2694, TS2695)**
- Impact: ~8,000+ test failures
- Files: `src/binder/state.rs`, lib loading infrastructure
- Issue: Module resolution, namespace exports, lib.d.ts loading incomplete

**3. Symbol Resolution (TS2304)**
- Impact: 2,045 false positives
- Files: `src/binder/state.rs`, `src/checker/symbol_resolver.rs`
- Issue: Symbol lookup and resolution incomplete

**4. Subtype Checking (TS2322)**
- Impact: 11,773 false positives
- Files: `src/solver/subtype*.rs`
- Issue: Type assignability rules not matching TSC

**5. Object Types (TS2339, TS2571)**
- Impact: 3,201 false positives
- Files: `src/solver/`, `src/checker/type_checking.rs`
- Issue: Property access, type narrowing, `unknown` type handling

---

## History (Last 20)

*No work history yet*

---

## Punted Todos

*No punted items*
