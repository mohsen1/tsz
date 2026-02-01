# Ignored Tests Tracker

**Total ignored: 578 tests across 40 files**
**Last updated: 2026-02-01**

## Summary by Component

| Component | Ignored | Files | Priority |
|-----------|---------|-------|----------|
| Checker   | 243     | 7     | High     |
| LSP       | 154     | 9     | Medium   |
| Emitter   | 112     | 7     | Medium   |
| Solver    | 37      | 7     | High     |
| CLI       | 24      | 3     | Low      |
| Parser    | 5       | 3     | Low      |
| Transforms| 2       | 1     | Low      |
| Other     | 1       | 1     | Low      |

## Detailed Breakdown

### Checker (243 tests)

| File | Count | Reason Categories |
|------|-------|-------------------|
| `src/tests/checker_state_tests.rs` | 221 | Generic TODO (majority), namespace-as-type detection, stack overflow in deep recursion, various unimplemented features |
| `src/checker/tests/control_flow_tests.rs` | 10 | Switch fallthrough narrowing, loop back-edge unions, closure START node antecedents (8 tests share same root cause) |
| `src/checker/tests/value_usage_tests.rs` | 4 | Bitwise operator error checking, enum arithmetic, TS18050 null/undefined property access |
| `src/checker/tests/ts2322_tests.rs` | 3 | Object literal property type mismatch detection |
| `src/checker/tests/ts2304_tests.rs` | 2 | Generic TODO |
| `src/checker/module_resolution.rs` | 1 | Generic TODO |
| `src/checker/tests/global_type_tests.rs` | 1 | Generic TODO |

**Quick wins:**
- [ ] `control_flow_tests.rs` closure tests (8 tests, single root cause in flow analysis)
- [ ] `value_usage_tests.rs` TS18050 checks (2 tests, straightforward diagnostic additions)
- [ ] `ts2322_tests.rs` object property mismatch (3 tests, same feature)

### LSP (154 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/lsp/tests/project_tests.rs` | 40 | TODO: Fix this test |
| `src/lsp/references.rs` | 34 | TODO: Fix this test |
| `src/lsp/tests/code_actions_tests.rs` | 21 | TODO: Fix this test |
| `src/lsp/hover.rs` | 17 | TODO: Fix this test |
| `src/lsp/definition.rs` | 14 | TODO: Fix this test |
| `src/lsp/highlighting.rs` | 11 | TODO: Fix this test |
| `src/lsp/tests/signature_help_tests.rs` | 8 | TODO: Fix this test |
| `src/lsp/rename.rs` | 8 | TODO: Fix this test |
| `src/lsp/tests/tests.rs` | 1 | TODO: Fix this test |

**Note:** All LSP tests have generic "TODO: Fix this test" reasons. Many may already pass — run `cargo test --lib lsp -- --ignored` to check.

### Emitter (112 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/tests/emitter_transform_integration_tests.rs` | 63 | ES5 IR transform incomplete |
| `src/tests/emitter_parity_tests_2.rs` | 21 | ES5 feature transforms (decorators, namespaces, private fields, static blocks) |
| `src/tests/emitter_parity_tests_1.rs` | 12 | ES5 spreads, decorators, private fields, rest params, async super |
| `src/tests/emitter_parity_tests_3.rs` | 7 | ES5 for-in/for-of, private generators, rest params |
| `src/tests/emitter_parity_tests_4.rs` | 4 | ES5 generic class, mixin private fields, satisfies, type guards |
| `src/tests/emitter_tests.rs` | 3 | ES5 destructured params and rest params in IR transform |
| `src/tests/source_map_tests_1.rs` | 2 | Source map tests |

**Common theme:** ES5 downlevel transform features — spreads, decorators, private fields, rest params, namespaces, static blocks.

### Solver (37 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/solver/tests/subtype_tests.rs` | 16 | Variance: contravariant params, invariant properties, template literals, optional/rest params, intersections |
| `src/solver/tests/integration_tests.rs` | 7 | Multi-file compilation, generic constraints |
| `src/solver/tests/operations_tests.rs` | 5 | Generic number index edge cases, tuple rest params, constraint dependencies |
| `src/solver/tests/infer_tests.rs` | 3 | `this` parameter bounds, union source inference, conditional type variance |
| `src/solver/tests/evaluate_tests.rs` | 2 | Generic TODO |
| `src/solver/tests/union_tests.rs` | 2 | Generic TODO |
| `src/solver/unsoundness_audit.rs` | 1 | Generic TODO |
| `src/solver/tests/type_predicate_tests.rs` | 1 | Generic TODO |

**Quick wins:**
- [ ] `operations_tests.rs` — "TODO: Fix this test" entries may just need assertion updates
- [ ] `evaluate_tests.rs` and `union_tests.rs` — generic TODOs worth re-checking

### CLI (24 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/cli/tests/tsc_compat_tests.rs` | 13 | TSC compatibility |
| `src/cli/tests/driver_tests.rs` | 10 | Compilation driver features |
| `src/cli/tests/config_tests.rs` | 1 | Config resolution |

### Parser (5 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/tests/p1_error_recovery_tests.rs` | 3 | Destructuring pattern error recovery |
| `src/tests/parser_state_tests.rs` | 1 | Generic TODO |
| `src/parser/tests/parser_improvement_tests.rs` | 1 | Parser hangs on malformed arrow function — needs investigation |

### Transforms (2 tests)

| File | Count | Reason |
|------|-------|--------|
| `src/transforms/tests/class_es5_tests.rs` | 2 | ES5 spread downleveling |

### Other (1 test)

| File | Count | Reason |
|------|-------|--------|
| `src/tests/parallel_tests.rs` | 1 | Generic TODO |

## How to Help

1. **Find tests that already pass:** `cargo test --lib <module> -- --ignored`
2. **Remove `#[ignore]`** from passing tests immediately
3. **Fix test expectations** if the feature works but assertions are outdated
4. **Implement missing features** for tests with specific ignore reasons
5. **Never add new `#[ignore]` tests** — see AGENTS.md Testing Requirements
