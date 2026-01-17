# Project Zang

Project Zang is a performance-first TypeScript compiler in Rust.[^1]
The goal is a correct, fast, drop-in replacement for `tsc`, with both native and WASM targets.

TypeScript is intentionally unsound. Zang keeps a sound core solver and layers a compatibility
engine on top to match TypeScript behavior while preserving correctness where possible.

## Project Direction

> This is a very high level project direction coming from project's manager's boss.

**Current Phase: Phase 8 - Conformance, Convergence, and Hardening**

### Priority number one is Conformance

#### HEADLINER

We need to keep working on our project and while maintaining the architectural integrity of our codebase, increase conformance with TypeScript.


Output of `wasm/differential-test/run-conformance.sh --max=10000` dictates where we are and where should we go from here


#### Current focus 

Based on the Conformance Report and the architectural constraints defined in `WASM_ARCHITECTURE.md`, here is a deep analysis of why conformance is low (23.4%) and where the fundamental architectural gaps lie.

##### Executive Summary: The "Permissive" Trap

The most alarming statistic is **Missing Errors: 68.2%**.
This means your compiler is **too permissive**. It accepts code that TypeScript rejects.
In a compiler, "Extra Errors" (35%) means you are buggy (parsing/binding issues). "Missing Errors" (68%) means you are **unsound**.

You are failing to catch:
1.  **Uninitialized Variables** (TS2454: 573 hits)
2.  **Uninitialized Properties** (TS2564: 443 hits)
3.  **Implicit Anys** (TS7006: 357 hits)

This suggests the architecture prioritizes *throughput* and *memory* (Data-Oriented Design) but lacks the **Control Flow Graph (CFG)** and **Inference strictness** required to match `tsc`.


1. Fundamental Issue: Data-Oriented Design vs. Control Flow Analysis (CFA)

**The Problem:**
You are using a `ThinNode` architecture (Struct-of-Arrays). This is excellent for parsing speed (500 MB/s), but it makes **Control Flow Analysis** (CFA) significantly harder.

TS2454 ("Variable used before assigned") and TS2564 require a **Control Flow Graph**. In a pointer-based AST (like TSC), you can attach "Flow Nodes" to AST nodes easily. In your `ThinNode` array, you cannot mutate nodes to add flow data.

**Evidence:**
*   `src/checker/control_flow.rs` exists but seems to rely on a `FlowNodeArena`.
*   The high missing count for TS2454 suggests that `check_identifier` in `thin_checker.rs` is **not** querying the Flow Graph effectively, or the Flow Graph construction is incomplete/disconnected from the linear parser pass.

**Architectural Fix:**
You need a dedicated **Side Table** for Flow Nodes that is computed *after* binding but *before* checking. The Checker must query `flow_graph[node_index]` for every identifier usage. Currently, it seems the checker defaults to "Assigned" if it can't prove otherwise. It must default to "Unassigned".

2. Fundamental Issue: The "Any" Fallback

**The Problem:**
The high number of missing **TS7006 (Implicit Any)** and **TS2322 (Type Not Assignable)** suggests that when your Solver encounters a complex type (generics, conditional types), it "bails out" and returns `Any` (or `true` for subtyping) to avoid crashing.

**Evidence:**
*   `src/solver/` uses a `TypeKey` system.
*   If `lower_type` fails to resolve a symbol (due to the TS2304 binding issues), it likely returns `Error` or `Any`.
*   In `specs/SOLVER.md`, the "Error Poisoning" rule states `Error` is compatible with everything.
*   Because you have 702 **TS2304 (Cannot find name)** errors, those unresolved names become `Any/Error`, which then silences all downstream errors (TS2322, TS7006).

**Architectural Fix:**
You cannot fix conformance until you fix **Binding (TS2304)**.
If the Binder cannot find `Array`, `Promise`, or `console`, the Solver treats them as `Any`.
1.  **Fix the Library Context:** Ensure `lib.d.ts` is actually loaded and bound in the test runner. The `WasmProgram` class seems to handle this, but the high TS2304 count implies global scope pollution is failing.
2.  **Strict Error Types:** Change the default bailout from `Any` to `Unknown`. `Unknown` is safe (errors on usage), whereas `Any` suppresses errors.

3. Fundamental Issue: The Parser is "Too Strict"

**The Problem:**
**TS1005 (Expected token)** and **TS1109 (Expression expected)** account for ~800 extra errors.
Your `ThinParser` is a recursive descent parser written from scratch. TypeScript allows many grammar ambiguities (ASI, loose keywords) that a strict Rust parser might reject.

**Impact:**
When parsing fails, the AST is incomplete. An incomplete AST leads to:
1.  Missing nodes -> Missing Symbols -> **TS2304 (Cannot find name)**.
2.  Missing Symbols -> Inferred as Any -> **Missing TS2322**.

**Architectural Fix:**
The parser needs a robust **Error Recovery** strategy.
*   Current: Seems to bail or produce error nodes that stop further analysis.
*   Required: "Resynchronization". If a statement is malformed, skip tokens until the next semicolon/brace and *continue parsing*. The AST must be as complete as possible even with syntax errors.

4. Fundamental Issue: The "Judge vs. Lawyer" Gap

**The Problem:**
Your `specs/SOLVER.md` describes a "Judge" (Sound Set Theory) and a "Lawyer" (Compat Layer).
The data shows the **Lawyer is missing**.

*   **TS2339 (Property does not exist):** 294 Extra Errors.
    *   This happens when you check `obj.prop`.
    *   A "Sound" solver checks if `prop` is in `obj`.
    *   TypeScript checks: `prop` in `obj` OR `obj` is `any` OR `obj` has string index signature OR `obj` is a Union and *one* constituent has it (sometimes).
*   **TS2322 (Type not assignable):** 310 Missing Errors.
    *   Your solver is likely returning `true` for things TS rejects (e.g., `string | number` assignable to `string`? No, but maybe your union logic is loose).

**Architectural Fix:**
The `CompatChecker` in `src/solver/` needs to implement the "Unsoundness Catalog" explicitly.
*   Implement **Apparent Members** for primitives (e.g., `string` has `.length`).
*   Implement **Union Widening** correctly.

##### Summary of Recommendations

1.  **Priority 1: Control Flow Analysis (TS2454/TS2564).**
    *   TS2454 (573 missing): Variable used before assigned - needs Flow Graph side-table
    *   TS2564 (443 missing): Property not initialized - extends CFA infrastructure
    *   Combined potential: +1,016 tests

2.  **Priority 2: Strict Solver Fallback (TS2322).**
    *   310 missing type assignability errors
    *   Change solver fallback from `Any` to `Unknown`/`Error`
    *   Will convert hidden "Missing Errors" into visible "Extra Errors"

3.  **Priority 3: Property Access Narrowing (TS2339).**
    *   292 extra errors (false positives) for property access
    *   Improve type narrowing and index signature handling

### Anti-Priorities (Do Not Work On)
*   **New Emitter transforms** (ES3, obscure module formats) - we have enough
*   New LSP features (Semantic Tokens, Code Actions) unless they expose a Solver bug
*   CLI argument parsing or fancy terminal output
*   Performance micro-optimizations (unless we regress significantly)


## Executive Summary
Last updated: 2026-01-11 (Director Loop 20)

### Conformance Metrics
| Metric | Current | Previous | Target | Status |
|--------|---------|----------|--------|--------|
| Exact Match | **30.8%** | 23.3% | 50%+ | +7.5pp |
| Missing Errors | **57.8%** | 68.2% | <30% | -10.4pp |
| Extra Errors | **28.9%** | 35.8% | <20% | -6.9pp |
| Parser Errors | **~85** | 1,122 | <100 | **TARGET MET** |
| Build | Passing | - | Green | OK |

### Completed Milestones
- **Parser Recovery**: 1,122 → 85 errors (92% reduction)
- **TS2304 Binding**: 759 → 10 false positives (98.7% reduction)
- **TS7006 Implicit Any**: 46 → 12 false positives (74% reduction)
- **TS2769 Overloads**: Complete
- **TS2339 Private Members**: 100% fixed

### Next Phase Priorities
1. **TS2454/TS2564 Control Flow** (1,016 missing) - Flow Graph side-table implementation
2. **TS2322 Solver Strictness** (310 missing) - Change fallback from Any to Unknown
3. **TS2339 Property Access** (292 extra) - Narrowing and index signature improvements

### Director Loop Status
**Recent Merges to rust:**
- ✅ squad/anvil (5 commits) - TS2322 false positive fixes, test improvements
- ✅ squad/forge (2 commits) - Worker wins merged

**Idle Workers (pending EM action):**
- Forge: W2 (32), W4 (6), W5 (35 commits ahead)
- Anvil: W1 (41), W2 (10), W3 (5), W4 (14 commits ahead)

**Active Workers:** Forge W1, W3; Anvil W5

## Status
This project is not ready for general use yet. The interface and distribution are in progress.

## Planned distribution
- `tsz` CLI (native binaries for major operating systems)
- Rust crate `tsz` (library + CLI)
- WASM bindings
- npm package `@tsz/tsz` (primary)
- compat package `@tsz/tsc` that exposes a `tsc` executable so tooling can swap without noticing
- Playground

## Testing Infrastructure

Project Zang has a comprehensive testing system to ensure conformance with TypeScript behavior while maintaining performance. The testing is organized into several levels:

### 1. Rust Unit Tests (`./wasm/test.sh`)

Core Rust compiler logic tests using Docker for consistent environments:

```bash
./wasm/test.sh                    # Run all Rust unit tests
./wasm/test.sh test_name          # Run specific test 
./wasm/test.sh --rebuild          # Force rebuild Docker image
./wasm/test.sh --clean            # Clean cached volumes
./wasm/test.sh --bench            # Run benchmarks
```

### 2. TypeScript Conformance Tests (`./wasm/differential-test/`)

The primary conformance system that tests against the full TypeScript test suite:

```bash
# Main conformance test runner
./wasm/differential-test/run-conformance.sh --max=10000  # Test up to 10K files
./wasm/differential-test/run-conformance.sh --all       # Test entire suite (~45K files)
./wasm/differential-test/run-conformance.sh --category=compiler  # Test specific category

# Analyze specific error types  
node wasm/differential-test/find-ts2454.mjs    # Find TS2454 "used before assigned" issues
node wasm/differential-test/find-ts2322.mjs    # Find TS2322 "not assignable" issues
node wasm/differential-test/find-ts2339.mjs    # Find TS2339 "property doesn't exist" issues
```

**Conformance Metrics** (updated in header):
- **Exact Match**: 30.8% (target: 50%+)  
- **Missing Errors**: 57.8% (target: <30%) - WASM too permissive
- **Extra Errors**: 28.9% (target: <20%) - WASM too strict
- **Parser Errors**: ~85 (target: <100) ✅

### 3. Individual Test Scripts (`./wasm/scripts/`)

Tools for debugging and development:

```bash
# Run single test with detailed output
node wasm/scripts/run-single-test.mjs tests/cases/compiler/2dArrays.ts --verbose

# Compare WASM output against TypeScript baselines  
node wasm/scripts/compare-baselines.mjs 100 compiler     # Test first 100 compiler tests
node wasm/scripts/compare-baselines.mjs --summary        # Show summary only

# Run batch of tests
node wasm/scripts/run-batch-tests.mjs

# Validate WASM module loads properly
node wasm/scripts/validate-wasm.mjs
```

### 4. Development Tests (`./wasm/dev-tests/`)

One-off test files for debugging specific issues:
- `test_debug.js` - General debugging
- `test_isolated.js` - Isolated test cases
- `test_promise_type.js` - Promise type checking
- Various `.ts` files for specific TypeScript features

### Test Organization Strategy

**Priority testing focuses on the highest-impact conformance gaps:**

1. **Control Flow Analysis (TS2454/TS2564)** - Variable/property initialization checking
2. **Type Assignability (TS2322)** - Core type checking logic  
3. **Property Access (TS2339)** - Object property resolution

Use `./wasm/differential-test/run-conformance.sh --max=1000` for quick iteration cycles, and `--all` for comprehensive validation before releases.

## Guiding principles
- Make tsz boringly correct before it is fast. Parity with `tsc` output, errors, and edge cases is the trust anchor.
- Measure everything: benchmark real repos, gate regressions, and only optimize hot paths that move real workloads.
- Determinism wins adoption: stable outputs, stable diagnostics, stable perf, and zero "sometimes" behavior.
- Incrementality is a feature, not a refactor: design caches and query boundaries up front so LSP/CLI stay snappy.
- UX matters as much as speed: error messages, source maps, and CLI flag compatibility are what teams feel every day.
- Keep the architecture clean and enforced. Performance-first is a habit, not a phase.


[^1]: Zang is Persian for rust.
