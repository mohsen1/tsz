# TSZ Codebase TODO Tracker

**Last Updated**: February 2026
**Source**: Systematic `TODO`/`FIXME`/`HACK` grep across all crates

---

## Summary

| Crate | Production TODOs | Ignored Tests | Total |
|-------|-----------------|---------------|-------|
| tsz-solver | 7 | 15 | 22 |
| tsz-checker | 5 | 12 | 17 |
| tsz-emitter | 1 | 0 | 1 |
| tsz-lsp | 4 | 9 | 13 |
| tsz-cli | 3 | 7 | 10 |
| tsz-wasm | 1 | 0 | 1 |
| Integration tests (src/) | 0 | 32 | 32 |
| **Total** | **21** | **75** | **96** |

---

## Solver (`tsz-solver`)

### High Priority — Feature Gaps

These block conformance progress and affect type correctness.

| File | Description | Impact |
|------|-------------|--------|
| `evaluate_rules/mapped.rs:145` | Array/Tuple preservation for homomorphic mapped types | Mapped types over arrays/tuples produce wrong shapes |
| `db.rs:1554` | Look up symbol extends clause for class hierarchy | Class hierarchy subtyping incomplete |

### Medium Priority — Correctness Improvements

| File | Description |
|------|-------------|
| `contextual.rs:176` | Check if `def_id` points to `lib.d.ts` `ThisType` via symbol resolution |
| `contextual.rs:252` | Blindly picks the first `ThisType` — needs proper selection |
| `narrowing.rs:824` | Check for static `[Symbol.hasInstance]` method overriding standard narrowing |
| `tracer.rs:342` | Return apparent shapes for primitives (`String`, `Number`, etc.) |

### Completed Items
- ~~`narrowing.rs:394`~~ Pass resolver to `PropertyAccessEvaluator` — **Done**
- ~~`db.rs:701`~~ Apply flags to `CompatChecker` — **Done**
- ~~`lib.rs:120`~~ Fix API mismatches in `mapped_key_remap_tests` — **Done**
- ~~`evaluate.rs:216`~~ Application type expansion — **Already implemented**, stale TODO removed

### Ignored Tests (15)

| File | Reason |
|------|--------|
| `evaluate_tests.rs` (2) | Tests that need fixing |
| `evaluate_tests.rs` | Constraint checking not implemented |
| `evaluate_tests.rs` | `Parameters` extraction for mixed optional/rest params |
| `evaluate_tests.rs` | `ConstructorParameters` extraction not fully implemented |
| `evaluate_tests.rs` | Tuple spread inference not fully implemented |
| `evaluate_tests.rs` | Leading rest patterns not fully implemented |
| `evaluate_tests.rs` | Unique symbol as subtype of symbol |
| `evaluate_tests.rs` | Structural subtyping verification |
| `evaluate_tests.rs` | Overload signature extraction for infer patterns |
| `infer_tests.rs` | Const BCT should produce `"a" \| "b"` union |
| `instantiate_tests.rs` | Mapped type instantiation with shadowed type parameter |
| `instantiate_tests.rs` | Template literal in mapped type instantiation |
| `typescript_quirks_tests.rs` | `disable_method_bivariance` doesn't fully prevent |
| `typescript_quirks_tests.rs` | Nested callback contravariance not fully supported |

---

## Checker (`tsz-checker`)

### Medium Priority — Improvements

| File | Description |
|------|-------------|
| `iterable_checker.rs:124` | Check call signatures for generators when `CallableShape` is implemented |
| `state_checking_members.rs:1817` | Investigate lib loading for Promise detection |
| `function_type.rs:335` | Investigate lib loading for Promise detection |
| `type_node.rs:61` | Add cache key based on type param hash for smarter caching |
| `control_flow_narrowing.rs:304` | Heuristic picks first predicate signature — needs improvement |

### Completed Items
- ~~`type_checking_utilities.rs:2247`~~ Implement `get_symbol_by_name` — **Done** (in state.rs)
- ~~`type_checking_utilities.rs:1760`~~ Evaluate constant expression — **Done**
- ~~`state.rs:757`~~ Add more node types for `clear_type_cache_recursive` — **Done**
- ~~`scope_finder.rs:14`~~ Remove dead code — **Done** (removed unused methods)
- ~~`import_checker.rs:501`~~ Check if right member exists (TS2694) — **Done** (via `report_type_query_missing_member`)

### Ignored Tests (12)

| File | Reason |
|------|--------|
| `generic_tests.rs` (7) | Generic constraint checking not yet implemented |
| `private_brands.rs` | Object literal extra property check with private class fields |
| `freshness_stripping_tests.rs` | Investigate destructuring from literals |
| `enum_nominality_tests.rs` | Enum member nominal typing |
| `control_flow_tests.rs` | `instanceof` narrowing with union types |
| `constructor_accessibility.rs` | Private constructor access inside class body |

---

## Emitter (`tsz-emitter`)

### Remaining TODOs

| File | Description |
|------|-------------|
| `declaration_emitter/tests/` (10) | All usage analyzer tests need `CheckerContext` initialization |

### Completed Items
- ~~`type_printer.rs`~~ All 7 type printing TODOs — **Done** (conditional, mapped, index access, callable, enum, string intrinsic, string/bigint literal)
- ~~`declaration_emitter/mod.rs:3614`~~ Store symbol alias — **Done**
- ~~`declaration_emitter/mod.rs:3752`~~ Check if symbol is declared in current file — **Done**
- ~~`declaration_emitter/mod.rs:3842`~~ Implement proper relative path calculation — **Done**
- ~~`declaration_emitter/usage_analyzer.rs:772`~~ Walk type arguments in usage analysis — **Done**
- ~~`transforms/es5.rs:247`~~ Pass super args in class constructors — **Done**

---

## LSP (`tsz-lsp`)

### High Priority — Cross-File Features

| File | Description | Impact |
|------|-------------|--------|
| `project.rs:1926` | Extend search across all files using `SymbolIndex` | Find-all-references limited to single file |
| `project.rs:1951` | Extend heritage clause search across all files | Go-to-implementation incomplete |

### Medium Priority — Feature Completeness

| File | Description |
|------|-------------|
| `completions.rs:739` | More AST-based checks needed for completion filtering |
| `linked_editing.rs:141` | Add tests once test infrastructure is set up |
| `file_rename.rs:170` | Add tests once test infrastructure is set up |

### Ignored Tests (9)

| File | Reason |
|------|--------|
| `project_tests.rs` (4) | LSP scope cache performance / reuse after edit |
| `signature_help_tests.rs` (3) | Signature help overload selection / count |
| `completions.rs` (4) | New identifier location detection, default sort text |
| `highlighting.rs` | Document highlight when no symbol found |

---

## CLI (`tsz-cli`)

### Production TODOs

| File | Description |
|------|-------------|
| `driver.rs:445` | Track most recently changed `.d.ts` file |
| `driver_resolution.rs:755` | Make module resolution configurable via CLI plumbing |
| `bin/tsz_lsp.rs:506,514` | Implement diagnostics / full completions when type checker is complete |
| `tests/build_tests.rs:123` | Check source file changes in build tests |

### Completed Items
- ~~Dead code~~ `handle_build_legacy`, `calculate_required_imports`, `diff_paths` — **Removed**

### Ignored Tests (7)

| File | Reason |
|------|--------|
| `driver_tests.rs` | Node modules type version resolution |
| `driver_tests.rs` (2) | Multi-file project compilation with imports |
| `driver_tests.rs` | Stack overflow — generic utility library infinite recursion |
| `driver_tests.rs` | Generic utility library compilation with constraints |
| `driver_tests.rs` | Function call spread compilation |
| `driver_tests.rs` | General test fix needed |

---

## WASM (`tsz-wasm`)

| File | Description | Impact |
|------|-------------|--------|
| `wasm_api/emit.rs:178` | Implement source maps | No source map output from WASM API |

### Completed Items
- ~~`wasm_api/language_service.rs:152`~~ Calculate actual diagnostic length — **Done**

---

## Integration Tests (`src/tests/checker_state_tests.rs`)

~32 ignored tests remaining. Grouped by category:

### Stack Overflows (3)
- Readonly index signature tests cause infinite recursion
- Static private field access causes infinite recursion
- Interface extending class with private fields causes infinite recursion

### Feature Implementation in Progress (11)
- Multiple `#[ignore = "TODO: Feature implementation in progress"]` scattered throughout

### Checker Needs Work (5)
- Multiple `#[ignore = "TODO: checker needs work"]` entries

### Specific Feature Gaps
| Reason | Count |
|--------|-------|
| Generic test fixes needed | 8 |
| Overload compatibility check — custom covariant parameter checking | 1 |
| Tuple spread in overload calls — rest parameter handling | 1 |
| Readonly method signature assignability check | 1 |
| Variadic tuple optional tail inference | 1 |
| Mixin pattern — advanced generic class expression support | 1 |
| Test infrastructure doesn't populate definition store for type aliases | 1 |
| Pre-existing failure — `IMixin` resolves to error in intersection type | 1 |
| `this` in derived constructor typed as `object` instead of Base | 1 |

### Recently Un-ignored (passing now)
- `test_covariant_this_interface_pattern`
- `test_readonly_element_access_assignment_2540`
- `test_ts2339_computed_name_this_missing_static`
- `test_ts2339_computed_name_this_in_class_expression`
- `test_contextual_property_type_infers_callback_param`

---

## Priority Tiers

### Tier 1 — Blocks Conformance (do first)
1. **Homomorphic mapped types** (`solver/evaluate_rules/mapped.rs`) — array/tuple preservation
2. **Generic constraint checking** (7 ignored checker tests) — `T extends U` enforcement
3. **Stack overflows** (3 integration tests) — infinite recursion in class hierarchies

### Tier 2 — Improves Correctness
1. **Promise detection** (`checker/state_checking_members.rs`, `function_type.rs`) — requires lib loading
2. **ThisType selection** (`solver/contextual.rs`) — proper union dispatch
3. **Overload predicate selection** (`checker/control_flow_narrowing.rs`)

### Tier 3 — Improves Developer Experience
1. **LSP cross-file search** (`lsp/project.rs`) — find-references across files
2. **LSP scope cache** (4 ignored tests) — performance for large projects
3. **Signature help overloads** (3 ignored LSP tests)
4. **WASM source maps** (`wasm_api/emit.rs`)

### Tier 4 — Cleanup & Polish
1. Migrate to `solver-visitor` pattern (`checker/type_checking_utilities.rs`)
2. Initialize usage analyzer tests properly (`emitter/declaration_emitter/tests/`)
3. Make module resolution configurable (`cli/driver_resolution.rs`)
