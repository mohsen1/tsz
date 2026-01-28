# Magic Numbers Audit

This document catalogs magic numbers found in the codebase that may warrant review for named constants, documentation, or consolidation.

## Status: Well-Structured

The codebase already follows good practices - most limits are named constants. This document identifies areas for potential improvement.

---

## 1. Recursion Depth Limits

These are already named constants but have **inconsistent values** and **duplicates** that could be consolidated.

| File | Constant | Value | Notes |
|------|----------|-------|-------|
| `src/checker/type_node.rs:15` | `MAX_TYPE_NODE_CHECK_DEPTH` | 500 | |
| `src/checker/expr.rs:41` | `MAX_EXPR_CHECK_DEPTH` | 500 | Same as above - consolidate? |
| `src/checker/state.rs:127` | `MAX_INSTANTIATION_DEPTH` | 50 | |
| `src/checker/state.rs:130` | `MAX_CALL_DEPTH` | 20 | |
| `src/checker/state.rs:134` | `MAX_TREE_WALK_ITERATIONS` | 10,000 | |
| `src/checker/class_type.rs:24` | `MAX_CLASS_INHERITANCE_DEPTH` | 256 | |
| `src/checker/optional_chain.rs:38` | `MAX_OPTIONAL_CHAIN_DEPTH` | 1,000 | |
| `src/checker/type_checking.rs:8268` | `MAX_ALIAS_RESOLUTION_DEPTH` | 128 | **Local** - should be module-level? |
| `src/checker/type_checking.rs:11484` | `MAX_TREE_WALK_ITERATIONS` | 1,000 | **Duplicate name, different value** |
| `src/checker/symbol_resolver.rs:1013,1135` | `MAX_QUALIFIED_NAME_DEPTH` | 128 | **Local** - defined twice |
| `src/emitter/special_expressions.rs:103` | `MAX_DECORATOR_TEXT_DEPTH` | 10 | |
| `src/emitter/mod.rs:183` | `MAX_EMIT_RECURSION_DEPTH` | 1,000 | |
| `src/lowering_pass.rs:51` | `MAX_AST_DEPTH` | 500 | |
| `src/lowering_pass.rs:54` | `MAX_QUALIFIED_NAME_DEPTH` | 100 | **Different value from symbol_resolver** |
| `src/lowering_pass.rs:57` | `MAX_BINDING_PATTERN_DEPTH` | 100 | |
| `src/parser/state.rs:135` | `MAX_RECURSION_DEPTH` | 1,000 | |
| `src/checker/state.rs:1587` | `MAX_MERGE_DEPTH` | 32 | **Local** |
| `src/checker/flow_analyzer.rs:22` | `MAX_FLOW_ANALYSIS_ITERATIONS` | 100,000 | |

### Action Items
- [ ] Consolidate `MAX_TREE_WALK_ITERATIONS` - currently 10,000 vs 1,000
- [ ] Consolidate `MAX_QUALIFIED_NAME_DEPTH` - currently 128 vs 100
- [ ] Move local constants to module-level for visibility
- [ ] Document rationale for each limit value

---

## 2. Type Resolution/Operations Limits

| File | Constant | Value | Notes |
|------|----------|-------|-------|
| `src/checker/state.rs:140-142` | `MAX_TYPE_RESOLUTION_OPS` | 20,000 (WASM) / 100,000 (native) | |
| `src/solver/subtype.rs:26` | `MAX_SUBTYPE_DEPTH` | 100 | |
| `src/solver/subtype.rs:175` | `MAX_IN_PROGRESS_PAIRS` | 10,000 | |
| `src/solver/subtype.rs:216` | `MAX_TOTAL_SUBTYPE_CHECKS` | 100,000 | |
| `src/solver/tracer.rs:41` | `MAX_TOTAL_TRACER_CHECKS` | 100,000 | |
| `src/solver/tracer.rs:44` | `MAX_IN_PROGRESS_PAIRS` | 10,000 | **Duplicate of subtype.rs** |
| `src/solver/evaluate.rs:39` | `MAX_EVALUATE_DEPTH` | 50 | |
| `src/solver/evaluate.rs:43` | `MAX_VISITING_SET_SIZE` | 10,000 | |
| `src/solver/evaluate.rs:47` | `MAX_TOTAL_EVALUATIONS` | 100,000 | |
| `src/solver/lower.rs:24` | `MAX_LOWERING_OPERATIONS` | 100,000 | |
| `src/solver/lower.rs:184` | `MAX_TREE_WALK_ITERATIONS` | 10,000 | **Another duplicate** |
| `src/solver/instantiate.rs:21` | `MAX_INSTANTIATION_DEPTH` | 50 | |
| `src/solver/infer.rs:227` | `MAX_CONSTRAINT_ITERATIONS` | 100 | |
| `src/solver/infer.rs:230` | `MAX_TYPE_RECURSION_DEPTH` | 100 | |
| `src/solver/operations.rs:46` | `MAX_CONSTRAINT_RECURSION_DEPTH` | 100 | |
| `src/solver/operations.rs:768` | `MAX_UNWRAP_ITERATIONS` | 1,000 | **Local** |
| `src/solver/operations.rs:2068` | `MAX_MAPPED_ACCESS_DEPTH` | 50 | **Local** |
| `src/solver/type_queries.rs:550` | `MAX_DEPTH` | 100 | **Local, generic name** |
| `src/solver/evaluate_rules/template_literal.rs:123` | `MAX_LITERAL_COUNT_DEPTH` | 50 | |

### Action Items
- [ ] Consider a central `src/limits.rs` module for shared limits
- [ ] `MAX_IN_PROGRESS_PAIRS` defined in both subtype.rs and tracer.rs
- [ ] Rename `MAX_DEPTH` to something more descriptive

---

## 3. Size/Capacity Limits

| File | Constant | Value | Notes |
|------|----------|-------|-------|
| `src/solver/intern.rs:39` | `PROPERTY_MAP_THRESHOLD` | 24 | Why 24? Document rationale |
| `src/solver/intern.rs:40` | `TYPE_LIST_INLINE` | 8 | SmallVec inline capacity |
| `src/solver/evaluate_rules/mapped.rs:51` | `MAX_MAPPED_KEYS` | 250 (WASM) / 500 (native) | |
| `src/solver/evaluate_rules/index_access.rs:53,125` | `MAX_UNION_INDEX_SIZE` | 100 | **Local, defined twice** |
| `src/solver/evaluate_rules/conditional.rs:217` | `MAX_DISTRIBUTION_SIZE` | 100 | **Local** |
| `src/solver/instantiate.rs:458` | `MAX_DISTRIBUTION_SIZE` | 100 | **Duplicate of conditional.rs** |
| `src/solver/diagnostics.rs:1023` | `UNION_MEMBER_DIAGNOSTIC_LIMIT` | 3 | |
| `src/parser/node.rs:1053` | `MAX_NODE_PREALLOC` | 5,000,000 | |
| `src/binder.rs:267` | `MAX_SYMBOL_PREALLOC` | 1,000,000 | |
| `src/lsp/project.rs:70` | `INCREMENTAL_NODE_MULTIPLIER` | 4 | |
| `src/lsp/project.rs:71` | `INCREMENTAL_MIN_NODE_BUDGET` | 4,096 | |

### Action Items
- [ ] Add comment explaining `PROPERTY_MAP_THRESHOLD = 24` choice
- [ ] Consolidate `MAX_DISTRIBUTION_SIZE` definitions
- [ ] Consolidate `MAX_UNION_INDEX_SIZE` definitions

---

## 4. WASM-Aware Memory Limits

| File | Constant | WASM | Native | Notes |
|------|----------|------|--------|-------|
| `src/checker/state.rs:140-142` | `MAX_TYPE_RESOLUTION_OPS` | 20,000 | 100,000 | 5x difference |
| `src/solver/intern.rs:46-48` | `TEMPLATE_LITERAL_EXPANSION_LIMIT` | 2,000 | 100,000 | 50x difference |
| `src/solver/intern.rs:53-55` | `MAX_INTERNED_TYPES` | 500,000 | 5,000,000 | 10x difference |
| `src/solver/evaluate_rules/mapped.rs:51` | `MAX_MAPPED_KEYS` | 250 | 500 | 2x difference |

### Action Items
- [ ] Document WASM memory constraints that drove these limits
- [ ] Consider a central `wasm_limits.rs` module

---

## 5. Bit Manipulation Constants

| File | Constant | Value | Purpose |
|------|----------|-------|---------|
| `src/solver/intern.rs:36` | `SHARD_BITS` | 6 | 2^6 = 64 shards |
| `src/solver/intern.rs:37` | `SHARD_COUNT` | 64 | Derived from SHARD_BITS |
| `src/solver/intern.rs:38` | `SHARD_MASK` | 63 | Derived from SHARD_COUNT |
| `src/interner.rs:39` | `SHARD_BITS` | 6 | **Duplicate** |

### Action Items
- [ ] Document why 64 shards were chosen (CPU cache lines? concurrency?)
- [ ] Consider sharing shard constants between modules

---

## 6. TypeScript Error Codes

Error codes are TypeScript-standardized but appear as raw numbers in tests and one module.

| File | Code | Meaning |
|------|------|---------|
| `src/module_resolver.rs:39` | 2307 | Cannot find module (named: `CANNOT_FIND_MODULE`) |
| Various test files | 2304 | Symbol not found |
| Various test files | 2322 | Type not assignable |
| Various test files | 2339 | Property not defined |
| Various test files | 2741 | Property missing on type |
| `src/parser/state.rs:7591` | 1500 | Duplicate regex flag |

### Action Items
- [ ] Create `src/error_codes.rs` with all TypeScript error codes
- [ ] Replace raw numbers in tests with named constants

---

## 7. Character Codes

**Status: Good** - Already centralized in `src/char_codes.rs`

---

## Priority Recommendations

### High Priority (Consistency Issues)
1. **Consolidate duplicate constants** with different values:
   - `MAX_TREE_WALK_ITERATIONS`: 10,000 vs 1,000
   - `MAX_QUALIFIED_NAME_DEPTH`: 128 vs 100

2. **Move local constants to module level** for visibility and documentation

### Medium Priority (Code Organization)
3. Create `src/limits.rs` for shared limits across modules
4. Create `src/error_codes.rs` for TypeScript error codes
5. Document WASM limit rationales

### Low Priority (Nice to Have)
6. Add comments explaining non-obvious values (PROPERTY_MAP_THRESHOLD = 24)
7. Standardize naming (MAX_* vs *_LIMIT vs *_THRESHOLD)

---

## Notes

- The codebase is already well-structured with named constants
- Most magic numbers are configuration thresholds with clear names
- WASM/native conditional compilation is handled correctly
- Main issues are duplicate definitions and inconsistent values
