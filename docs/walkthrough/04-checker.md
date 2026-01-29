# Checker Module Deep Dive

The checker is a large module (~64,000 lines), responsible for type checking orchestration. It coordinates symbol resolution, type computation, control flow analysis, and error reporting.

## File Structure

```
src/checker/
├── mod.rs              Module organization
├── state.rs            Main orchestration and state machine (~9,000 LOC)
├── context.rs          Shared checking context (~1,100 LOC)
├── type_checking.rs    Type checking utilities (~7,900 LOC)
├── type_computation.rs Expression type computation (~3,200 LOC)
├── control_flow.rs     Control flow analysis (~3,900 LOC)
├── symbol_resolver.rs  Symbol resolution (~2,000 LOC)
├── flow_analysis.rs    Definite assignment analysis (~1,700 LOC)
├── error_reporter.rs   Error emission and formatting (~2,100 LOC)
├── class_type.rs       Class-specific type checking
├── function_type.rs    Function-specific type checking
├── array_type.rs       Array type handling
├── interface_type.rs   Interface type handling
├── union_type.rs       Union type operations
├── intersection_type.rs Intersection type operations
├── iterators.rs        Iterator protocol checking
├── generators.rs       Generator support
├── jsx.rs              JSX type checking
├── promise_checker.rs  Promise type checking
└── types/              Type-specific modules
```

## Core Architecture

### CheckerState (`state.rs`)

The main orchestration struct with `ctx: CheckerContext` containing all shared state.

### CheckerContext (`context.rs`)

Houses all shared state during type checking:

**Compiler options:** `options`, `strict_null_checks`, `no_implicit_any`, `strict_function_types`, `strict_property_initialization`

**Caching:** `symbol_types`, `node_types`, `type_parameter_names` (all `RefCell<FxHashMap<...>>`)

**Recursion guards:** `symbol_resolution_stack`, `node_resolution_set`, `class_instance_resolution_set`

**Type environment:** `type_env: RefCell<TypeEnvironment>`

**Diagnostics:** `diagnostics: RefCell<Vec<Diagnostic>>`

**Fuel counter:** `fuel: RefCell<u32>` - `MAX_TYPE_RESOLUTION_OPS` (100,000 native / 20,000 WASM)

## Type Resolution Entry Points

### 📍 KEY: get_type_of_node (`state.rs`)

Main type computation with caching and circular reference detection.

`get_type_of_node(idx) -> TypeId`:
1. Check cache (`ctx.node_types`)
2. Check for circular reference (`ctx.node_resolution_set`)
3. Mark as in-progress
4. Compute type via `compute_type_of_node()`
5. Remove from in-progress set
6. Cache and return result

### get_type_of_symbol (`state.rs:4020`)

Symbol type resolution with dependency tracking:

```rust
pub fn get_type_of_symbol(&mut self, symbol_id: SymbolId) -> TypeId {
    // 1. Check cache
    if let Some(cached) = self.ctx.symbol_types.borrow().get(&symbol_id) {
        return *cached;
    }

    // 2. Check for circular reference
    if self.ctx.symbol_resolution_stack.borrow().contains(&symbol_id) {
        return TypeId::ERROR;
    }

    // 3. Push to resolution stack
    self.ctx.symbol_resolution_stack.borrow_mut().push(symbol_id);

    // 4. Compute type
    let result = self.compute_type_of_symbol(symbol_id);

    // 5. Pop from stack
    self.ctx.symbol_resolution_stack.borrow_mut().pop();

    // 6. Cache result
    self.ctx.symbol_types.borrow_mut().insert(symbol_id, result);

    result
}
```

### compute_type_of_node (Dispatch)

Dispatches by AST node kind:

```rust
fn compute_type_of_node(&mut self, arena: &NodeArena, node: NodeIndex) -> TypeId {
    match arena.get(node).kind {
        // Literals
        NumericLiteral => self.get_type_of_numeric_literal(arena, node),
        StringLiteral => self.get_type_of_string_literal(arena, node),
        TrueKeyword | FalseKeyword => TypeId::BOOLEAN,
        NullKeyword => TypeId::NULL,

        // Identifiers
        Identifier => self.get_type_of_identifier(arena, node),
        ThisKeyword => self.get_type_of_this_keyword(arena, node),

        // Expressions
        BinaryExpression => self.get_type_of_binary_expression(arena, node),
        CallExpression => self.get_type_of_call_expression(arena, node),
        PropertyAccessExpression => self.get_type_of_property_access(arena, node),
        ElementAccessExpression => self.get_type_of_element_access(arena, node),

        // ... 100+ cases
    }
}
```

## Type Relations

### is_assignable_to (`state.rs:6028`)

Main assignability check via CompatChecker:

```rust
pub fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
    let checker = CompatChecker::new(
        &self.interner,
        self.ctx.strict_null_checks,
        self.ctx.strict_function_types,
        // ... other options
    );
    checker.is_assignable(source, target)
}
```

### is_subtype_of (`state.rs:6315`)

Stricter subtype checking:

```rust
pub fn is_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
    SubtypeChecker::new(&self.interner).check_subtype(source, target)
        == SubtypeResult::True
}
```

### are_types_identical

O(1) structural equality via type interning:

```rust
pub fn are_types_identical(&self, a: TypeId, b: TypeId) -> bool {
    a == b  // Same TypeId means structurally identical
}
```

## Type Checking Flow

### check_source_file (`state.rs:8359`)

Main entry point:

```rust
pub fn check_source_file(&mut self, arena: &NodeArena, root: NodeIndex) {
    // Reset fuel counter
    self.ctx.fuel.replace(MAX_TYPE_RESOLUTION_OPS);

    // Check each statement
    for statement in get_statements(arena, root) {
        self.check_statement(arena, statement);
    }
}
```

### check_statement (`state.rs:8492`)

Statement type checking dispatcher:

```rust
pub fn check_statement(&mut self, arena: &NodeArena, node: NodeIndex) {
    match arena.get(node).kind {
        VariableStatement => self.check_variable_statement(arena, node),
        FunctionDeclaration => self.check_function_declaration(arena, node),
        ClassDeclaration => self.check_class_declaration(arena, node),
        IfStatement => self.check_if_statement(arena, node),
        ReturnStatement => self.check_return_statement(arena, node),
        // ... more cases
    }
}
```

## Expression Type Computation (`type_computation.rs`)

### Binary Expressions (`type_computation.rs:735`)

```rust
pub fn get_type_of_binary_expression(&mut self, arena: &NodeArena, node: NodeIndex) -> TypeId {
    let data = arena.binary_exprs[node.data_index()];
    let left_type = self.get_type_of_node(arena, data.left);
    let right_type = self.get_type_of_node(arena, data.right);

    match data.operator {
        // Arithmetic
        Plus => {
            if is_string_like(left_type) || is_string_like(right_type) {
                TypeId::STRING
            } else {
                TypeId::NUMBER
            }
        }
        Minus | Asterisk | Slash | Percent => TypeId::NUMBER,

        // Comparison
        LessThan | GreaterThan | LessThanEquals | GreaterThanEquals => TypeId::BOOLEAN,
        EqualsEquals | ExclamationEquals | EqualsEqualsEquals | ExclamationEqualsEquals => TypeId::BOOLEAN,

        // Logical
        AmpersandAmpersand => {
            // Type narrowing: left && right
            // If left is falsy, result is left type
            // Otherwise, result is right type
            self.interner.intern_union(&[left_type, right_type])
        }

        // Assignment
        Equals => right_type,
        PlusEquals => /* ... */,

        // ... more operators
    }
}
```

### Call Expressions (`type_computation.rs:2405`)

Complex resolution with overloads and generics:

```rust
pub fn get_type_of_call_expression(&mut self, arena: &NodeArena, node: NodeIndex) -> TypeId {
    let data = arena.call_exprs[node.data_index()];

    // 1. Get callee type
    let callee_type = self.get_type_of_node(arena, data.expression);

    // 2. Check call depth (MAX_CALL_DEPTH = 20)
    if self.call_depth > MAX_CALL_DEPTH {
        return TypeId::ERROR;
    }
    self.call_depth += 1;

    // 3. Get call signatures from callee
    let signatures = self.get_call_signatures(callee_type);

    // 4. For each signature, check argument compatibility
    for signature in signatures {
        if self.check_arguments_match(arena, &data.arguments, &signature.parameters) {
            // 5. Instantiate generic type parameters
            let return_type = self.instantiate_signature(signature, type_arguments);
            self.call_depth -= 1;
            return return_type;
        }
    }

    // 6. No matching signature
    self.error(/* TS2554: Expected N arguments, got M */);
    self.call_depth -= 1;
    TypeId::ERROR
}
```

### Object Literal (`type_computation.rs:1316`)

```rust
pub fn get_type_of_object_literal(&mut self, arena: &NodeArena, node: NodeIndex) -> TypeId {
    let mut properties = Vec::new();

    for member in get_members(arena, node) {
        match member.kind {
            PropertyAssignment => {
                let name = get_property_name(arena, member);
                let value_type = self.get_type_of_node(arena, get_initializer(member));
                properties.push(PropertyInfo {
                    name,
                    ty: value_type,
                    optional: false,
                    readonly: false,
                });
            }
            ShorthandPropertyAssignment => {
                // { foo } === { foo: foo }
            }
            SpreadAssignment => {
                // { ...obj }
            }
            MethodDeclaration => {
                // { method() {} }
            }
        }
    }

    self.interner.intern_object(ObjectShape { properties, ... })
}
```

## Control Flow Analysis (`control_flow.rs`)

### FlowAnalyzer (`control_flow.rs:201`)

Iterative worklist algorithm (prevents stack overflow):

```rust
pub struct FlowAnalyzer<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    interner: &'a dyn TypeDatabase,
    node_types: &'a RefCell<FxHashMap<NodeIndex, TypeId>>,
    flow_graph: &'a FlowGraph,
}

impl FlowAnalyzer<'_> {
    pub fn get_flow_type(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        // Worklist algorithm
        let mut worklist = vec![flow_node];
        let mut results = FxHashMap::default();

        while let Some(current) = worklist.pop() {
            let node = self.flow_graph.get(current);

            match node.flags {
                ASSIGNMENT => {
                    // Check if this assignment affects our reference
                    if self.targets_reference(node.node, reference) {
                        let assigned_type = self.get_assigned_type(node.node);
                        results.insert(current, assigned_type);
                    } else {
                        // Flow through to antecedents
                        worklist.extend(&node.antecedent);
                    }
                }
                TRUE_CONDITION => {
                    // Apply type narrowing from condition
                    let narrowed = self.narrow_type_by_condition(
                        initial_type,
                        node.node,
                        true,  // assuming true
                    );
                    results.insert(current, narrowed);
                }
                FALSE_CONDITION => {
                    let narrowed = self.narrow_type_by_condition(
                        initial_type,
                        node.node,
                        false, // assuming false
                    );
                    results.insert(current, narrowed);
                }
                BRANCH_LABEL => {
                    // Union of all antecedent types
                    let union_types: Vec<_> = node.antecedent
                        .iter()
                        .filter_map(|a| results.get(a))
                        .copied()
                        .collect();
                    results.insert(current, self.interner.intern_union(&union_types));
                }
                // ... more cases
            }
        }

        results.get(&flow_node).copied().unwrap_or(initial_type)
    }
}
```

### Type Narrowing Conditions

```rust
fn narrow_type_by_condition(
    &self,
    type_: TypeId,
    condition: NodeIndex,
    assume_true: bool,
) -> TypeId {
    match self.arena.get(condition).kind {
        // typeof x === "string"
        BinaryExpression => {
            if is_typeof_comparison(condition) {
                let narrowed_type = get_type_from_typeof(condition);
                if assume_true {
                    self.narrow_to_type(type_, narrowed_type)
                } else {
                    self.subtract_type(type_, narrowed_type)
                }
            }
        }

        // x instanceof C
        BinaryExpression if operator == InstanceOfKeyword => {
            let class_type = self.get_type_of_node(right);
            if assume_true {
                self.narrow_to_type(type_, class_type)
            } else {
                type_  // Can't narrow on false instanceof
            }
        }

        // obj.kind === "A" (discriminant)
        BinaryExpression if is_discriminant_check(condition) => {
            // Narrow union by discriminant property
        }

        // x (truthy check)
        Identifier => {
            if assume_true {
                self.subtract_type(type_, self.interner.intern_union(&[
                    TypeId::NULL,
                    TypeId::UNDEFINED,
                ]))
            } else {
                type_
            }
        }
    }
}
```

## Symbol Resolution (`symbol_resolver.rs`)

### resolve_identifier_symbol (`symbol_resolver.rs`)

```rust
pub fn resolve_identifier_symbol(
    &self,
    arena: &NodeArena,
    node: NodeIndex,
) -> Option<SymbolId> {
    let name = get_identifier_text(arena, node);

    // 1. Check type parameter scope
    if let Some(type_param) = self.lookup_type_parameter(&name) {
        return Some(type_param);
    }

    // 2. Walk scope chain upward
    let scope_id = self.binder.find_enclosing_scope(arena, node)?;
    let mut current = Some(scope_id);

    while let Some(id) = current {
        let scope = &self.binder.scopes[id.0 as usize];
        if let Some(symbol) = scope.table.get(&name) {
            return Some(symbol);
        }
        current = if scope.parent == ScopeId::NONE { None } else { Some(scope.parent) };
    }

    // 3. Check file locals
    if let Some(symbol) = self.binder.file_locals.get(&name) {
        return Some(symbol);
    }

    // 4. Check lib binders
    for lib in &self.binder.lib_binders {
        if let Some(symbol) = lib.file_locals.get(&name) {
            return Some(symbol);
        }
    }

    None
}
```

### Global Intrinsic Detection (`symbol_resolver.rs`)

```rust
fn is_global_intrinsic(&self, name: &str) -> Option<TypeId> {
    match name {
        "undefined" => Some(TypeId::UNDEFINED),
        "NaN" | "Infinity" => Some(TypeId::NUMBER),
        "Math" => Some(self.get_math_type()),
        "JSON" => Some(self.get_json_type()),
        "Promise" => Some(self.get_promise_constructor_type()),
        // ... more intrinsics
        _ => None,
    }
}
```

## Error Reporting (`error_reporter.rs`)

### Layered Approach

**Level 1 - Core Emitters**:

```rust
pub fn error_at_node(&mut self, node: NodeIndex, code: u32, message: &str) {
    let span = self.get_node_span(node);
    self.ctx.diagnostics.borrow_mut().push(Diagnostic {
        code,
        message: message.to_string(),
        start: span.start,
        end: span.end,
        severity: DiagnosticSeverity::Error,
    });
}
```

**Level 2 - Type Errors**:

```rust
pub fn error_type_not_assignable_at(
    &mut self,
    node: NodeIndex,
    source: TypeId,
    target: TypeId,
) {
    self.diagnose_assignment_failure(node, source, target);
}

fn diagnose_assignment_failure(&mut self, node: NodeIndex, source: TypeId, target: TypeId) {
    // Get detailed failure reason from solver
    let reason = self.get_assignment_failure_reason(source, target);

    // Render failure reason to human-readable message
    let message = self.render_failure_reason(&reason);

    self.error_at_node(node, 2322, &message);
}
```

**Level 3 - Failure Reason Rendering**:

```rust
fn render_failure_reason(&self, reason: &SubtypeFailureReason) -> String {
    match reason {
        MissingProperty { name, target_type } => {
            format!(
                "Property '{}' is missing in type '{}' but required in type '{}'",
                name,
                self.format_type(source),
                self.format_type(target),
            )
        }
        PropertyTypeMismatch { name, source_type, target_type, nested_reason } => {
            let base = format!(
                "Types of property '{}' are incompatible",
                name,
            );
            if let Some(nested) = nested_reason {
                format!("{}\n  {}", base, self.render_failure_reason(nested))
            } else {
                base
            }
        }
        // ... more cases
    }
}
```

## Return Type Stack (`state.rs`)

For checking return statements against expected types:

```rust
pub fn push_return_type(&mut self, return_type: TypeId) {
    self.return_type_stack.push(return_type);
}

pub fn pop_return_type(&mut self) {
    self.return_type_stack.pop();
}

pub fn current_return_type(&self) -> Option<TypeId> {
    self.return_type_stack.last().copied()
}

// Usage:
fn check_function_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    let return_type = self.get_declared_return_type(arena, node);
    self.push_return_type(return_type);
    self.check_function_body(arena, body);
    self.pop_return_type();
}

fn check_return_statement(&mut self, arena: &NodeArena, node: NodeIndex) {
    let actual = self.get_type_of_node(arena, expression);
    if let Some(expected) = self.current_return_type() {
        if !self.is_assignable_to(actual, expected) {
            self.error(/* TS2322 */);
        }
    }
}
```

## Critical Limits

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_INSTANTIATION_DEPTH` | 50 | Generic instantiation recursion |
| `MAX_CALL_DEPTH` | 20 | Function call resolution nesting |
| `MAX_TREE_WALK_ITERATIONS` | 10,000 | Parent chain walking |
| `MAX_TYPE_RESOLUTION_OPS` | 500,000 | Fuel counter per file |
| `MAX_EXPR_CHECK_DEPTH` | 500 | Expression checker recursion |

## Known Gaps

### ⚠️ GAP: Definite Assignment Analysis (`flow_analysis.rs:1579`)

```rust
// TODO: Implement proper definite assignment checking
```

**Impact**: May not catch all uninitialized variable uses (TS2454)

### ⚠️ GAP: TDZ Checking (`flow_analysis.rs`)

```rust
// TODO: Implement TDZ checking for static blocks
// TODO: Implement TDZ checking for computed properties
// TODO: Implement TDZ checking for heritage clauses
```

**Impact**: Temporal Dead Zone violations may not be caught in all contexts

### ⚠️ GAP: Promise Detection (`state.rs 11913`)

```rust
// TODO: Investigate lib loading for Promise detection
```

**Impact**: Promise type checking may not work correctly if lib loading fails

### ⚠️ GAP: Reference Tracking (`type_checking.rs:5457`)

```rust
// TODO: Re-enable and fix reference tracking system properly
```

**Impact**: Some reference tracking disabled, may affect diagnostics

### ⚠️ GAP: Module Validation (`mod.rs:46`)

```rust
// mod module_validation;  // TODO: Fix API mismatches
```

**Impact**: Module/namespace validation disabled

### ⚠️ GAP: Generator Call Signatures (`iterable_checker.rs:120`)

```rust
// TODO: Check call signatures for generators when CallableShape is implemented
```

**Impact**: Generator return type checking may be incomplete

## Integration with Solver

The checker delegates type system operations to the solver:

```rust
// Type interning
let type_id = self.interner.intern(TypeKey::Union(types));

// Subtype checking
let is_subtype = SubtypeChecker::new(&self.interner)
    .check_subtype(source, target);

// Assignability with config
let compat = CompatChecker::new(&self.interner, config);
let is_assignable = compat.is_assignable(source, target);

// Type lowering (AST → TypeId)
let lowered = TypeLowering::new(&self.interner, resolver)
    .lower_type_node(arena, type_node);

// Type instantiation
let instantiated = TypeInstantiator::new(&self.interner)
    .substitute(type_id, substitution);
```

---

**Previous**: [03-binder.md](./03-binder.md) - Binder Module
**Next**: [05-solver.md](./05-solver.md) - Solver Module
