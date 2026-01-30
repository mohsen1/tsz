# Phase 2: Type System Foundation - Implementation Tasks

**Duration:** 3 weeks (15 business days)
**Team Size:** 2-3 developers
**Goal:** Fix type checker gaps that limit LSP accuracy
**Prerequisites:** Phase 1 complete
**Expected Outcome:** Accurate hover/completions in narrowed contexts

---

## Overview

Based on [Team 5's research](../research-findings/team-5-type-checker-gaps.md), this phase addresses the 4 critical type checker gaps that prevent LSP features from showing accurate types:

| Priority | Gap | Effort | LSP Impact | Days |
|----------|-----|--------|------------|------|
| **Tier 1** | Control Flow Narrowing API | 3-5 days | Fixes hover + completions | 1-5 |
| **Tier 1** | Definite Assignment Analysis | 5-7 days | Enables diagnostics | 6-12 |
| **Tier 2** | TDZ Checking | 6-9 days | Filters completions | 13-21 |
| **Tier 2** | Module Resolution | 4-6 days | Cross-file features | 22-27 |

**Total Effort:** 27 days (5.5 weeks with parallelization)

---

## Task 1: Control Flow Narrowing API

**Effort:** 3-5 days
**Impact:** ⭐⭐⭐⭐⭐ (Highest LSP impact)
**Research:** [Team 5 Report](../research-findings/team-5-type-checker-gaps.md) - Section "Tier 1 Critical Gaps"
**Files:** `src/checker/flow_narrowing.rs`, `src/checker/state.rs`

### Background

**Problem:** Hover shows declared type, not narrowed type at cursor position.

```typescript
function foo(x: string | null) {
    if (x !== null) {
        // User hovers over 'x'
        // Expected: "string"
        // Actual:   "string | null"  ❌
    }
}
```

**Root Cause:** No API to query narrowed type at location.

### Implementation

**Step 1: Add `get_type_at_location()` API** (Day 1-2, ~150 lines)

```rust
// src/checker/state.rs
impl CheckerState {
    /// Get the narrowed type of a node at its location in the code.
    /// This considers flow-sensitive type guards and conditionals.
    pub fn get_type_at_location(&self, node_idx: NodeIndex) -> Option<TypeId> {
        // 1. Find FlowNode for this AST node
        let flow_node = self.flow_graph.get_node_for_ast(node_idx)?;

        // 2. Traverse flow graph backwards to root
        let mut current_flow = flow_node;
        let mut narrowed_type = self.get_declared_type(node_idx)?;

        while let Some(parent) = self.flow_graph.get_parent(current_flow) {
            // 3. Apply type guards at each flow node
            if let Some(guard) = parent.get_type_guard() {
                narrowed_type = guard.apply_to(narrowed_type);
            }

            current_flow = parent;
        }

        // 4. Return narrowed type or fall back to declared
        Some(narrowed_type)
    }
}
```

**Step 2: Implement type guard application** (Day 2-3, ~200 lines)

```rust
// src/checker/flow_narrowing.rs
impl TypeGuard {
    pub fn apply_to(&self, base_type: TypeId) -> TypeId {
        match self {
            TypeGuard::NotNull => {
                // Remove 'null' and 'undefined' from union
                self.remove_from_union(base_type, &[TypeId::NULL, TypeId::UNDEFINED])
            }
            TypeGuard::TypeOf(expected_type) => {
                // Narrow to type matching typeof check
                self.narrow_by_typeof(base_type, expected_type)
            }
            TypeGuard::Discriminant(property, value) => {
                // Narrow discriminated union
                self.narrow_discriminated_union(base_type, property, value)
            }
            TypeGuard::Truthiness => {
                // Remove falsy values from union
                self.remove_falsy_from_union(base_type)
            }
        }
    }

    fn remove_from_union(&self, base_type: TypeId, types_to_remove: &[TypeId]) -> TypeId {
        if let TypeKind::Union(members) = self.get_type_kind(base_type) {
            let filtered: Vec<TypeId> = members
                .iter()
                .filter(|t| !types_to_remove.contains(t))
                .copied()
                .collect();

            if filtered.is_empty() {
                return TypeId::NEVER;
            }

            if filtered.len() == 1 {
                return filtered[0];
            }

            return self.interner.intern_union(filtered);
        }

        base_type
    }
}
```

**Step 3: Integrate with LSP** (Day 3-4, ~50 lines)

```rust
// src/lsp/hover.rs
impl<'a> HoverProvider<'a> {
    fn get_hover_content(&self, node_idx: NodeIndex) -> Option<String> {
        // OLD: Always shows declared type
        // let type_id = self.checker.get_type_of_node(node_idx)?;

        // NEW: Shows narrowed type at location
        let type_id = self.checker.get_type_at_location(node_idx)
            .or_else(|| self.checker.get_type_of_node(node_idx))?;

        Some(self.format_type(type_id))
    }
}
```

**Step 4: Add tests** (Day 4-5, ~150 lines)

```rust
// src/checker/tests/flow_narrowing_tests.rs
#[test]
fn test_narrowing_not_null() {
    let source = r#"
        function foo(x: string | null) {
            if (x !== null) {
                x; // hover should show "string"
            }
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    let x_ref = find_node_at(source, "x; // hover");
    let type_at_loc = checker.get_type_at_location(x_ref).unwrap();

    assert_eq!(type_at_loc, TypeId::STRING);
}

#[test]
fn test_narrowing_typeof() {
    let source = r#"
        function foo(x: unknown) {
            if (typeof x === "string") {
                x; // should show "string"
            }
        }
    "#;

    // ... similar test ...
}

#[test]
fn test_narrowing_discriminant() {
    let source = r#"
        type Shape =
            | { kind: "circle", radius: number }
            | { kind: "square", side: number };

        function area(shape: Shape) {
            if (shape.kind === "circle") {
                shape; // should show { kind: "circle", radius: number }
            }
        }
    "#;

    // ... similar test ...
}
```

### Acceptance Criteria

- ✅ `get_type_at_location()` API works for all narrowing types
- ✅ Hover shows narrowed type in `if` blocks
- ✅ Completions show narrowed members in guarded blocks
- ✅ No regressions in existing type checking
- ✅ Performance: <5ms per query (with caching)
- ✅ Test coverage: 15+ test cases

### Impact

After this task:
- Hover shows "string" instead of "string | null" in `if (x !== null)` blocks
- Completions show `.toCharArray()` but not `.toFixed()` on narrowed strings
- User sees accurate types throughout their code

---

## Task 2: Definite Assignment Analysis

**Effort:** 5-7 days
**Impact:** ⭐⭐⭐⭐
**Research:** [Team 5 Report](../research-findings/team-5-type-checker-gaps.md) - Section "Definite Assignment Analysis"
**Files:** `src/checker/definite_assignment.rs` (new), `src/checker/state.rs`

### Background

**Problem:** `is_definitely_assigned_at()` always returns `true` (stubbed).

```typescript
function foo() {
    let x: number;
    // x is not definitely assigned here
    console.log(x);  // Should error: TS2454
}
```

**Root Cause:** No flow-sensitive assignment tracking.

### Implementation

**Step 1: Design assignment tracking** (Day 1, ~50 lines)

```rust
// src/checker/definite_assignment.rs
pub struct DefiniteAssignmentAnalyzer {
    /// Track "definitely assigned" status for each variable on each control flow path
    assigned_on_entry: FxHashMap<NodeIndex, FxHashSet<SymbolId>>,
    assigned_on_exit: FxHashMap<NodeIndex, FxHashSet<SymbolId>>,
}

pub enum AssignmentStatus {
    DefinitelyAssigned,
    MaybeAssigned,
    NotDefinitelyAssigned,
}
```

**Step 2: Implement forward flow analysis** (Day 2-4, ~400 lines)

```rust
impl DefiniteAssignmentAnalyzer {
    pub fn analyze(&mut self, checker: &CheckerState, root: NodeIndex) {
        // For each block/statement:
        // 1. Determine which variables are assigned on entry
        // 2. Track assignments within the block
        // 3. At merge points (if/else join, loops), INTERSECT assigned sets
        // 4. Use fixpoint iteration for loops

        self.analyze_function(checker, root);
    }

    fn analyze_block(&mut self, checker: &CheckerState, block: NodeIndex) {
        let mut assigned = self.get_assigned_on_entry(block);

        for statement in self.get_statements(block) {
            match statement.kind {
                NodeKind::VariableDeclaration => {
                    // Check if has initializer
                    if let Some(init) = checker.get_initializer(statement) {
                        assigned.insert(checker.get_symbol(statement));
                    }
                }
                NodeKind::AssignmentExpression => {
                    // Track LHS symbol as assigned
                    let lhs = checker.get_assignment_target(statement);
                    assigned.insert(checker.get_symbol(lhs));
                }
                NodeKind::IfStatement => {
                    // Analyze both branches, then merge assigned sets
                    let then_assigned = self.analyze_branch(checker, block.then_branch);
                    let else_assigned = self.analyze_branch(checker, block.else_branch);

                    // INTERSECT at merge point
                    assigned = then_assigned.intersection(&else_assigned).copied().collect();
                }
                // ... handle other statement types ...
            }
        }

        self.set_assigned_on_exit(block, assigned);
    }

    fn analyze_loop(&mut self, checker: &CheckerState, loop_node: NodeIndex) {
        // Use fixpoint iteration:
        // 1. Start with all variables assigned (optimistic)
        // 2. Iterate through loop body
        // 3. Update assigned sets
        // 4. Repeat until assigned sets stop changing
        let mut assigned = self.get_assigned_on_entry(loop_node);
        let mut changed = true;

        while changed {
            let old_assigned = assigned.clone();
            self.analyze_block(checker, loop_node.body);
            let new_assigned = self.get_assigned_on_exit(loop_node.body);

            // Merge with entry set (variables must be assigned on all paths)
            assigned = old_assigned.intersection(&new_assigned).copied().collect();

            changed = (assigned != old_assigned);
        }
    }
}
```

**Step 3: Integrate with checker** (Day 5, ~50 lines)

```rust
// src/checker/state.rs
impl CheckerState {
    pub fn is_definitely_assigned_at(&self, node_idx: NodeIndex) -> bool {
        // Before: return true; (stubbed)

        // After:
        let symbol = self.get_symbol_of_node(node_idx)?;
        let location = self.get_location(node_idx);

        self.definite_assignment
            .is_assigned_at(symbol, location)
            .unwrap_or(true)
    }

    pub fn check_variable_use(&mut self, node_idx: NodeIndex) {
        let symbol = self.get_symbol_of_node(node_idx)?;

        if !self.is_definitely_assigned_at(node_idx) {
            self.error(
                node_idx,
                "TS2454",
                format!("Variable '{}' is used before being assigned", symbol.name),
            );
        }
    }
}
```

**Step 4: Add diagnostics** (Day 5-6, ~50 lines)

```rust
// src/checker/diagnostics.rs
impl CheckerState {
    fn report_undefinite_assignment_errors(&mut self) {
        for (node, symbol) in self.find_variable_uses() {
            if !self.is_definitely_assigned_at(node) {
                self.emit_error(Diagnostic {
                    range: self.get_range(node),
                    code: "TS2454",
                    message: format!("Variable '{}' is used before being assigned", symbol.name),
                });
            }
        }
    }
}
```

**Step 5: Add tests** (Day 6-7, ~200 lines)

```rust
// src/checker/tests/definite_assignment_tests.rs
#[test]
fn test_variable_before_assignment() {
    let source = r#"
        function foo() {
            let x: number;
            console.log(x);  // ERROR: x used before assignment
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 1);
    assert_eq!(checker.errors[0].code, "TS2454");
}

#[test]
fn test_variable_after_assignment() {
    let source = r#"
        function foo() {
            let x: number;
            x = 5;
            console.log(x);  // OK: x is definitely assigned
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 0);
}

#[test]
fn test_conditional_assignment() {
    let source = r#"
        function foo(condition: boolean) {
            let x: number;
            if (condition) {
                x = 5;
            }
            console.log(x);  // ERROR: x not assigned on all paths
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 1);
}

#[test]
fn test_loop_assignment() {
    let source = r#"
        function foo() {
            let x: number;
            for (let i = 0; i < 10; i++) {
                x = i;
            }
            console.log(x);  // OK: x assigned on loop exit
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 0);
}
```

### Acceptance Criteria

- ✅ `is_definitely_assigned_at()` correctly tracks assignments
- ✅ TS2454 errors reported for variables used before assignment
- ✅ No false positives (variables reported as unassigned when they are)
- ✅ Handles all control flow constructs (if/else, loops, try/catch)
- ✅ Performance: <50ms for 1000-line functions
- ✅ Test coverage: 20+ test cases

### Impact

After this task:
- Runtime errors caught at compile time
- "Initialize variable" code actions available
- Improved user trust in type checker

---

## Task 3: TDZ Checking

**Effort:** 6-9 days
**Impact:** ⭐⭐⭐
**Research:** [Team 5 Report](../research-findings/team-5-type-checker-gaps.md) - Section "TDZ Checking"
**Files:** `src/checker/tdz.rs` (new), `src/checker/state.rs`

### Background

**Problem:** Three TDZ implementations return `false` (not implemented).

```typescript
// Temporal Dead Zone violations not detected:
console.log(x);  // Should error: x is in TDZ
let x = 5;

const obj = {
    [y]: 5,  // Should error: y is in TDZ
};
const y = "key";

class A extends (B, C) {}  // Should error: B is in TDZ
class B {}
class C {}
```

### Implementation

**Step 1: Implement `is_in_tdz_static_block`** (Day 1-2, ~100 lines)

```rust
// src/checker/tdz.rs
impl CheckerState {
    pub fn is_in_tdz_static_block(&self, node_idx: NodeIndex) -> bool {
        // Check if node is in static block and references a class member
        // that hasn't been initialized yet.

        let symbol = self.get_symbol_of_node(node_idx)?;

        // Find containing static block
        let static_block = self.find_containing_static_block(node_idx)?;

        // Find all variable declarations in the static block
        let declarations = self.get_declarations_in_order(static_block);

        // Check if symbol is declared AFTER the reference
        let ref_position = self.get_position(node_idx);
        let decl_position = self.get_declaration_position(symbol)?;

        decl_position > ref_position
    }
}
```

**Step 2: Implement `is_in_tdz_computed_property`** (Day 3-4, ~150 lines)

```rust
impl CheckerState {
    pub fn is_in_tdz_computed_property(&self, node_idx: NodeIndex) -> bool {
        // Check if computed property key references a variable
        // that hasn't been initialized yet.

        let property_key = self.get_property_key(node_idx)?;

        // Must be an expression (not a literal)
        if !self.is_expression(property_key) {
            return false;
        }

        let symbol = self.get_referenced_symbol(property_key)?;

        // Find containing object literal
        let obj_literal = self.find_containing_object_literal(node_idx)?;

        // Check if symbol is declared AFTER the object literal
        let obj_literal_position = self.get_position(obj_literal);
        let symbol_decl_position = self.get_declaration_position(symbol)?;

        symbol_decl_position > obj_literal_position
    }
}
```

**Step 3: Implement `is_in_tdz_heritage_clause`** (Day 5-6, ~150 lines)

```rust
impl CheckerState {
    pub fn is_in_tdz_heritage_clause(&self, node_idx: NodeIndex) -> bool {
        // Check if class heritage clause extends/implements a class
        // that hasn't been declared yet.

        let class_decl = self.get_containing_class_declaration(node_idx)?;
        let heritage_clause = self.get_heritage_clause(class_decl)?;

        // For each class in heritage clause:
        for class_ref in self.get_class_references(heritage_clause) {
            let symbol = self.get_referenced_symbol(class_ref)?;

            // Check if symbol is declared AFTER the current class
            let class_position = self.get_position(class_decl);
            let symbol_decl_position = self.get_declaration_position(symbol)?;

            if symbol_decl_position > class_position {
                return true;
            }
        }

        false
    }
}
```

**Step 4: Add diagnostics** (Day 7, ~50 lines)

```rust
// src/checker/diagnostics.rs
impl CheckerState {
    fn check_tdz_violations(&mut self) {
        for node in self.get_all_identifier_references() {
            if self.is_in_tdz_static_block(node) {
                self.emit_error(Diagnostic {
                    range: self.get_range(node),
                    code: "TS2448",
                    message: "Cannot access class member before initialization".to_string(),
                });
            }

            if self.is_in_tdz_computed_property(node) {
                self.emit_error(Diagnostic {
                    range: self.get_range(node),
                    code: "TS2449",
                    message: "Cannot access variable before initialization in computed property".to_string(),
                });
            }

            if self.is_in_tdz_heritage_clause(node) {
                self.emit_error(Diagnostic {
                    range: self.get_range(node),
                    code: "TS2450",
                    message: "Cannot reference class before it is declared".to_string(),
                });
            }
        }
    }
}
```

**Step 5: Filter completions** (Day 8, ~50 lines)

```rust
// src/lsp/completions.rs
impl<'a> CompletionsProvider<'a> {
    fn filter_completions(&self, completions: Vec<CompletionItem>) -> Vec<CompletionItem> {
        completions
            .into_iter()
            .filter(|item| {
                let symbol = self.get_symbol_for_completion(item)?;

                // Filter out symbols in TDZ
                !self.checker.is_in_tdz_static_block(symbol)
                    && !self.checker.is_in_tdz_computed_property(symbol)
                    && !self.checker.is_in_tdz_heritage_clause(symbol)
            })
            .collect()
    }
}
```

**Step 6: Add tests** (Day 8-9, ~200 lines)

```rust
// src/checker/tests/tdz_tests.rs
#[test]
fn test_let_const_tdz() {
    let source = r#"
        console.log(x);  // ERROR: x in TDZ
        let x = 5;
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 1);
    assert_eq!(checker.errors[0].code, "TS2449");
}

#[test]
fn test_static_block_tdz() {
    let source = r#"
        class A {
            static x = B.y;  // ERROR: B.y in TDZ
        }
        class B {
            static y = 5;
        }
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    assert_eq!(checker.errors.len(), 1);
}
```

### Acceptance Criteria

- ✅ All three TDZ checks implemented correctly
- ✅ TDZ violations reported as errors
- ✅ Completions filtered to exclude TDZ symbols
- ✅ No false positives
- ✅ Performance: <100ms for 1000-line files
- ✅ Test coverage: 15+ test cases

### Impact

After this task:
- Temporal Dead Zone violations caught at compile time
- Completions don't suggest unavailable variables
- Improved code safety

---

## Task 4: Module Resolution

**Effort:** 4-6 days
**Impact:** ⭐⭐⭐
**Research:** [Team 5 Report](../research-findings/team-5-type-checker-gaps.md) - Section "Module Resolution"
**Files:** `src/checker/module_resolver.rs` (new), `src/binder/mod.rs`

### Background

**Problem:** External module resolution not implemented.

```typescript
// import { foo } from './utils';
//        ^^^ Cannot resolve module './utils'
```

### Implementation

**Step 1: Implement file system-based resolver** (Day 1-2, ~200 lines)

```rust
// src/checker/module_resolver.rs
pub struct ModuleResolver {
    /// Root directory for module resolution
    root: PathBuf,
    /// tsconfig.json settings
    config: TsConfig,
    /// Cache of resolved modules
    cache: FxHashMap<String, Option<String>>,
}

impl ModuleResolver {
    pub fn resolve_module(&mut self, from_file: &str, specifier: &str) -> Option<String> {
        // Check cache
        if let Some(cached) = self.cache.get(specifier) {
            return *cached;
        }

        let result = self.resolve_module_impl(from_file, specifier);
        self.cache.insert(specifier.to_string(), result);
        result
    }

    fn resolve_module_impl(&self, from_file: &str, specifier: &str) -> Option<String> {
        // 1. Check relative imports: './utils' -> './utils.ts'
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_relative_import(from_file, specifier);
        }

        // 2. Check node_modules: 'lodash' -> './node_modules/lodash/index.ts'
        if self.is_node_modules_import(specifier) {
            return self.resolve_node_modules_import(from_file, specifier);
        }

        // 3. Check tsconfig paths: '@utils/foo' -> './src/utils/foo'
        if let Some(mapped) = self.config.paths.get(specifier) {
            return self.resolve_path_alias(from_file, mapped);
        }

        None
    }

    fn resolve_relative_import(&self, from_file: &str, specifier: &str) -> Option<String> {
        let from_dir = Path::new(from_file).parent()?;
        let target_path = from_dir.join(specifier);

        // Try: utils.ts
        if target_path.with_extension("ts").exists() {
            return Some(target_path.with_extension("ts").to_str()?.to_string());
        }

        // Try: utils/index.ts
        if target_path.join("index.ts").exists() {
            return Some(target_path.join("index.ts").to_str()?.to_string());
        }

        // Try: utils.d.ts
        if target_path.with_extension("d.ts").exists() {
            return Some(target_path.with_extension("d.ts").to_str()?.to_string());
        }

        None
    }

    fn resolve_node_modules_import(&self, from_file: &str, specifier: &str) -> Option<String> {
        let mut current_dir = Path::new(from_file).parent()?;

        loop {
            let node_modules = current_dir.join("node_modules").join(specifier);

            // Try: node_modules/lodash/index.ts
            if node_modules.join("index.ts").exists() {
                return Some(node_modules.join("index.ts").to_str()?.to_string());
            }

            // Try: node_modules/lodash/package.json -> "types" field
            if let Ok(pkg) = std::fs::read_to_string(node_modules.join("package.json")) {
                if let Some(types) = self.parse_types_field(&pkg) {
                    return Some(node_modules.join(types).to_str()?.to_string());
                }
            }

            // Move up to parent directory
            current_dir = current_dir.parent()?;

            if current_dir == self.root {
                break;
            }
        }

        None
    }
}
```

**Step 2: Integrate with binder** (Day 3, ~50 lines)

```rust
// src/binder/mod.rs
impl BinderState {
    pub fn resolve_import(&mut self, import_specifier: &str) -> Option<String> {
        self.module_resolver.resolve_module(&self.file_name, import_specifier)
    }
}
```

**Step 3: Update LSP features** (Day 4-5, ~100 lines)

```rust
// src/lsp/completions.rs
impl<'a> CompletionsProvider<'a> {
    fn add_import_completions(&mut self) -> Vec<CompletionItem> {
        // Search project files for exported symbols
        let mut completions = Vec::new();

        for (file_name, file) in &self.project.files {
            if file_name == self.file_name {
                continue;
            }

            for export in file.get_exported_symbols() {
                let module_specifier = self.project.get_module_specifier(
                    self.file_name,
                    file_name,
                )?;

                completions.push(CompletionItem {
                    label: export.name.clone(),
                    kind: export.kind,
                    detail: Some(format!("Auto-import from '{}'", module_specifier)),
                    additional_text_edits: Some(vec![
                        TextEdit {
                            range: self.get_import_insert_position(),
                            new_text: format!("import {{ {} }} from '{}';\n", export.name, module_specifier),
                        }
                    ]),
                    ..Default::default()
                });
            }
        }

        completions
    }
}
```

**Step 4: Add tests** (Day 5-6, ~150 lines)

```rust
// src/checker/tests/module_resolution_tests.rs
#[test]
fn test_relative_import() {
    let source = r#"
        import { foo } from './utils';
        foo();
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    // Should resolve './utils' -> './utils.ts'
    assert!(checker.resolves_to("./utils.ts"));
}

#[test]
fn test_node_modules_import() {
    let source = r#"
        import { map } from 'lodash';
        map([1, 2, 3], x => x * 2);
    "#;

    let mut checker = CheckerState::new();
    checker.check(&parse(source));

    // Should resolve 'lodash' -> './node_modules/lodash/index.ts'
    assert!(checker.resolves_to("./node_modules/lodash/index.ts"));
}
```

### Acceptance Criteria

- ✅ Relative imports resolved correctly
- ✅ Node module imports resolved correctly
- ✅ Path aliases resolved correctly
- ✅ Auto-import suggestions work
- ✅ Go-to-definition works across files
- ✅ Performance: <50ms per import resolution
- ✅ Test coverage: 10+ test cases

### Impact

After this task:
- Cross-file completions work
- Auto-import suggestions available
- Go-to-definition navigates to external files
- Fourslash pass rate: +2-3%

---

## Summary

**Total Effort:** 27 days (5.5 weeks)
**Total Code Changes:** ~2,500 lines
**Type Checker Gaps Fixed:** 4

### Timeline

| Week | Tasks | Deliverables |
|------|-------|--------------|
| 1 | Control Flow Narrowing | Accurate hover/completions in narrowed contexts |
| 2 | Definite Assignment | TS2454 diagnostics, code actions |
| 3 | TDZ Checking (Part 1) | Static block TDZ detection |
| 4 | TDZ Checking (Part 2) | Computed property TDZ detection |
| 5 | Module Resolution | Cross-file completions, auto-import |
| 5.5 | Integration | All features working together |

### Success Metrics

- ✅ Hover shows narrowed types in all contexts
- ✅ Completions show contextually appropriate suggestions
- ✅ Runtime error diagnostics working
- ✅ Cross-file navigation working
- ✅ Fourslash pass rate: 15-20% (up from 6%)
- ✅ All tests passing
- ✅ No regressions

### Next Phase

After completing type system foundation, proceed to **Phase 3: Performance Optimization** to activate SymbolIndex for 100-1000x speedup.
