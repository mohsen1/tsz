# TSZ Codebase TODO Tracker

**Last Updated**: February 2026
**Source**: Systematic `TODO`/`FIXME`/`HACK` grep across all crates

---

## Summary

| Crate | Production TODOs | Ignored Tests | Total |
|-------|-----------------|---------------|-------|
| tsz-solver | 10 | 15 | 25 |
| tsz-checker | 8 | 12 | 20 |
| tsz-emitter | 8 | 0 | 8 |
| tsz-lsp | 4 | 9 | 13 |
| tsz-cli | 3 | 7 | 10 |
| tsz-wasm | 2 | 0 | 2 |
| Integration tests (src/) | 0 | 37 | 37 |
| **Total** | **35** | **80** | **115** |

---

## Solver (`tsz-solver`)

### High Priority — Feature Gaps

These block conformance progress and affect type correctness.

| File | Description | Impact |
|------|-------------|--------|
| `evaluate_rules/mapped.rs:145` | Array/Tuple preservation for homomorphic mapped types | Mapped types over arrays/tuples produce wrong shapes |
| `evaluate.rs:216` | Application type expansion (Redux test fix) | Generic application types may not fully expand |
| `db.rs:1554` | Look up symbol extends clause for class hierarchy | Class hierarchy subtyping incomplete |

### Medium Priority — Correctness Improvements

| File | Description |
|------|-------------|
| `contextual.rs:176` | Check if `def_id` points to `lib.d.ts` `ThisType` via symbol resolution |
| `contextual.rs:252` | Blindly picks the first `ThisType` — needs proper selection |
| `narrowing.rs:824` | Check for static `[Symbol.hasInstance]` method overriding standard narrowing |
| `narrowing.rs:394` | Pass resolver to `PropertyAccessEvaluator` when available |
| `db.rs:701` | Apply flags to `CompatChecker` once it supports `apply_flags` |
| `db.rs:1163` | Configure `SubtypeChecker` with variance flags |
| `tracer.rs:342` | Return apparent shapes for primitives (`String`, `Number`, etc.) |

### Low Priority — Integration

| File | Description |
|------|-------------|
| `operations_property.rs:96` | Integrate resolver into `PropertyAccessEvaluator` |
| `lib.rs:120` | Fix API mismatches in `mapped_key_remap_tests` (`TypeId::TYPE_PARAM`, `keyof`, etc.) |

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

### High Priority — Feature Gaps

| File | Description | Impact |
|------|-------------|--------|
| `type_checking_utilities.rs:2247` | Implement `get_symbol_by_name` | Symbol lookup gaps |
| `type_checking_utilities.rs:1760` | Evaluate constant expression to get literal value | Const enum members, etc. |

### Medium Priority — Improvements

| File | Description |
|------|-------------|
| `state.rs:757` | Add more node types for checking (object literals, etc.) |
| `iterable_checker.rs:124` | Check call signatures for generators when `CallableShape` is implemented |
| `state_checking_members.rs:1817` | Investigate lib loading for Promise detection |
| `function_type.rs:335` | Investigate lib loading for Promise detection |
| `type_node.rs:61` | Add cache key based on type param hash for smarter caching |
| `type_checking_utilities.rs:1372` | `jsdoc_for_node` lives in LSP module; stub until extracted |
| `type_checking_utilities.rs:1960,1969` | Consider migrating to `type_queries` / `solver-visitor` pattern |
| `control_flow_narrowing.rs:304` | Heuristic picks first predicate signature — needs improvement |
| `import_checker.rs:501` | Check if right member exists (TS2694) when left is resolved |
| `scope_finder.rs:14` | Remove dead code once methods are used by the checker |

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

### High Priority — Type Printer Completeness

| File | Description |
|------|-------------|
| `type_printer.rs:213,219` | Look up actual string from interner (2 occurrences) |
| `type_printer.rs:378` | Implement callable type printing (overloaded call signatures) |
| `type_printer.rs:437` | Implement enum type printing |
| `type_printer.rs:453` | Implement conditional type printing |
| `type_printer.rs:479` | Implement mapped type printing |
| `type_printer.rs:484` | Implement index access type printing |
| `type_printer.rs:493` | Implement string intrinsic type printing |

### Medium Priority — Declaration Emitter

| File | Description |
|------|-------------|
| `declaration_emitter/mod.rs:3614` | Store symbol alias for symbol-based tracking |
| `declaration_emitter/mod.rs:3752` | Check if symbol is declared in current file |
| `declaration_emitter/mod.rs:3842` | Implement proper relative path calculation |
| `declaration_emitter/usage_analyzer.rs:772` | Walk type arguments in usage analysis |
| `declaration_emitter/tests/` (10) | All usage analyzer tests need `CheckerContext` initialization |

### Low Priority — Transforms

| File | Description |
|------|-------------|
| `transforms/es5.rs:247` | Pass super args instead of just `this()` in class constructors |

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
| `wasm_api/language_service.rs:152` | Calculate actual diagnostic length | Diagnostics report `length: 0` |

---

## Integration Tests (`src/tests/checker_state_tests.rs`)

37 ignored tests total. Grouped by category:

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
| Lazy contextual type resolution conflicts with contravariance | 1 |
| Computed property names with `this` for static members | 1 |
| Computed property names with `this` in class expressions | 1 |
| Variadic tuple optional tail inference | 1 |
| Mixin pattern — advanced generic class expression support | 1 |
| Test infrastructure doesn't populate definition store for type aliases | 1 |
| Pre-existing failure — `IMixin` resolves to error in intersection type | 1 |
| `this` in derived constructor typed as `object` instead of Base | 1 |

---

## Priority Tiers

### Tier 1 — Blocks Conformance (do first)
1. **Homomorphic mapped types** (`solver/evaluate_rules/mapped.rs`) — array/tuple preservation
2. **Generic constraint checking** (7 ignored checker tests) — `T extends U` enforcement
3. **Stack overflows** (3 integration tests) — infinite recursion in class hierarchies

### Tier 2 — Improves Correctness
1. **Application type expansion** (`solver/evaluate.rs`) — generic instantiation completeness
2. **Type printer completeness** (`emitter/type_printer.rs`) — 7 missing type kinds
3. **Symbol lookup** (`checker/type_checking_utilities.rs`) — `get_symbol_by_name`
4. **Constant expression evaluation** (`checker/type_checking_utilities.rs`) — const enum values

### Tier 3 — Improves Developer Experience
1. **LSP cross-file search** (`lsp/project.rs`) — find-references across files
2. **LSP scope cache** (4 ignored tests) — performance for large projects
3. **Signature help overloads** (3 ignored LSP tests)
4. **WASM source maps** (`wasm_api/emit.rs`)
5. **Declaration emitter** — symbol tracking and path calculation

### Tier 4 — Cleanup & Polish
1. Remove dead code (`checker/scope_finder.rs`)
2. Migrate to `solver-visitor` pattern (`checker/type_checking_utilities.rs`)
3. Initialize usage analyzer tests properly (`emitter/declaration_emitter/tests/`)
4. Make module resolution configurable (`cli/driver_resolution.rs`)
