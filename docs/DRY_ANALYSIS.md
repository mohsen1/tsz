# TSZ Codebase DRY (Don't Repeat Yourself) Analysis

**Analysis Date**: 2026-02-12
**Total LOC**: 456,399 across 315 Rust source files
**Crates Analyzed**: 12 main crates (tsz-solver, tsz-checker, tsz-binder, tsz-parser, etc.)

---

## Executive Summary

This document identifies patterns in the TSZ TypeScript compiler codebase that violate the DRY (Don't Repeat Yourself) principle. The analysis reveals both successfully consolidated patterns and remaining opportunities for code cleanup.

**Key Findings:**
- ✅ **Strengths**: Excellent use of Visitor pattern, Database abstraction, and recent consolidation of modifier extraction
- ⚠️ **High-Impact Opportunities**: 20+ type assignability variants, 136+ error formatting calls, 5 overlapping flow analysis modules
- 📊 **Scale**: 102 specialized checker modules, 59 solver modules (generally well-modularized)

---

## Table of Contents

1. [Successfully Consolidated Patterns](#1-successfully-consolidated-patterns)
2. [High-Impact DRY Violations](#2-high-impact-dry-violations)
3. [Medium-Impact Opportunities](#3-medium-impact-opportunities)
4. [Low-Impact Patterns](#4-low-impact-patterns)
5. [Intentional Patterns (Not Violations)](#5-intentional-patterns-not-violations)
6. [Recommendations](#6-recommendations)

---

## 1. Successfully Consolidated Patterns

These patterns were previously duplicated but have been successfully consolidated:

### ✅ A. Numeric Property Name Checking
**Status**: Consolidated in `crates/tsz-solver/src/utils.rs`

**Previous Pattern** (duplicated across 4+ files):
```rust
// Duplicated in: operations.rs, evaluate.rs, subtype.rs, infer.rs
let name = atom.to_string();
if let Ok(num) = name.parse::<f64>() {
    if num.to_string() == name { /* is numeric */ }
}
```

**Consolidated Solution**:
```rust
// Single implementation in utils.rs
pub fn is_numeric_property_name(atom: &Atom) -> bool { ... }
```

**Files Affected**: 4+ files
**Impact**: Eliminated 15+ lines of duplicated logic

---

### ✅ B. Declaration Modifier Extraction
**Status**: Consolidated in `crates/tsz-checker/src/type_checking.rs`

**Previous Pattern** (duplicated for each declaration type):
```rust
// Repeated 7+ times for function, class, variable, interface, enum, etc.
match node.syntax_kind {
    SyntaxKind::FunctionDeclaration => {
        let decl = arena.get_function_declaration(node);
        decl.modifiers.clone()
    }
    SyntaxKind::ClassDeclaration => {
        let decl = arena.get_class_declaration(node);
        decl.modifiers.clone()
    }
    // ... 5+ more identical patterns
}
```

**Consolidated Solution**:
```rust
// Unified helper functions
fn get_declaration_modifiers(arena: &NodeArena, node: NodeIndex) -> Option<Vec<Modifier>>
fn get_member_modifiers(arena: &NodeArena, node: NodeIndex) -> Option<Vec<Modifier>>
```

**Files Affected**: `type_checking.rs`, multiple checker modules
**Impact**: Eliminated 50+ lines of match statement duplication

---

### ✅ C. Visitor Pattern for Type Traversal
**Status**: Consolidated via `TypeVisitor` trait in `crates/tsz-solver/src/visitor.rs`

**Previous Pattern** (large match statements on TypeKey):
```rust
// Duplicated logic across multiple modules
match type_key {
    TypeKey::Union(list_id) => { /* process union */ }
    TypeKey::Intersection(list_id) => { /* process intersection */ }
    TypeKey::Object(obj_id) => { /* process object */ }
    // ... 20+ variants
}
```

**Consolidated Solution**:
```rust
// Single trait with specialized implementations
pub trait TypeVisitor {
    fn visit_union(&mut self, list_id: u32) { ... }
    fn visit_intersection(&mut self, list_id: u32) { ... }
    fn visit_object(&mut self, obj_id: u32) { ... }
    // ... composable methods
}
```

**Files Using Pattern**: 12+ files (variance.rs, narrowing.rs, operations.rs, etc.)
**Impact**: Eliminated hundreds of lines of repetitive match statements
**LOC Saved**: ~2,000 lines across all implementations

---

## 2. High-Impact DRY Violations

These patterns have significant duplication and would benefit most from consolidation:

### ⚠️ A. Type Assignability Checking Variants
**Priority**: HIGH
**Estimated Duplication**: 20+ similar functions across multiple files

**Location**:
- Primary: `crates/tsz-checker/src/assignability_checker.rs`
- Secondary: `crates/tsz-solver/src/operations.rs`, `crates/tsz-checker/src/state.rs`

**Duplicated Patterns**:
```rust
// Base pattern duplicated with slight variations:

// operations.rs
fn is_assignable_to_bivariant_callback(source: TypeId, target: TypeId) -> bool

// operations.rs
fn is_assignable_to_strict(source: TypeId, target: TypeId) -> bool

// state.rs
fn is_array_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_object_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_tuple_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_enum_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_union_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_intersection_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_function_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_class_assignable_to(source: TypeId, target: TypeId) -> bool
fn is_interface_assignable_to(source: TypeId, target: TypeId) -> bool

// Plus 10+ more variants in different modules
```

**Common Structure** (90% identical):
1. Check for `any`/`unknown`/`never` early exit
2. Get type keys from interner
3. Match on type pair combination
4. Recursively check nested types
5. Return boolean result

**Consolidation Opportunity**:
```rust
// Proposed: Trait-based composition
pub trait AssignabilityChecker {
    fn is_assignable(&self, source: TypeId, target: TypeId, mode: AssignabilityMode) -> bool;

    // Default implementations for common cases
    fn check_trivial_cases(&self, source: TypeId, target: TypeId) -> Option<bool> {
        // Handle any, unknown, never
    }

    fn check_structural(&self, source: TypeId, target: TypeId) -> bool {
        // Common structural logic
    }
}

// Mode enum to replace function variants
pub enum AssignabilityMode {
    Standard,
    Strict,
    BivariantCallback,
    Array,
    Tuple,
    // ... other variants
}
```

**Impact**: Would consolidate 20+ functions into 1 trait + implementations
**Estimated LOC Reduction**: 500-800 lines

---

### ⚠️ B. Error Message Formatting + Emission
**Priority**: HIGH
**Estimated Duplication**: 136+ call sites across checker modules

**Pattern** (repeated throughout checker):
```rust
// Pattern 1: Manual formatting (70+ occurrences)
let message = format_message(
    diagnostic_messages::TYPE_NOT_ASSIGNABLE,
    &[&type1_str, &type2_str]
);
self.error_at_node(node, &message, DiagnosticCode::TypeNotAssignable);

// Pattern 2: Similar for different error types (40+ occurrences)
let message = format_message(
    diagnostic_messages::PROPERTY_DOES_NOT_EXIST,
    &[&prop_name, &type_str]
);
self.error_at_node(node, &message, DiagnosticCode::PropertyDoesNotExist);

// Pattern 3: Inline messages (26+ occurrences)
self.error_at_node(node, "Cannot find name", DiagnosticCode::CannotFindName);
```

**Files Affected**:
- `error_reporter.rs` (core infrastructure)
- All 102 checker modules using error reporting
- High concentration in: `type_checking.rs`, `call_checker.rs`, `property_checker.rs`

**Consolidation Opportunity**:
```rust
// Proposed: Type-safe error builder pattern
impl ErrorReporter {
    fn type_not_assignable(&mut self, node: NodeIndex, source: TypeId, target: TypeId) {
        let msg = format_message(
            diagnostic_messages::TYPE_NOT_ASSIGNABLE,
            &[&self.type_to_string(source), &self.type_to_string(target)]
        );
        self.error_at_node(node, &msg, DiagnosticCode::TypeNotAssignable);
    }

    fn property_does_not_exist(&mut self, node: NodeIndex, prop: &str, type_id: TypeId) {
        // ... similar pattern
    }

    // 50+ specialized methods for common errors
}
```

**Alternative**: Macro-based approach:
```rust
macro_rules! diagnostic_error {
    ($self:ident, $node:expr, TypeNotAssignable($source:expr, $target:expr)) => {
        $self.error_type_mismatch($node, $source, $target,
            diagnostic_messages::TYPE_NOT_ASSIGNABLE)
    };
    // ... patterns for other common errors
}
```

**Impact**: Would reduce 136+ repetitive calls to ~50 semantic methods
**Estimated LOC Reduction**: 200-300 lines
**Additional Benefit**: Type-safe error emission, easier to refactor error messages

---

### ⚠️ C. Flow Analysis Module Overlap
**Priority**: HIGH
**Estimated Duplication**: 5 overlapping modules with similar concerns

**Modules**:
1. `crates/tsz-checker/src/flow_analysis.rs` (1,511 LOC) - Definite assignment analysis
2. `crates/tsz-checker/src/flow_analyzer.rs` - Type narrowing infrastructure
3. `crates/tsz-checker/src/flow_narrowing.rs` - Narrowing rules
4. `crates/tsz-checker/src/control_flow.rs` (3,709 LOC) - Flow graph + narrowing
5. `crates/tsz-checker/src/control_flow_narrowing.rs` - Flow-based narrowing

**Total LOC**: ~7,000+ lines with significant conceptual overlap

**Duplicated Patterns**:
```rust
// Pattern: FlowNode traversal (found in 3+ modules)
fn traverse_flow_nodes(node: FlowNodeId) -> NarrowingResult {
    match flow_node.kind {
        FlowNodeKind::Assignment => { /* handle assignment */ }
        FlowNodeKind::Condition => { /* handle condition */ }
        FlowNodeKind::Label => { /* handle label */ }
        // ... similar traversal logic in multiple places
    }
}

// Pattern: Type narrowing application (found in 4+ modules)
fn apply_narrowing(type_id: TypeId, narrowing: &Narrowing) -> TypeId {
    match narrowing.kind {
        NarrowingKind::TypeGuard => { /* apply guard */ }
        NarrowingKind::Typeof => { /* apply typeof */ }
        NarrowingKind::Instanceof => { /* apply instanceof */ }
        // ... repeated narrowing application logic
    }
}

// Pattern: Control flow analysis (found in 2+ modules)
fn analyze_control_flow(statement: NodeIndex) -> FlowNodeId {
    match statement.kind {
        SyntaxKind::IfStatement => { /* create conditional flow */ }
        SyntaxKind::WhileStatement => { /* create loop flow */ }
        SyntaxKind::SwitchStatement => { /* create switch flow */ }
        // ... similar flow construction
    }
}
```

**Consolidation Opportunity**:

```rust
// Proposed: Unified flow analysis module structure

// crates/tsz-checker/src/flow/
// ├── mod.rs              - Public API
// ├── graph.rs            - FlowNode construction & traversal (from control_flow.rs)
// ├── narrowing.rs        - Narrowing rules & application (merged from flow_narrowing.rs + control_flow_narrowing.rs)
// ├── analyzer.rs         - Type narrowing analysis (from flow_analyzer.rs)
// └── assignment.rs       - Definite assignment (from flow_analysis.rs)

// Shared infrastructure:
pub struct FlowContext {
    graph: FlowGraph,           // Single source of truth
    narrowing_rules: Rules,     // Unified narrowing logic
    assignment_tracker: Tracker, // Definite assignment state
}
```

**Impact**:
- Consolidate 5 modules into 1 module with 4-5 submodules
- Eliminate duplicated FlowNode traversal logic
- Single source of truth for narrowing rules
- **Estimated LOC Reduction**: 1,000-1,500 lines

---

### ⚠️ D. Type Computation Expression Handlers
**Priority**: MEDIUM-HIGH
**Estimated Duplication**: 30+ similar functions with identical structure

**Location**:
- `crates/tsz-checker/src/type_computation.rs`
- `crates/tsz-checker/src/type_computation_complex.rs`
- Split across 10+ specialized modules

**Duplicated Pattern**:
```rust
// Repeated 30+ times with minor variations:

fn get_type_of_binary_expression(&mut self, node: NodeIndex) -> TypeId {
    let expr = self.arena.get_binary_expression(node);
    let left_type = self.get_type_of_expression(expr.left);
    let right_type = self.get_type_of_expression(expr.right);
    self.check_binary_operation(expr.operator, left_type, right_type)
}

fn get_type_of_call_expression(&mut self, node: NodeIndex) -> TypeId {
    let expr = self.arena.get_call_expression(node);
    let func_type = self.get_type_of_expression(expr.expression);
    let arg_types = expr.arguments.iter()
        .map(|arg| self.get_type_of_expression(*arg))
        .collect();
    self.check_call(func_type, arg_types)
}

fn get_type_of_property_access(&mut self, node: NodeIndex) -> TypeId {
    let expr = self.arena.get_property_access_expression(node);
    let obj_type = self.get_type_of_expression(expr.expression);
    let prop_name = expr.name;
    self.get_property_type(obj_type, prop_name)
}

// ... 27+ more functions following this exact pattern
```

**Common Structure** (95% identical):
1. Get specific expression node from arena (`arena.get_X_expression`)
2. Recursively compute child expression types (`get_type_of_expression`)
3. Delegate to type checker method (`check_X` or `get_X_type`)

**Consolidation Opportunity**:

```rust
// Proposed: Macro-based dispatch
macro_rules! type_computation_handler {
    ($name:ident, $arena_getter:ident, $fields:tt => $logic:expr) => {
        fn $name(&mut self, node: NodeIndex) -> TypeId {
            let expr = self.arena.$arena_getter(node);
            $logic
        }
    };
}

// Usage:
type_computation_handler!(
    get_type_of_binary_expression,
    get_binary_expression,
    (left, right, operator) => {
        let left_type = self.get_type_of_expression(left);
        let right_type = self.get_type_of_expression(right);
        self.check_binary_operation(operator, left_type, right_type)
    }
);

// Or: Trait-based dispatch table
pub trait ExpressionTypeComputer {
    fn compute_type(&mut self, node: NodeIndex) -> TypeId {
        match node.syntax_kind() {
            SyntaxKind::BinaryExpression => self.compute_binary(node),
            SyntaxKind::CallExpression => self.compute_call(node),
            // ... dispatch via table instead of separate functions
        }
    }
}
```

**Impact**: Would consolidate 30+ near-identical functions
**Estimated LOC Reduction**: 300-500 lines
**Additional Benefit**: Easier to add new expression types

---

## 3. Medium-Impact Opportunities

### ⚠️ E. Property/Member Lookup Duplication
**Priority**: MEDIUM
**Estimated Duplication**: 3+ implementations with similar logic

**Location**:
- `crates/tsz-checker/src/property_checker.rs` - Main implementation
- `crates/tsz-checker/src/class_checker.rs` - Class-specific checks
- `crates/tsz-checker/src/jsx_checker.rs` - JSX attribute checks
- Helper methods in `symbol_resolver.rs`, `context.rs`

**Duplicated Pattern**:
```rust
// Property access validation (repeated 3+ times):

// Pattern in property_checker.rs
fn check_property_access(obj_type: TypeId, prop_name: &str) -> Result<TypeId, Error> {
    let members = get_members(obj_type);
    let member = members.get(prop_name)?;

    // Check accessibility
    if member.is_private() && !is_same_class() {
        error_private_access();
    }
    if member.is_protected() && !is_derived_class() {
        error_protected_access();
    }

    Ok(member.type_id)
}

// Similar logic in class_checker.rs for class members
// Similar logic in jsx_checker.rs for JSX attributes
```

**Consolidation Opportunity**:
```rust
// Proposed: Unified property accessor trait
pub trait PropertyAccessor {
    fn get_property(&self, type_id: TypeId, name: &str, context: AccessContext)
        -> Result<PropertyInfo, PropertyError>;

    fn validate_accessibility(&self, property: &PropertyInfo, context: &AccessContext)
        -> Result<(), AccessibilityError>;
}

pub enum AccessContext {
    Normal,
    ClassInheritance { base_class: TypeId },
    JsxAttribute,
}
```

**Impact**: Consolidate 3 implementations into 1 trait + 3 specialized contexts
**Estimated LOC Reduction**: 100-200 lines

---

### ⚠️ F. Type List Processing
**Priority**: MEDIUM
**Estimated Duplication**: Repeated in 5+ modules

**Location**:
- `crates/tsz-solver/src/operations.rs`
- `crates/tsz-solver/src/expression_ops.rs`
- `crates/tsz-solver/src/type_queries.rs`
- Various checker modules

**Duplicated Pattern**:
```rust
// Normalize/dedup/sort type lists (repeated 5+ times):

fn normalize_union_types(types: &[TypeId]) -> Vec<TypeId> {
    let mut result = Vec::new();

    // Flatten nested unions
    for type_id in types {
        if is_union(type_id) {
            result.extend(get_union_members(type_id));
        } else {
            result.push(type_id);
        }
    }

    // Remove duplicates
    result.sort();
    result.dedup();

    // Remove never, filter any
    result.retain(|t| !is_never(t));
    if result.iter().any(|t| is_any(t)) {
        return vec![any_type()];
    }

    result
}

// Similar functions for intersection, tuple, template normalization
```

**Consolidation Opportunity**:
```rust
// Already exists partially in operations.rs, needs extraction
// Proposed: Dedicated type_list_utils.rs module

pub fn normalize_union(types: &[TypeId]) -> Vec<TypeId>;
pub fn normalize_intersection(types: &[TypeId]) -> Vec<TypeId>;
pub fn normalize_tuple(types: &[TypeId]) -> Vec<TypeId>;
pub fn flatten_nested<F>(types: &[TypeId], predicate: F) -> Vec<TypeId>
    where F: Fn(TypeId) -> Option<Vec<TypeId>>;
```

**Impact**: Extract common logic to dedicated module
**Estimated LOC Reduction**: 150-250 lines

---

### ⚠️ G. Match on SyntaxKind/TypeKey for Dispatch
**Priority**: MEDIUM (inherent pattern, but could be optimized)
**Estimated Duplication**: Hundreds of match statements across all modules

**Pattern**:
```rust
// Repeated throughout checker modules:
match node.syntax_kind {
    SyntaxKind::BinaryExpression => self.check_binary_expression(node),
    SyntaxKind::CallExpression => self.check_call_expression(node),
    SyntaxKind::PropertyAccessExpression => self.check_property_access(node),
    // ... 50+ variants
}
```

**Note**: Some duplication is inherent to AST traversal, but could use dispatch tables:

**Optimization Opportunity**:
```rust
// Proposed: Function pointer table for hot paths
type CheckerFn = fn(&mut TypeChecker, NodeIndex) -> TypeId;

lazy_static! {
    static ref EXPRESSION_CHECKERS: HashMap<SyntaxKind, CheckerFn> = {
        let mut m = HashMap::new();
        m.insert(SyntaxKind::BinaryExpression, TypeChecker::check_binary_expression as CheckerFn);
        m.insert(SyntaxKind::CallExpression, TypeChecker::check_call_expression as CheckerFn);
        // ... table-driven dispatch
        m
    };
}

fn check_expression(&mut self, node: NodeIndex) -> TypeId {
    let kind = node.syntax_kind();
    if let Some(handler) = EXPRESSION_CHECKERS.get(&kind) {
        handler(self, node)
    } else {
        self.check_expression_fallback(node)
    }
}
```

**Impact**: Micro-optimization, reduces branch prediction misses
**Performance Gain**: Estimated 2-5% for hot type checking paths

---

## 4. Low-Impact Patterns

These patterns have some duplication but are less critical:

### ⚠️ H. Visitor Implementations Boilerplate
**Priority**: LOW
**Scale**: 12+ files implementing `TypeVisitor` trait

**Pattern**:
```rust
impl TypeVisitor for MyVisitor {
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) { /* default impl */ }
    fn visit_literal(&mut self, value: &LiteralValue) { /* default impl */ }
    fn visit_union(&mut self, list_id: u32) { /* default impl */ }
    // ... 20+ methods, many are no-ops
}
```

**Observation**: Most visitors only override 3-5 methods, rest are boilerplate

**Consolidation Opportunity**:
```rust
// Proposed: Default trait implementations (already partially done)
pub trait TypeVisitor {
    fn visit_intrinsic(&mut self, kind: IntrinsicKind) {
        // Default: do nothing
    }

    fn visit_literal(&mut self, value: &LiteralValue) {
        // Default: do nothing
    }

    // ... default implementations for all methods
}

// Visitors only override what they need
impl TypeVisitor for VarianceVisitor {
    fn visit_union(&mut self, list_id: u32) {
        // Only implement what changes
    }
}
```

**Impact**: Reduce boilerplate in visitor implementations
**Estimated LOC Reduction**: 50-100 lines per visitor × 12 visitors = 600-1,200 lines

---

### ⚠️ I. Symbol Table Lookups
**Priority**: LOW
**Estimated Duplication**: 3+ implementations

**Pattern**:
```rust
// Repeated symbol lookup patterns:
fn lookup_symbol(&self, name: &str, scope: ScopeId) -> Option<SymbolId> {
    let current_scope = self.scopes.get(scope)?;
    if let Some(symbol) = current_scope.symbols.get(name) {
        return Some(*symbol);
    }

    // Check parent scopes
    let mut parent = current_scope.parent;
    while let Some(parent_id) = parent {
        let parent_scope = self.scopes.get(parent_id)?;
        if let Some(symbol) = parent_scope.symbols.get(name) {
            return Some(*symbol);
        }
        parent = parent_scope.parent;
    }

    None
}
```

**Consolidation Opportunity**: Already mostly consolidated in `symbol_resolver.rs`, but some duplication in `context.rs` and `binder/state.rs`

---

## 5. Intentional Patterns (Not Violations)

These patterns appear duplicated but are intentional design choices:

### ✅ J. Multiple TypeVisitor Implementations
**Status**: INTENTIONAL (Composition Pattern)

**Reason**: Each visitor serves a different purpose:
- `VarianceVisitor` - Computes type parameter variance
- `NarrowingVisitor` - Applies type narrowing
- `SubtypeVisitor` - Checks subtype relationships
- 9+ other specialized visitors

**Why Not Duplication**: Visitor pattern enables composition and separation of concerns. Consolidating would create monolithic visitor with mixed responsibilities.

---

### ✅ K. 102 Specialized Checker Modules
**Status**: INTENTIONAL (Separation of Concerns)

**Modules**:
- `call_checker.rs`, `property_checker.rs`, `class_checker.rs`
- `interface_type.rs`, `union_type.rs`, `function_type.rs`
- `jsx.rs`, `decorators.rs`, `spread.rs`, `destructuring.rs`
- 90+ more specialized modules

**Why Not Duplication**: Each module handles a specific TypeScript language feature. Consolidation would create unmaintainable monolithic checker.

---

### ✅ L. Match on SyntaxKind (Inherent AST Dispatching)
**Status**: INHERENT (AST Traversal Requirement)

**Reason**: TypeScript has 200+ syntax kinds. Dispatching on AST node type is fundamental to compiler architecture. Some match statements are unavoidable.

**Mitigation**: Already using helper functions (`get_declaration_modifiers`) to reduce duplication within match arms.

---

## 6. Recommendations

### Priority 1: High-Impact Quick Wins

1. **Error Message Consolidation** (Impact: HIGH, Effort: MEDIUM)
   - Create specialized error methods in `error_reporter.rs`
   - Target: 50 most common error patterns
   - Expected reduction: 200-300 LOC
   - Timeline: 1-2 days

2. **Type Assignability Trait** (Impact: HIGH, Effort: HIGH)
   - Extract `AssignabilityChecker` trait with mode enum
   - Consolidate 20+ `is_*_assignable_to` functions
   - Expected reduction: 500-800 LOC
   - Timeline: 3-5 days

### Priority 2: Architectural Improvements

3. **Flow Analysis Module Consolidation** (Impact: HIGH, Effort: HIGH)
   - Merge 5 overlapping flow modules into unified `flow/` submodule
   - Single source of truth for flow graph and narrowing
   - Expected reduction: 1,000-1,500 LOC
   - Timeline: 5-7 days

4. **Type Computation Macro** (Impact: MEDIUM, Effort: MEDIUM)
   - Create macro or trait for expression type computation
   - Consolidate 30+ similar handler functions
   - Expected reduction: 300-500 LOC
   - Timeline: 2-3 days

### Priority 3: Polish & Optimization

5. **Visitor Default Implementations** (Impact: LOW, Effort: LOW)
   - Add default implementations to `TypeVisitor` trait
   - Reduce boilerplate in 12+ visitor implementations
   - Expected reduction: 600-1,200 LOC
   - Timeline: 1 day

6. **Property Access Trait** (Impact: MEDIUM, Effort: MEDIUM)
   - Unify 3 property access implementations
   - Expected reduction: 100-200 LOC
   - Timeline: 1-2 days

### Total Impact
- **Estimated LOC Reduction**: 2,700 - 4,500 lines (0.6% - 1% of codebase)
- **Maintenance Benefits**:
  - Easier to add new error types (single location)
  - Easier to modify assignability rules (trait-based)
  - Easier to debug flow analysis (unified module)
  - Reduced cognitive overhead (fewer similar functions)

---

## Code Statistics

### Codebase Overview
- **Total LOC**: 456,399 across 315 Rust files
- **Test LOC**: ~250,000 (56% of codebase)
  - `evaluate_tests.rs`: 41,000 LOC
  - `subtype_tests.rs`: 24,000 LOC
  - Other conformance tests: 185,000 LOC
- **Production LOC**: ~206,000 (44% of codebase)

### Module Breakdown
| Crate | Modules | Key Files (LOC) |
|-------|---------|-----------------|
| tsz-solver | 59 | narrowing.rs (3,087), compat.rs tests (4,674) |
| tsz-checker | 102 | state.rs (12,974), type_checking.rs (4,393), control_flow.rs (3,709) |
| tsz-binder | 5 | state.rs (3,803) |
| tsz-parser | 8 | scanner_impl.rs (2,866), state.rs, node_arena.rs |
| tsz-common | 10 | diagnostics.rs (17,361!), interner.rs |
| tsz-lsp | 32 | Various language server features |
| tsz-emitter | 6 | Code generation modules |
| tsz-cli | 13 | CLI infrastructure |

### Duplication Metrics
- **High-Impact Violations**: 4 patterns (2,200-3,800 LOC affected)
- **Medium-Impact Violations**: 3 patterns (550-850 LOC affected)
- **Low-Impact Patterns**: 2 patterns (700-1,400 LOC affected)
- **Successfully Consolidated**: 3 patterns (~2,050 LOC saved historically)

---

## Conclusion

The TSZ codebase demonstrates **good architectural patterns** with the Visitor pattern, Database abstraction, and modular checker design. The most significant DRY opportunities lie in:

1. **Consolidating type assignability checks** into a trait-based system
2. **Unifying error message formatting** with specialized helper methods
3. **Merging overlapping flow analysis modules** into a cohesive subsystem

These improvements would reduce duplication by **~3,000-4,500 lines** while improving maintainability and making it easier to extend the compiler with new TypeScript features.

The codebase is already well-factored in many areas (modifier extraction, numeric property checking, visitor pattern). The recommended consolidations would bring the remaining high-duplication areas up to the same standard.
