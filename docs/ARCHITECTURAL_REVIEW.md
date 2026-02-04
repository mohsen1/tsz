# TSZ Architectural Review: Gap Analysis

**Date**: 2026-02-04
**Scope**: Full codebase (578K LOC across 415 Rust files)
**Conformance**: 39.4% type checking (5292/13443), 19.4% emit (1259/6500), 11.5% LSP (757/6563)

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [Critical Gaps](#3-critical-gaps)
4. [Solver Gaps](#4-solver-gaps)
5. [Checker Gaps](#5-checker-gaps)
6. [Parser & Binder Gaps](#6-parser--binder-gaps)
7. [Emitter & Transform Gaps](#7-emitter--transform-gaps)
8. [LSP Gaps](#8-lsp-gaps)
9. [Cross-Cutting Concerns](#9-cross-cutting-concerns)
10. [Sound Mode & Unsoundness Audit](#10-sound-mode--unsoundness-audit)
11. [Conformance Blockers (Ranked)](#11-conformance-blockers-ranked)
12. [Recommendations](#12-recommendations)

---

## 1. Executive Summary

TSZ is architecturally sound in its core design: the Solver-First principle, the Judge/Lawyer separation, arena-based allocation, and global type interning are well-chosen. The parser is ~98% complete, the binder ~92%, and the solver has comprehensive type representation with 30+ TypeKey variants.

However, six systemic issues prevent the project from reaching its goal of exact `tsc` parity:

| # | Issue | Impact | Severity |
|---|-------|--------|----------|
| 1 | **Any-poisoning via lib loading** | Masks thousands of real errors behind `Any` fallback | Critical |
| 2 | **Evaluate-before-cycle-check bug** | ~25% of conformance failures trace to this | Critical |
| 3 | **RefCell in Solver breaks parallelism** | Concurrent type checking will panic | Critical |
| 4 | **75+ TypeKey matches in Checker** | Architectural drift from Solver-First design | High |
| 5 | **CFA side-table not queried** | TS2454/TS2564 diagnostics cannot be emitted | High |
| 6 | **Import/export elision missing** | Declaration emit blocked (~5-10% conformance) | High |

The codebase has 25+ `eprintln!` debug statements in production paths and 747+ crash points (`panic!`/`unwrap()`/`expect()`) in non-test code within solver and checker.

---

## 2. Architecture Overview

```
                    ┌─────────────────────────────────────────────┐
                    │                  CLI Driver                  │
                    │          (203K LOC - config, args,           │
                    │       module resolution, incremental)        │
                    └────────────┬────────────────────────────────┘
                                 │
          ┌──────────┬───────────┼───────────┬──────────┐
          ▼          ▼           ▼           ▼          ▼
     ┌─────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
     │ Scanner │→│ Parser │→│ Binder │→│Checker │→│Emitter │
     │  3.5K   │ │  18K   │ │  6K    │ │  69K   │ │  8.6K  │
     └─────────┘ └────────┘ └────────┘ └───┬────┘ └────────┘
                                            │
                                     ┌──────┴──────┐
                                     │   Solver    │
                                     │   165K LOC  │
                                     │  (Lawyer →  │
                                     │   Judge →   │
                                     │  Interner)  │
                                     └─────────────┘
```

**Stated Goal**: Match `tsc` behavior exactly. Every error, inference, and edge case identical.

**Core Principles** (from NORTH_STAR.md):
1. Solver-First: All type computations go through Solver
2. Thin Wrappers: Checker is orchestration only, never logic
3. Visitor Patterns: Systematic traversal over ad-hoc matching
4. Arena Allocation: O(1) equality via interning

---

## 3. Critical Gaps

### 3.1 Any-Poisoning via Library Loading

**The single biggest conformance blocker.** When the binder fails to resolve standard globals (`console`, `Promise`, `Array`, `Map`, etc.), the solver defaults to `Any` to avoid crashing. `Any` then suppresses ALL downstream type errors through propagation.

**Root Cause**: TSZ loads core libs only (e.g., `es5.d.ts` instead of `lib.d.ts`). This was intentional for conformance testing but means:
- No DOM types by default
- No ScriptHost types
- Missing `console.log()` without explicit `@lib: dom`
- `/// <reference lib="..." />` chain-following differs from tsc

**Impact**: High conformance metrics hide true errors. The "Missing Errors" column in conformance is largely driven by Any-poisoning. A file that should emit 10 errors may emit 0 because the first unresolved symbol poisons everything.

**Files**: `src/lib_loader.rs`, `src/embedded_libs.rs`

### 3.2 Evaluate-Before-Cycle-Check Bug

**From the project's own SOLVER_REFACTORING_PROPOSAL.md (Section 2.1):**

> "Estimated ~25% of conformance failures trace back to this bug."

When the evaluator encounters a recursive type, it currently evaluates the type body BEFORE checking if it's in a cycle. This causes:
- Infinite loops caught only by depth limits
- Incorrect results when limits are hit (returns input type unchanged)
- Type `Any` fallback instead of proper circular type representation

**Impact**: 30+ hardcoded limits in `src/limits.rs` serve as band-aids for this fundamental issue. These limits cause incorrect behavior when hit (returning `false` instead of `Provisional` for subtype checks).

**Files**: `src/solver/evaluate.rs`, `src/limits.rs`

### 3.3 RefCell Throughout Solver Breaks Parallelism

The codebase claims "true parallel type checking" and uses `DashMap` in the TypeInterner for thread-safety. But the Solver itself uses `RefCell` extensively:

| File | Usage |
|------|-------|
| `src/solver/application.rs:55-70` | `RefCell<depth, visiting, cache>` |
| `src/solver/judge.rs:357-370` | `RefCell<subtype_cache, eval_cache>` |
| `src/solver/lower.rs:40-44` | `RefCell<type_param_scopes, operations>` |
| `src/solver/inheritance.rs:40-55` | `RefCell<nodes, max_symbol_id>` |
| `src/solver/db.rs:900` | `Rc<RefCell<TypeEnvironment>>` |

`RefCell` panics on concurrent access. If two threads attempt type checking simultaneously, the runtime will panic. This contradicts the Salsa-based architecture described in NORTH_STAR.md and blocks the project's stated performance goals.

### 3.4 TypeKey Matching in Checker (75+ Violations)

NORTH_STAR.md Rule #3: "Checker NEVER inspects type internals."

Reality: 75+ instances where Checker matches on `TypeKey` variants directly, creating tight coupling between Checker and Solver internals. This prevents:
- Independent Solver refactoring
- Salsa migration (types must be opaque queries)
- Sound Mode toggle (Checker hard-codes unsound assumptions)

**Example violation pattern** (from `src/checker/assignability_checker.rs`):
```rust
match type_key {
    TypeKey::Ref(symbol_ref) => { ... }
    TypeKey::Union(members) => { ... }
    // 200+ lines of manual traversal that should be in Solver
}
```

### 3.5 CFA Side-Table Exists But Is Not Queried

The binder constructs a control flow graph (CFG) and stores it in a side-table. The checker has `flow_analysis.rs` (1,511 LOC) and `flow_narrowing.rs`. However:

- TS2454 ("Variable used before being assigned") is largely unimplemented
- TS2564 ("Property has no initializer") is only partially working
- Definite assignment analysis is incomplete
- Unreachable code detection (flags set but not used)

**Root Cause**: The checker doesn't fully query the CFG side-table. Unlike tsc which mutates AST nodes to add flow flags, TSZ uses a side-table approach but hasn't completed the integration.

**Files**: `src/checker/flow_analysis.rs`, `src/checker/control_flow.rs`, `src/checker/control_flow_narrowing.rs`

### 3.6 Import/Export Elision Not Implemented

Declaration emit (.d.ts) outputs ALL imports, even unused ones, causing "Module not found" errors. An attempt to implement a UsageAnalyzer (tsz-5 session) was abandoned after Gemini review found critical architectural flaws:

- Pure AST walk misses inferred types (return types not annotated but inferred)
- Qualified name handling incomplete (`MyModule.SomeType`)
- Missing handlers for TypeQuery, IndexedAccessType, MappedType, ConditionalType
- Confused boundary between AST walking and type system walking

**Impact**: Blocks ~5-10% conformance improvement. Current declaration emit: 41.9% (267/637 tests).

---

## 4. Solver Gaps

### 4.1 Type Representation (types.rs)

The solver defines 27 TypeKey variants covering most of TypeScript's type system. Notable gaps:

| Feature | Status | Notes |
|---------|--------|-------|
| Nominal class checking | Partial | Visibility enum added but not fully populated in 6 class_type.rs sites |
| Application type expansion | TODO | `Application(Ref(sym), args)` not expanded to instantiated form |
| Lazy type resolution | Dual system | Phase 3.2/3.4 Ref→Lazy migration incomplete; both systems active |
| DefId identity system | Incomplete | Still uses SymbolRef in TypeQuery and many other places |

### 4.2 Subtyping (subtype.rs)

Core algorithm is well-implemented with coinductive cycle detection and three-layer depth protection. Gaps:

- **Nominal class checking** requires InheritanceGraph and symbol callbacks not always available
- **Generic variance checking** limited to basic constraint checking (no full variance inference)
- **Intersection distribution** has O(N^2) complexity for large intersections
- **Open numeric enums** intentionally unsound (matches tsc) but may need Sound Mode override

### 4.3 Type Evaluation (evaluate.rs)

Critical TODO documented at lines 200-233:

```
Application Type Expansion (Worker 2 - Redux test fix)

Problem: Application(Ref(sym), args) types like Reducer<S, A> are not
being expanded to their instantiated form. Shows as "Ref(5)<error>"
in diagnostics instead of actual type.
```

Additional evaluation gaps:
- Mapped type evaluation needs more comprehensive testing
- Template literal expansion limited to 100K cardinality (hardcoded)
- Conditional type distributive behavior may have edge cases in nested conditionals

### 4.4 Type Narrowing (narrowing.rs)

Supported operations: typeof, instanceof, literal equality, nullish, truthiness, discriminants, property presence (`in`).

Gaps:
- **Type guard functions** (user-defined `is` predicates): Not implemented
- **Assertion narrowing** (assert functions): Not implemented
- **Discriminant narrowing has 3 known bugs** (reversed subtype check, missing Lazy/Ref/Intersection resolution, broken for optional properties)
- **Debug logging in production**: 3 `eprintln!` calls at lines 289, 382, 496

### 4.5 Type Inference (infer.rs)

Union-Find based with constraint collection and BCT algorithm. Gaps:

- `collect_constraint()` at line 961 is a placeholder
- BCT common base class search is O(N*D) where D = class depth
- Constraint deduplication is O(N^2)
- Circular dependency expansion may have edge cases

### 4.6 Freshness Tracking

**Tracked by TypeId instead of syntactic+binding position.** This causes "Zombie Freshness" where:
- Object literals with the same structural shape incorrectly share freshness state
- Freshness persists through variable bindings where it shouldn't (or vice versa)
- Sound Mode's "sticky freshness" feature (TS9001) cannot work correctly on this foundation

Per the project's own docs, this should be tracked by AST expression identity, not TypeId.

### 4.7 Debug Logging in Production Code

25+ `eprintln!`/`println!` calls in solver production code (not behind `#[cfg(debug_assertions)]` or tracing):

| File | Count | Lines |
|------|-------|-------|
| `narrowing.rs` | 3 | 289, 382, 496 |
| `evaluate_rules/conditional.rs` | 5 | various |
| `evaluate_rules/infer_pattern.rs` | 2 | various |
| `evaluate_rules/template_literal.rs` | 4 | various |
| `intern.rs` | 2 | 1687, 1773 |
| `tracer.rs` | 1 | 935 (println!) |

---

## 5. Checker Gaps

### 5.1 Incomplete Module Split

The architectural goal is for Checker to be a "thin orchestration layer." In practice:
- `state.rs` is 12,974 lines
- `type_checking.rs` is 9,556 lines
- `type_computation.rs` is 3,189 lines

Most logic from `declarations.rs` and `statements.rs` remains in `CheckerState` with stub delegations.

### 5.2 Expression Checking (expr.rs)

- JSX elements delegate with no outline of needed checks
- Conditional expression narrowing in branches not outlined
- Optional chaining not explicitly handled in expression checker
- Cell-based depth tracking used as workaround for immutable context

### 5.3 Declaration Checking (declarations.rs)

Most methods are stubs that delegate to CheckerState:
- `check_variable_statement()` - TODO
- `check_function_declaration()` - TODO
- `check_interface_declaration()` - TODO
- `check_type_alias_declaration()` - TODO
- `check_enum_declaration()` - TODO

Missing checks:
- TS2564 property initialization uses `HashSet` instead of `FxHashSet`
- Initialization via getter/setter properties not checked
- Cross-file module resolution verification missing
- Namespace merging validation incomplete

### 5.4 Statement Checking (statements.rs)

- `for-await-of` async context validation deferred (check exists but validation unclear)
- Labeled statements with break/continue not explicitly handled
- Unreachable code after return/throw not detected
- Function overload matching validation missing
- Break/continue label validation missing

### 5.5 Class Checking (class_checker.rs)

- Accessor compatibility not checked (getter vs property, setter vs property)
- Variance checking for generics in base class properties missing
- Transitive interface extension checking missing (A extends B extends C)
- Method signature compatibility in implements clause not validated
- Readonly property assignment in constructor not checked

### 5.6 Class Inheritance (class_inheritance.rs)

- Code duplication between `resolve_qualified_symbol()` and `resolve_heritage_symbol_access()`
- Interface inheritance cycles not detected
- Type alias circular references not detected
- Generic constraint cycles not detected
- Cycle error messages don't show the full path

### 5.7 Generic Checking (generic_checker.rs)

- Type argument bound checking missing
- Default type argument handling incomplete
- Type parameter variance checking (covariant/contravariant) not implemented
- Generic constraint cycle detection missing
- Higher-order generics (generics of generics) not validated

### 5.8 Flow Analysis (flow_analysis.rs)

Critical issues:
- 6+ `eprintln!` debug statements in production control flow paths (lines 287, 304, 635-693)
- `is_node_within()` function called but may not be defined in expected scope
- No exhaust checking after switch on discriminated unions
- No unreachable code detection
- No type guard function detection
- `is_captured_variable()` walks scope chain up to 10,000 iterations (MAX_TREE_WALK_ITERATIONS)
- Diagnostics created directly instead of through error reporting infrastructure

### 5.9 Missing TypeScript Error Codes

Key diagnostic codes that are defined but not fully emitted:
- **TS2454**: Variable used before assignment (CFA integration missing)
- **TS2564**: Property has no initializer (partially working)
- **TS7006**: Implicit any (silently defaulting instead of erroring)
- **TS2694**: Resolved import with missing members (TODO in import_checker.rs)

---

## 6. Parser & Binder Gaps

### 6.1 Parser (98% Complete)

The parser is the most complete component with 308+ syntax kinds and support for all modern TypeScript features including satisfies, using/await using, private fields, static blocks, and decorators.

Remaining gaps:
- **Error recovery too strict**: ~400+ false positive TS1005/TS1109 errors where tsc would recover
- **Tagged template literal error recovery**: Incomplete template literals don't produce valid TAGGED_TEMPLATE_EXPRESSION nodes
- **Using declarations**: Correctly errors on destructuring patterns (TS1375) but this restriction should be validated against latest tsc behavior

### 6.2 Binder (92% Complete)

Well-implemented symbol table with persistent scopes and control flow graph construction. Gaps:

- **Wildcard ambient module pattern matching**: `declare module "*.json"` patterns are stored but NOT matched during module resolution. Confirmed in test at `src/tests/checker_state_tests.rs:3264`
- **Decorator binding**: Decorators are parsed but not bound/validated. The binder doesn't track decorator applications for later transformation or type checking
- **Constructor overload binding**: Multiple constructor signatures are parsed but binding and merging needs verification

---

## 7. Emitter & Transform Gaps

### 7.1 JavaScript Emit (75% Complete for ES5)

All 9 ES targets (ES3-ESNext) and 10 module systems are supported. 28 runtime helpers are fully implemented. Most ES5 transforms are complete.

Gaps:
- **ES5 decorator lowering**: Standalone decorators skipped with warning in ES5 mode (class-level coordination exists but standalone emission incomplete)
- **Super argument passing edge cases**: TODO at `src/transforms/es5.rs:247` - "pass super args"
- **Namespace transform error handling**: 12 panic sites with "Expected NamespaceIIFE IR node" in `namespace_es5_ir.rs`

### 7.2 Declaration Emit (41.9% Conformance)

Type printer (`src/emitter/type_printer.rs`) has significant gaps:

| Type Feature | Fallback | Impact |
|-------------|----------|--------|
| Callable types (overloaded signatures) | Emits `Function` | High |
| Lazy type resolution | Emits `any` | High |
| Enum member types | Emits `any` | Medium |
| Conditional types | Emits `any` | Medium |
| Template literal types | Emits `string` | Medium |
| Mapped types | Emits `any` | Medium |
| Index access types | Emits `any` | Medium |
| String intrinsics (Uppercase, etc.) | Emits `any` | Low |

### 7.3 Source Maps

Fully implemented with `SourceWriter` tracking line/column positions. Both `.js.map` and `.d.ts.map` generation supported.

---

## 8. LSP Gaps

### 8.1 Current State (11.5% Conformance)

20 LSP features are architecturally implemented. The core engines for completions, hover, and references are fully built. The primary gap is wiring.

### 8.2 Wiring Gaps (Quick Wins Available)

| Feature | Engine Status | LSP Wired? | Effort |
|---------|-------------|------------|--------|
| Completions | Fully implemented with 3 strategies | Only returns keywords | 2-4 hours |
| Hover | Infrastructure complete with JSDoc | Not wired to TypeInterner | 1-2 days |
| Inlay Hints | Clear implementation path | Not started | 6-10 hours |
| Workspace Symbols | SymbolIndex exists | Not activated | 1-2 days |

### 8.3 Cross-File Navigation

Go to Definition: 0/175 tests passing. Find References works within single files only. Cross-file operations use O(N) full-file scans (500-2000ms for 10K files) despite SymbolIndex infrastructure existing that could provide O(1) lookups.

### 8.4 Type System Accuracy

The LSP returns `TypeId::ANY` or `Unknown` for complex types. Hover shows declared type instead of narrowed type because control flow narrowing API isn't integrated. This makes the LSP fundamentally less useful even where features are wired up.

### 8.5 Performance

- Type cache completely invalidated on every edit (should be incremental)
- Scope chains rebuilt from scratch on every query
- No result caching between requests
- Potential 100-1000x speedup available via SymbolIndex activation

---

## 9. Cross-Cutting Concerns

### 9.1 Crash Points in Production

747+ unrecovered error paths in solver and checker:
- ~497 `panic!` calls (many in non-test code)
- ~200 `unwrap()` calls on operations that can fail
- ~50 `expect()` calls on arena operations

These will crash production on edge cases. Key locations:
- `src/solver/judge.rs:1166, 1213, 1245, 1251` - 4 direct panics assuming type structure correctness
- `src/solver/binary_ops.rs:320, 328` - Panics on unexpected type patterns
- `src/checker/control_flow.rs:2168` - Panic in flow graph building
- `src/checker/statements.rs:129-336` - Multiple `unwrap()` on node access
- `src/checker/flow_graph_builder.rs:991, 1053` - `unwrap()` on graph operations

### 9.2 Unsafe Code

One instance of `unsafe` at `src/solver/compat.rs:179-184`:
```rust
// Lifetime transmute - claims safety assumes CheckerContext outlives CompatChecker
```
Use-after-free risk if the lifetime assumption is ever violated.

### 9.3 Memory Patterns

- 169 `Vec::new()` without capacity hints in hot paths (allocation thrashing)
- Template literal expansion limited to 100K but still risky for pathological inputs
- Union simplification is O(N^2) for large unions
- No evidence of memory profiling or benchmarking on real-world projects

### 9.4 Incremental Compilation

Basic structure present (`BuildInfo`, `FileInfo`, `EmitSignature`) with cache version "0.1.0" (experimental). Missing:
- Sophisticated dependency invalidation
- Project reference support
- Proper cache versioning/migration

### 9.5 WASM API

23 public types exposed via `wasm-bindgen`. Missing:
- Type checking integration (blocks full test suite for declaration emit)
- Project references
- Incremental API
- Profiling hooks
- Custom transform support

---

## 10. Sound Mode & Unsoundness Audit

### 10.1 Audit Claims vs Reality

`src/solver/unsoundness_audit.rs` claims all 44 unsoundness rules are `FullyImplemented`. However, the project's own `SOLVER_REFACTORING_PROPOSAL.md` documents architectural gaps that make several of these incomplete:

**Rules with architectural gaps despite "FullyImplemented" status:**
- Rule #4 (Freshness): Tracked by TypeId instead of syntactic position (Zombie Freshness)
- Rule #21 (Intersection Reduction): Only reduces some impossible intersections
- Rule #7 (Open Numeric Enums): Sound Mode override not wired

### 10.2 Sound Mode Status

SOUND_MODE_ASPIRATIONS.md lists 10 categories with TS9001-TS9008 diagnostic codes. The diagnostics are defined in code, but enforcement is incomplete:

| Sound Mode Feature | Diagnostic | Defined | Enforced |
|-------------------|-----------|---------|----------|
| Sticky freshness | TS9001 | Yes | No (wrong tracking model) |
| Mutable array covariance | TS9002 | Yes | Partial |
| Method bivariance | TS9003 | Yes | Partial |
| `any` escape detection | TS9004 | Yes | Partial |
| Enum-number assignment | TS9005 | Yes | Partial |
| Missing index signature | TS9006 | Yes | Partial |
| Unsafe type assertion | TS9007 | Yes | Partial |
| Unchecked indexed access | TS9008 | Yes | Partial |

### 10.3 Judge/Lawyer Architecture

**Partially implemented.** Key decision: QueryCache is the "final architecture" (Salsa deferred per Appendix F).

Consequences of deferring Salsa:
- Coinductive cycles NOT properly handled
- Evaluate-before-cycle-check bug NOT fixed
- Production memoization incomplete
- Thread-safety not achievable with RefCell

---

## 11. Conformance Blockers (Ranked)

Ranked by estimated impact on conformance test pass rate:

| Rank | Blocker | Current | Est. Impact | Effort |
|------|---------|---------|-------------|--------|
| 1 | **Any-poisoning from lib loading** | Masks errors | +10-15% | Medium |
| 2 | **Evaluate-before-cycle-check** | ~25% failures from this | +8-12% | Hard |
| 3 | **Parser error recovery** | ~400 false positives | +5-8% | Medium |
| 4 | **Import/export elision** | Blocks decl emit | +5-10% (decl) | Medium |
| 5 | **CFA integration** (TS2454/TS2564) | Not emitting | +3-5% | Medium |
| 6 | **Discriminant narrowing bugs** | 3 critical bugs | +2-4% | Easy |
| 7 | **Nominal class subtyping** | 50% complete | +2-3% | Medium |
| 8 | **Type guard functions** | Not implemented | +1-3% | Medium |
| 9 | **Application type expansion** | TODO in evaluate.rs | +1-2% | Easy |
| 10 | **Type printer completeness** | Many types → `any` | +2-5% (decl) | Medium |

**Estimated ceiling if all fixed**: ~65-75% type checking conformance (up from 39.4%)

---

## 12. Recommendations

### 12.1 Immediate (Week 1-2)

1. **Remove all `eprintln!`/`println!` from production paths** - Convert to `tracing::debug!` or remove entirely. 25+ instances causing noise and performance impact.

2. **Fix discriminant narrowing bugs** (3 known issues in `narrowing.rs`) - Easy wins with clear diagnosis already available from Gemini review.

3. **Wire LSP completions to server** - 2-4 hours for massive user-visible improvement. Core engine already fully implemented.

4. **Implement Application type expansion** - TODO clearly documented in `evaluate.rs:200-233`. Blocks Redux-style test cases.

### 12.2 Short-Term (Month 1)

5. **Fix lib loading to match tsc** - Load `lib.d.ts` (full) instead of `es5.d.ts` (core only). Follow `/// <reference lib="..." />` chains. This alone could improve conformance 10-15% by eliminating Any-poisoning.

6. **Complete CFA diagnostic integration** - Wire flow_analysis.rs side-table queries to emit TS2454/TS2564. Infrastructure exists; needs connection.

7. **Improve parser error recovery** - Reduce ~400 false positive TS1005/TS1109 errors. Add resynchronization for common syntax patterns.

8. **Complete nominal subtyping** - Populate visibility in remaining 6 `class_type.rs` sites. Unblocks significant class-related conformance.

### 12.3 Medium-Term (Month 2-3)

9. **Address evaluate-before-cycle-check** - The most impactful single algorithmic fix. Consider partial Salsa integration or QueryCache-based cycle recovery.

10. **Replace RefCell with thread-safe alternatives** - `RwLock`, `Mutex`, or restructure to per-thread Solver instances. Required for stated parallelism goals.

11. **Implement import/export elision** - Using hybrid AST+type-system walk per Gemini architectural guidance.

12. **Implement type guard functions** - User-defined `is` predicates and assertion functions.

### 12.4 Long-Term (Month 3+)

13. **Eliminate TypeKey matches in Checker** - Systematic refactoring of 75+ violations. Move all type inspection to Solver queries.

14. **Complete Sound Mode enforcement** - Fix freshness tracking model (syntactic instead of TypeId), then wire TS9001-TS9008 enforcement.

15. **Activate LSP SymbolIndex** - 100-1000x performance improvement for cross-file operations. Infrastructure already exists.

16. **Replace crash points with Result types** - Systematic audit of 747+ panic/unwrap/expect sites. Start with hot paths in subtype checking and evaluation.

---

## Appendix A: File Size Hotspots

Files exceeding the 3,000-line anti-pattern threshold (per NORTH_STAR.md):

| File | Lines | Notes |
|------|-------|-------|
| `src/checker/state.rs` | 12,974 | Main orchestration - needs further decomposition |
| `src/checker/type_checking.rs` | 9,556 | Type validation - candidate for splitting |
| `src/cli/driver.rs` | 101,000 | Compiler driver - monolithic |
| `src/cli/driver_resolution.rs` | 59,600 | Module resolution - monolithic |
| `src/cli/config.rs` | 42,100 | Config parsing - large but stable |
| `src/embedded_libs.rs` | 38,300 | Generated - acceptable |
| `src/cli/args.rs` | 34,500 | Argument parsing - large but stable |
| `src/diagnostics.rs` | 22,300 | Error codes - generated/stable |

## Appendix B: Test Infrastructure Status

| Test Suite | Passing | Total | Rate |
|-----------|---------|-------|------|
| Type checking conformance | 5,292 | 13,443 | 39.4% |
| JavaScript emit | 1,259 | 6,500 | 19.4% |
| Declaration emit | 267 | 637 | 41.9% |
| LSP fourslash | 757 | 6,563 | 11.5% |
| Solver unit tests | ~90%+ | ~40 files | Good |
| Checker unit tests | ~85%+ | ~13 files | Good |
| Ignored tests (all suites) | 60+ | - | Technical debt |

## Appendix C: Dual System Technical Debt

The Ref→Lazy migration (Phase 3.2/3.4) left two parallel type identity systems active:

- **SymbolRef** (`Ref(SymbolRef)`) - Original system, used in TypeQuery and many Checker paths
- **DefId** (`Lazy(DefId)`) - New system, used in Solver's evaluate/instantiate paths

Both systems are active simultaneously, requiring dual resolution paths and creating confusion about which to use in new code. Completing this migration would reduce cognitive load and simplify the codebase.

## Appendix D: Session Coordination Status

| Session | Focus | Status |
|---------|-------|--------|
| tsz-1 | Solver Infrastructure, Nominal Subtyping | Active |
| tsz-2 | Advanced Type Evaluation | Active (50% on Task 2) |
| tsz-3 | CFA & Loop Narrowing | Complete |
| tsz-4 | Declaration Emit | Complete |
| tsz-5 | Import/Export Elision | Blocked (architectural redesign needed) |
| tsz-6 | Advanced Type Nodes | Complete |
