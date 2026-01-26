# Path to 100% TypeScript Conformance

**Date**: 2026-01-26
**Current Status**: 27.1% (3,310/12,198 tests passing)
**Target**: 100% conformance with clean solver/visitor architecture

---

## Executive Summary

This document outlines a comprehensive plan to achieve 100% TypeScript conformance while maintaining a clean, maintainable architecture based on the solver/visitor pattern. The plan is organized into 5 phases with clear milestones, priorities, and success metrics.

### Current State Snapshot

| Metric | Value |
|--------|-------|
| **Pass Rate** | 27.1% (3,310/12,198) |
| **Failing Tests** | 8,807 |
| **Crashed Tests** | 19 |
| **OOM Tests** | 9 |
| **Timed Out Tests** | 53 |
| **Worker Crashes** | 116 |

### Top Missing Error Codes (We Should Emit But Don't)

| Code | Description | Count | Priority |
|------|-------------|-------|----------|
| **TS2318** | Cannot find global type | 3,387 | P0 |
| **TS2304** | Cannot find name | 2,230 | P0 |
| **TS2307** | Cannot find module | 2,069 | P0 |
| **TS2488** | Type must have Symbol.iterator | 1,690 | P1 |
| **TS2322** | Type not assignable | 1,106 | P1 |
| **TS2583** | Cannot find name (suggest lib) | 1,040 | P1 |
| **TS18050** | Element implicitly has 'any' | 679 | P2 |
| **TS2300** | Duplicate identifier | 654 | P2 |

### Top Extra Error Codes (We Emit But Shouldn't)

| Code | Description | Count | Priority |
|------|-------------|-------|----------|
| **TS2749** | Refers to value but used as type | 40,621 | P0 |
| **TS2322** | Type not assignable (false positive) | 12,971 | P0 |
| **TS2693** | Type only refers to type | 9,559 | P1 |
| **TS2339** | Property does not exist | 5,523 | P1 |
| **TS2507** | Type is not a constructor | 4,364 | P1 |
| **TS2345** | Argument not assignable | 2,879 | P2 |
| **TS2362** | Left-hand arithmetic operand | 2,761 | P2 |
| **TS1005** | Expected token | 2,730 | P2 |

---

## Phase 1: Stability & Foundation (Target: 40% conformance)

### 1.1 Fix Critical Crashes (Week 1-2)

**Problem**: 19 crashes, 9 OOM, 53 timeouts, 116 worker crashes

**Root Causes**:
1. Infinite recursion in type evaluation
2. Stack overflow in deep binary expressions
3. Memory exhaustion in complex generic instantiation
4. Timeout in pathological code patterns

**Solutions**:

```rust
// 1. Add/enforce depth limits in ALL recursive functions
const MAX_TYPE_INSTANTIATION_DEPTH: u32 = 100;
const MAX_CONDITIONAL_EVALUATION_DEPTH: u32 = 50;
const MAX_INDEX_ACCESS_DEPTH: u32 = 50;
const MAX_MAPPED_TYPE_DEPTH: u32 = 50;

// 2. Convert recursive algorithms to iterative where possible
// Example: Deep binary expression handling
fn check_binary_expression_iterative(&mut self, node: NodeId) -> TypeId {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        // Process iteratively instead of recursively
    }
}

// 3. Add fuel consumption tracking
struct TypeChecker {
    fuel: Cell<u32>,
    max_fuel: u32,
}

impl TypeChecker {
    fn consume_fuel(&self) -> bool {
        let current = self.fuel.get();
        if current == 0 { return false; }
        self.fuel.set(current - 1);
        true
    }
}
```

**Files to Modify**:
- `src/solver/evaluate.rs` - Add depth guards to `evaluate()`
- `src/solver/instantiate.rs` - Add depth guards to instantiation
- `src/checker/type_computation.rs` - Convert recursive to iterative
- `src/solver/subtype.rs` - Ensure cycle detection is working

**Success Metric**: 0 crashes, 0 OOM, <5 timeouts

### 1.2 Fix Compiler Options Parsing (Week 1)

**Problem**: Tests crash due to malformed compiler options
```
Failed to parse compiler options: invalid type: string "true, false", expected a boolean
```

**Solution**: Fix `parse_test_option_bool` in symbol_resolver.rs
```rust
fn parse_test_option_bool(value: &str) -> bool {
    // Handle comma-separated values by taking first
    let first_value = value.split(',').next().unwrap_or(value).trim();
    matches!(first_value, "true" | "True" | "TRUE" | "1")
}
```

**Files**: `src/checker/symbol_resolver.rs`

### 1.3 Fix TS2749 False Positives (Week 2-3) - CRITICAL

**Problem**: 40,621 extra TS2749 errors ("refers to value but used as type")

**Root Cause**: Symbol resolution not distinguishing value vs type contexts

**Solution**:
```rust
// In symbol resolution, track context
enum SymbolContext {
    Type,      // Used as type annotation: let x: Foo
    Value,     // Used as value: new Foo()
    TypeValue, // Can be either (class names)
}

fn resolve_symbol(&self, name: &str, context: SymbolContext) -> Option<Symbol> {
    let symbol = self.lookup(name)?;

    match context {
        SymbolContext::Type => {
            if symbol.flags.contains(SymbolFlags::TYPE) {
                Some(symbol)
            } else {
                None // Don't error yet - might be valid
            }
        }
        SymbolContext::Value => {
            if symbol.flags.contains(SymbolFlags::VALUE) {
                Some(symbol)
            } else {
                None
            }
        }
        SymbolContext::TypeValue => Some(symbol),
    }
}
```

**Files**:
- `src/checker/symbol_resolver.rs` - Fix context tracking
- `src/binder/mod.rs` - Ensure SymbolFlags are set correctly
- `src/checker/state.rs` - Fix type vs value position detection

### 1.4 Fix Global Type Resolution (Week 3-4)

**Problem**: 3,387 missing TS2318 errors + lib.d.ts types not loading

**Root Cause**: Global symbols (Array, Promise, Object) not being resolved

**Solution**:
1. Ensure lib.d.ts files are properly loaded
2. Add global symbol table initialization
3. Fix `resolve_global_type` to search lib scope

```rust
// lib_loader.rs improvements
impl LibLoader {
    fn load_lib_files(&mut self, target: ScriptTarget, libs: &[String]) {
        // 1. Load core lib (always needed)
        self.load_lib("lib.es5.d.ts");

        // 2. Load target-specific libs
        match target {
            ScriptTarget::ES2015 => self.load_lib("lib.es2015.d.ts"),
            ScriptTarget::ES2020 => {
                self.load_lib("lib.es2015.d.ts");
                self.load_lib("lib.es2020.d.ts");
            }
            // ...
        }

        // 3. Register global symbols
        self.register_globals();
    }
}
```

**Files**:
- `src/lib_loader.rs` - Fix lib loading
- `src/binder/mod.rs` - Add global scope
- `src/checker/symbol_resolver.rs` - Fix global lookup

---

## Phase 2: Core Type System Fixes (Target: 60% conformance)

### 2.1 Fix TS2322 False Positives (Week 5-6)

**Problem**: 12,971 extra TS2322 + 1,106 missing TS2322

**Root Causes**:
1. Union type assignability too strict
2. Literal type widening not applied
3. Contextual typing not propagating
4. Object literal excess property checking incorrect

**Solution Areas**:

```rust
// 1. Fix union assignability in subtype_rules/unions.rs
fn check_union_to_all_optional_object(
    &mut self,
    source_members: &[TypeId],
    target: ObjectShapeId,
) -> bool {
    // Each union member only needs to satisfy properties it has
    // Optional properties in target are satisfied by absence
}

// 2. Fix literal widening in type_computation.rs
fn widen_literal_type(&self, ty: TypeId) -> TypeId {
    match self.types.get_key(ty) {
        TypeKey::Literal(LiteralValue::String(_)) => TypeId::STRING,
        TypeKey::Literal(LiteralValue::Number(_)) => TypeId::NUMBER,
        TypeKey::Literal(LiteralValue::Boolean(_)) => TypeId::BOOLEAN,
        _ => ty,
    }
}

// 3. Fix contextual typing in contextual.rs
fn apply_contextual_type(
    &self,
    expr_type: TypeId,
    contextual_type: TypeId,
) -> TypeId {
    // Preserve literal types when contextual type allows
}
```

**Files**:
- `src/solver/subtype_rules/unions.rs`
- `src/solver/subtype_rules/objects.rs`
- `src/checker/type_computation.rs`
- `src/solver/contextual.rs`

### 2.2 Fix Symbol Resolution (Week 6-7)

**Problem**: 2,230 missing TS2304 ("Cannot find name")

**Root Causes**:
1. Scope chain not properly constructed
2. Import resolution incomplete
3. Re-exports not followed
4. Namespace member lookup failing

**Solution**:
```rust
// Fix scope chain construction in binder
fn build_scope_chain(&mut self, node: NodeId) -> ScopeId {
    // 1. Create proper lexical scope hierarchy
    // 2. Handle function hoisting
    // 3. Handle variable hoisting (var vs let/const)
    // 4. Handle class declaration hoisting
}

// Fix import resolution
fn resolve_import(&self, specifier: &str, from_file: &Path) -> Option<SymbolId> {
    // 1. Resolve module path
    // 2. Load module if not cached
    // 3. Find exported symbol
    // 4. Follow re-exports
}
```

**Files**:
- `src/binder/mod.rs` - Fix scope construction
- `src/module_resolver.rs` - Fix import resolution
- `src/checker/symbol_resolver.rs` - Fix lookup

### 2.3 Fix Module Resolution (Week 7-8)

**Problem**: 2,069 missing TS2307 ("Cannot find module")

**Root Causes**:
1. Node module resolution algorithm incomplete
2. Path mapping not implemented
3. @types package resolution missing
4. Relative vs absolute path handling

**Solution**:
```rust
// Implement full Node module resolution
fn resolve_module_name(
    &self,
    module_name: &str,
    containing_file: &Path,
    options: &CompilerOptions,
) -> Option<PathBuf> {
    // 1. Check if relative path
    if module_name.starts_with('.') {
        return self.resolve_relative(module_name, containing_file);
    }

    // 2. Check path mappings
    if let Some(mapped) = self.check_path_mappings(module_name, options) {
        return Some(mapped);
    }

    // 3. Node module resolution
    self.resolve_node_module(module_name, containing_file)
}
```

**Files**:
- `src/module_resolver.rs` - Implement full resolution

### 2.4 Fix Iterator Protocol (Week 8)

**Problem**: 1,690 missing TS2488 ("Type must have Symbol.iterator")

**Root Cause**: Symbol.iterator property check not working correctly

**Solution**:
```rust
fn is_iterable_type(&self, ty: TypeId) -> bool {
    // Check for [Symbol.iterator]() method
    let iterator_method = self.get_property(ty, "[Symbol.iterator]");
    if let Some(method_ty) = iterator_method {
        // Verify it returns an iterator
        if let Some(return_ty) = self.get_call_return_type(method_ty) {
            return self.is_iterator_type(return_ty);
        }
    }

    // Also check for built-in iterables
    self.is_array_like(ty) || self.is_string_type(ty)
}
```

**Files**:
- `src/checker/iterable_checker.rs`
- `src/checker/iterators.rs`

---

## Phase 3: Advanced Type Features (Target: 80% conformance)

### 3.1 Conditional Types (Week 9-10)

**Current Implementation**: Basic support exists in `solver/evaluate_rules/conditional.rs`

**Improvements Needed**:
1. Distributive conditional types over unions
2. Infer type in conditional branches
3. Nested conditional type evaluation
4. Conditional type constraints

```rust
// Proper distributive conditional implementation
fn evaluate_conditional_type(
    &mut self,
    check_type: TypeId,
    extends_type: TypeId,
    true_branch: TypeId,
    false_branch: TypeId,
    is_distributive: bool,
) -> TypeId {
    if is_distributive && self.is_type_parameter(check_type) {
        // Defer evaluation until type parameter is instantiated
        return self.create_conditional_type(
            check_type, extends_type, true_branch, false_branch
        );
    }

    if is_distributive && self.is_union_type(check_type) {
        // Distribute over union
        let members = self.get_union_members(check_type);
        let results: Vec<TypeId> = members.iter().map(|member| {
            self.evaluate_conditional_type(
                *member, extends_type, true_branch, false_branch, false
            )
        }).collect();
        return self.create_union_type(&results);
    }

    // Regular evaluation
    if self.is_subtype_of(check_type, extends_type) {
        true_branch
    } else {
        false_branch
    }
}
```

### 3.2 Mapped Types (Week 10-11)

**Current Implementation**: Basic support in `solver/evaluate_rules/mapped.rs`

**Improvements Needed**:
1. Homomorphic mapped types
2. Key remapping (`as` clause)
3. Property modifiers (+/- readonly, +/- optional)
4. Template literal key mapping

```rust
fn evaluate_mapped_type(
    &mut self,
    type_param: TypeParameterId,
    constraint: TypeId,        // e.g., keyof T
    template: TypeId,          // e.g., T[K]
    modifiers: MappedModifiers,
    name_type: Option<TypeId>, // Key remapping
) -> TypeId {
    let keys = self.get_keys_of_type(constraint);
    let properties: Vec<PropertyInfo> = keys.iter().filter_map(|key| {
        // Apply key remapping if present
        let mapped_key = if let Some(name_ty) = name_type {
            self.evaluate_key_remap(key, name_ty)
        } else {
            Some(*key)
        };

        mapped_key.map(|k| {
            let value_type = self.instantiate_mapped_template(template, type_param, k);
            PropertyInfo {
                name: k,
                type_id: value_type,
                optional: modifiers.apply_optional(/* original optional */),
                readonly: modifiers.apply_readonly(/* original readonly */),
            }
        })
    }).collect();

    self.create_object_type(properties)
}
```

### 3.3 Template Literal Types (Week 11-12)

**Current Implementation**: Pattern matching in `solver/subtype_rules/literals.rs`

**Improvements Needed**:
1. Template literal inference
2. String manipulation intrinsics (Uppercase, Lowercase, etc.)
3. Union distribution in template literals
4. Complex pattern backtracking

```rust
fn check_template_literal_assignability(
    &mut self,
    source: TypeId,
    template_parts: &[TemplatePart],
) -> bool {
    if let TypeKey::Literal(LiteralValue::String(s)) = self.types.get_key(source) {
        return self.match_template_pattern(s, template_parts);
    }

    if self.is_union_type(source) {
        // All union members must match
        return self.get_union_members(source)
            .iter()
            .all(|m| self.check_template_literal_assignability(*m, template_parts));
    }

    // General string is assignable if template has no type holes
    if self.is_string_type(source) {
        return template_parts.iter().all(|p| matches!(p, TemplatePart::Literal(_)));
    }

    false
}
```

### 3.4 Index Access Types (Week 12)

**Current Implementation**: `solver/evaluate_rules/index_access.rs`

**Improvements Needed**:
1. Indexed access on unions
2. Indexed access with literal keys
3. Indexed access on tuples
4. Recursive index access types

```rust
fn evaluate_index_access(
    &mut self,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    // Handle union of indices
    if self.is_union_type(index_type) {
        let members = self.get_union_members(index_type);
        let results: Vec<TypeId> = members.iter().map(|idx| {
            self.evaluate_index_access(object_type, *idx)
        }).collect();
        return self.create_union_type(&results);
    }

    // Handle union of objects
    if self.is_union_type(object_type) {
        let members = self.get_union_members(object_type);
        let results: Vec<TypeId> = members.iter().map(|obj| {
            self.evaluate_index_access(*obj, index_type)
        }).collect();
        return self.create_union_type(&results);
    }

    // Handle specific cases
    match self.types.get_key(object_type) {
        TypeKey::Array(element) => element,
        TypeKey::Tuple(elements) => {
            if let Some(idx) = self.get_literal_number(index_type) {
                elements.get(idx as usize).copied().unwrap_or(TypeId::UNDEFINED)
            } else {
                self.create_union_type(&elements)
            }
        }
        TypeKey::Object(shape) => {
            self.get_index_signature_type(shape, index_type)
        }
        _ => TypeId::ERROR,
    }
}
```

---

## Phase 4: Edge Cases & Compatibility (Target: 95% conformance)

### 4.1 Strict Null Checks (Week 13)

**Problem**: null/undefined handling inconsistent

**Solution**:
```rust
fn check_strict_null_assignment(
    &self,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    if !strict_null_checks {
        // In non-strict mode, null/undefined are assignable to anything
        return true;
    }

    // In strict mode, null/undefined only assignable to:
    // - themselves
    // - union types containing them
    // - any/unknown
    if source == TypeId::NULL || source == TypeId::UNDEFINED {
        return target == source
            || target == TypeId::ANY
            || target == TypeId::UNKNOWN
            || self.union_contains(target, source);
    }

    true
}
```

### 4.2 Excess Property Checking (Week 13)

**Problem**: Object literal excess property errors incorrect

**Solution**:
```rust
fn check_excess_properties(
    &self,
    source: ObjectShapeId,
    target: ObjectShapeId,
    is_fresh_literal: bool,
) -> Vec<ExcessPropertyError> {
    if !is_fresh_literal {
        return vec![]; // Only check fresh object literals
    }

    let source_props = self.get_object_properties(source);
    let target_props = self.get_object_properties(target);

    source_props.iter()
        .filter(|p| !target_props.contains_key(&p.name))
        .filter(|p| !self.has_matching_index_signature(target, p))
        .map(|p| ExcessPropertyError { property: p.name.clone() })
        .collect()
}
```

### 4.3 Type Narrowing (Week 14)

**Problem**: Control flow analysis incomplete

**Solution**:
```rust
fn narrow_type(
    &self,
    ty: TypeId,
    narrowing_expression: &NarrowingExpr,
) -> TypeId {
    match narrowing_expression {
        NarrowingExpr::Typeof { value, type_name } => {
            self.narrow_by_typeof(ty, type_name)
        }
        NarrowingExpr::Instanceof { constructor } => {
            self.narrow_by_instanceof(ty, constructor)
        }
        NarrowingExpr::PropertyAccess { property, is_truthy } => {
            self.narrow_by_property(ty, property, *is_truthy)
        }
        NarrowingExpr::Equality { value, is_equal } => {
            self.narrow_by_equality(ty, value, *is_equal)
        }
        NarrowingExpr::In { property } => {
            self.narrow_by_in(ty, property)
        }
    }
}
```

### 4.4 Generic Constraints (Week 14-15)

**Problem**: Generic type constraints not enforced

**Solution**:
```rust
fn check_type_argument_constraints(
    &self,
    type_params: &[TypeParameter],
    type_args: &[TypeId],
) -> Result<(), Vec<ConstraintViolation>> {
    let violations: Vec<ConstraintViolation> = type_params.iter()
        .zip(type_args.iter())
        .filter_map(|(param, arg)| {
            if let Some(constraint) = param.constraint {
                if !self.is_subtype_of(*arg, constraint) {
                    Some(ConstraintViolation {
                        param: param.name.clone(),
                        constraint,
                        actual: *arg,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}
```

### 4.5 Declaration Merging (Week 15)

**Problem**: Interface/namespace merging not working

**Solution**:
```rust
fn merge_declarations(&mut self, symbols: &[SymbolId]) -> SymbolId {
    // 1. Collect all declarations for the same name
    // 2. Merge interfaces: combine members
    // 3. Merge namespaces: combine exports
    // 4. Merge class with namespace (static members)
    // 5. Return merged symbol
}
```

---

## Phase 5: Final Polish (Target: 100% conformance)

### 5.1 Error Message Accuracy (Week 16)

**Goal**: Match TypeScript error messages exactly

**Tasks**:
1. Review all error message formats
2. Add proper type formatting for error messages
3. Add related information spans
4. Match error code semantics exactly

### 5.2 Edge Case Sweep (Week 16-17)

**Goal**: Fix remaining edge cases

**Approach**:
1. Run full conformance suite
2. Categorize remaining failures
3. Fix in priority order (most impactful first)
4. Add regression tests

### 5.3 Performance Optimization (Week 17-18)

**Goal**: Ensure tests run without timeout

**Tasks**:
1. Profile slow tests
2. Add caching for expensive operations
3. Optimize type interning
4. Add early-exit fast paths

### 5.4 Final Validation (Week 18)

**Goal**: Achieve 100% pass rate

**Tasks**:
1. Run full conformance suite
2. Fix any remaining failures
3. Document any intentional differences
4. Create regression test suite

---

## Architecture Improvements

### Solver/Visitor Pattern Enhancement

The codebase already has a good foundation with `TypeVisitor` in `solver/visitor.rs`. Continue this pattern:

```rust
// Enhance TypeVisitor to handle all type operations
pub trait TypeVisitor {
    type Output;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output;
    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output;
    fn visit_object(&mut self, shape: ObjectShapeId) -> Self::Output;
    fn visit_union(&mut self, members: &[TypeId]) -> Self::Output;
    fn visit_intersection(&mut self, members: &[TypeId]) -> Self::Output;
    fn visit_function(&mut self, sig: &FunctionSignature) -> Self::Output;
    fn visit_tuple(&mut self, elements: &[TupleElement]) -> Self::Output;
    fn visit_conditional(&mut self, cond: &ConditionalType) -> Self::Output;
    fn visit_mapped(&mut self, mapped: &MappedType) -> Self::Output;
    fn visit_template_literal(&mut self, parts: &[TemplatePart]) -> Self::Output;
    fn visit_index_access(&mut self, obj: TypeId, index: TypeId) -> Self::Output;
    fn visit_type_parameter(&mut self, param: &TypeParameter) -> Self::Output;
    fn visit_reference(&mut self, symbol: SymbolRef) -> Self::Output;
    fn visit_application(&mut self, base: TypeId, args: &[TypeId]) -> Self::Output;

    // Default implementation walks the type
    fn visit(&mut self, types: &TypeInterner, type_id: TypeId) -> Self::Output {
        match types.get_key(type_id) {
            TypeKey::Intrinsic(kind) => self.visit_intrinsic(kind),
            TypeKey::Literal(value) => self.visit_literal(value),
            // ... dispatch to appropriate method
        }
    }
}
```

### Subtype Rules Organization

Current structure is good:
```
solver/subtype_rules/
├── intrinsics.rs    (338 lines)
├── literals.rs      (587 lines)
├── unions.rs        (541 lines)
├── objects.rs       (624 lines)
├── functions.rs     (996 lines)
├── tuples.rs        (379 lines)
├── generics.rs      (425 lines)
└── conditionals.rs  (133 lines)
```

### Evaluate Rules Organization

Current structure is good:
```
solver/evaluate_rules/
├── conditional.rs       (764 lines)
├── index_access.rs      (516 lines)
├── infer_pattern.rs     (2961 lines)  <- Consider breaking up
├── keyof.rs             (366 lines)
├── mapped.rs            (373 lines)
├── string_intrinsic.rs  (264 lines)
├── template_literal.rs  (198 lines)
└── apparent.rs          (132 lines)
```

**Recommendation**: Break up `infer_pattern.rs` (2961 lines) into smaller modules.

### Checker Decomposition

Continue breaking up `checker/state.rs` (currently ~13,000 lines):

```
checker/
├── state.rs              (~2,000 lines - orchestration only)
├── type_computation.rs   (existing, expand)
├── type_checking.rs      (existing, expand)
├── symbol_resolver.rs    (existing, expand)
├── error_reporter.rs     (existing, expand)
├── flow_analysis.rs      (existing, expand)
├── class_checker.rs      (new - extract class checking)
├── function_checker.rs   (new - extract function checking)
├── statement_checker.rs  (new - extract statement checking)
└── expression_checker.rs (new - extract expression checking)
```

---

## Success Metrics & Milestones

### Phase 1 Complete (Week 4)
- [ ] 0 crashes
- [ ] 0 OOM errors
- [ ] <5 timeouts
- [ ] TS2749 false positives reduced by 80%
- [ ] **40% conformance achieved**

### Phase 2 Complete (Week 8)
- [ ] TS2322 issues reduced by 70%
- [ ] TS2304/TS2307/TS2318 issues reduced by 60%
- [ ] All global types loading correctly
- [ ] **60% conformance achieved**

### Phase 3 Complete (Week 12)
- [ ] Conditional types working correctly
- [ ] Mapped types working correctly
- [ ] Template literal types working
- [ ] **80% conformance achieved**

### Phase 4 Complete (Week 15)
- [ ] Type narrowing working
- [ ] Generic constraints enforced
- [ ] Declaration merging working
- [ ] **95% conformance achieved**

### Phase 5 Complete (Week 18)
- [ ] All conformance tests passing
- [ ] Error messages match TypeScript
- [ ] Performance acceptable (<10s per test)
- [ ] **100% conformance achieved**

---

## Risk Mitigation

### Risk 1: Architectural Debt

**Mitigation**: Continue god object decomposition in parallel with conformance work. Each fix should follow the established patterns.

### Risk 2: Regression

**Mitigation**: Run conformance suite on every PR. Add unit tests for each fix.

### Risk 3: Scope Creep

**Mitigation**: Focus on error codes by priority. Don't chase edge cases until core issues are fixed.

### Risk 4: Performance

**Mitigation**: Profile regularly. Add caching where beneficial. Use depth limits to prevent runaway.

---

## Appendix: Error Code Reference

### Priority 0 (Must Fix First)
- **TS2318**: Cannot find global type 'X'
- **TS2304**: Cannot find name 'X'
- **TS2307**: Cannot find module 'X'
- **TS2749**: 'X' refers to a value, but is being used as a type

### Priority 1 (High Impact)
- **TS2322**: Type 'X' is not assignable to type 'Y'
- **TS2488**: Type 'X' must have a '[Symbol.iterator]()' method
- **TS2583**: Cannot find name 'X'. Do you need to change your target library?
- **TS2693**: 'X' only refers to a type, but is being used as a value
- **TS2339**: Property 'X' does not exist on type 'Y'
- **TS2507**: Type 'X' is not a constructor function type

### Priority 2 (Medium Impact)
- **TS18050**: The value 'X' cannot be used here
- **TS2300**: Duplicate identifier 'X'
- **TS2345**: Argument of type 'X' is not assignable to parameter of type 'Y'
- **TS2362**: The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type
- **TS1005**: 'X' expected

---

*This plan will be updated as progress is made. Each phase may be adjusted based on findings during implementation.*
