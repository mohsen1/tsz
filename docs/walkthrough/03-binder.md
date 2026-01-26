# Binder Module Deep Dive

The binder transforms the AST into a symbol table and control flow graph. It establishes the semantic foundation for type checking by resolving names to symbols and tracking variable scope.

## File Structure

```
src/
├── binder.rs              (~2.3 KB)  Shared types and flags
└── binder/
    └── state.rs           (~197 KB)  Main binder implementation
```

## Core Data Structures

### Symbol (`binder.rs`)

```rust
pub struct Symbol {
    pub flags: u32,                          // Kind and properties
    pub escaped_name: String,                // Symbol name
    pub declarations: Vec<NodeIndex>,        // All declarations
    pub value_declaration: NodeIndex,        // First value declaration
    pub parent: SymbolId,                    // Parent symbol
    pub exports: Option<Box<SymbolTable>>,   // For modules/namespaces
    pub members: Option<Box<SymbolTable>>,   // For classes/interfaces
    pub is_exported: bool,
    pub is_type_only: bool,                  // import type { X }
    pub decl_file_idx: u32,                  // Cross-file resolution
    pub import_module: Option<String>,       // Module specifier
    pub import_name: Option<String>,         // Original name if renamed
}
```

### SymbolId (`binder.rs:114`)

```rust
pub struct SymbolId(u32);

impl SymbolId {
    pub const NONE: SymbolId = SymbolId(u32::MAX);
}
```

### 📍 KEY: Symbol Flags (`binder.rs`)

```rust
// Declaration type flags
pub const FUNCTION_SCOPED_VARIABLE: u32 = 1 << 0;  // var, parameter
pub const BLOCK_SCOPED_VARIABLE: u32 = 1 << 1;     // let, const
pub const PROPERTY: u32 = 1 << 2;
pub const ENUM_MEMBER: u32 = 1 << 3;
pub const FUNCTION: u32 = 1 << 4;
pub const CLASS: u32 = 1 << 5;
pub const INTERFACE: u32 = 1 << 6;
pub const CONST_ENUM: u32 = 1 << 7;
pub const REGULAR_ENUM: u32 = 1 << 8;
pub const VALUE_MODULE: u32 = 1 << 9;              // Instantiated namespace
pub const NAMESPACE_MODULE: u32 = 1 << 10;         // Uninstantiated namespace
pub const METHOD: u32 = 1 << 13;
pub const CONSTRUCTOR: u32 = 1 << 14;
pub const GET_ACCESSOR: u32 = 1 << 15;
pub const SET_ACCESSOR: u32 = 1 << 16;
pub const TYPE_PARAMETER: u32 = 1 << 18;
pub const TYPE_ALIAS: u32 = 1 << 19;
pub const EXPORT_VALUE: u32 = 1 << 20;
pub const ALIAS: u32 = 1 << 21;                    // Imports

// Modifier flags
pub const PRIVATE: u32 = 1 << 28;
pub const PROTECTED: u32 = 1 << 29;
pub const ABSTRACT: u32 = 1 << 30;
pub const STATIC: u32 = 1 << 31;
```

### SymbolArena (`binder.rs`)

```rust
pub struct SymbolArena {
    symbols: Vec<Symbol>,
    base_offset: u32,  // For checker-local symbols
}

// Checker symbols start at this offset
pub const CHECKER_SYMBOL_BASE: u32 = 0x10000000;
```

## Scope Management

### Scope Structure (`binder.rs`)

```rust
pub struct Scope {
    pub parent: ScopeId,                 // Parent scope
    pub table: SymbolTable,              // Local symbols
    pub kind: ContainerKind,
    pub container_node: NodeIndex,       // AST node
}

pub enum ContainerKind {
    SourceFile,  // Global scope
    Function,    // Function scope (var hoisting)
    Module,      // Namespace/module scope
    Class,       // Class scope
    Block,       // Block scope
}
```

### Scope Stack (`state.rs`)

```rust
fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
    // Push to legacy scope_chain
    self.scope_chain.push(ScopeContext { ... });
    // Push to scope_stack (symbol table stack)
    self.scope_stack.push(SymbolTable::new());
    // Enter persistent scope for checker access
    self.enter_persistent_scope(kind, node);
}

fn exit_scope(&mut self, arena: &NodeArena) {
    // Save exports for modules
    if kind == ContainerKind::Module {
        // Copy current_scope to symbol.exports
    }
    // Save members for classes
    if kind == ContainerKind::Class {
        // Copy current_scope to symbol.members
    }
    // Pop scope
    self.scope_stack.pop();
    self.exit_persistent_scope();
}
```

## Binding Process

### Entry Point (`state.rs`)

```rust
pub fn bind_source_file(&mut self, arena: &NodeArena, root: NodeIndex)
```

### Two-Pass Approach

**Pass 1: Collect hoisted declarations** (`state.rs:764`)
```rust
self.collect_hoisted_declarations(arena, root);
```

**Process hoisting** (`state.rs`)
```rust
self.process_hoisted_functions();  // line 1106
self.process_hoisted_vars();       // line 1125
```

**Pass 2: Bind statements** (`state.rs`)
```rust
for statement in statements {
    self.bind_node(arena, statement);
}
```

### Hoisting Mechanism (`state.rs`)

JavaScript's hoisting behavior for `var` and function declarations:

```typescript
// This works due to hoisting:
console.log(x);  // undefined (not error)
var x = 5;

foo();  // works
function foo() { }
```

**Collection** (`state.rs`)
```rust
fn collect_hoisted_declarations(&mut self, arena: &NodeArena, node: NodeIndex) {
    // Recursive scan of statements
    // Collect var declarations (not let/const)
    // Collect function declarations
}
```

**Processing** (`state.rs`)
```rust
fn process_hoisted_functions(&mut self) {
    for (name, node) in &self.hoisted_functions {
        self.declare_symbol(name, FUNCTION, node, false);
    }
}

fn process_hoisted_vars(&mut self) {
    for (name, node) in &self.hoisted_vars {
        self.declare_symbol(name, FUNCTION_SCOPED_VARIABLE, node, false);
    }
}
```

## Declaration Binding

### Variable Declarations (`state.rs`)

```rust
fn bind_variable_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    // 1. Determine scope type
    let flags = if is_block_scoped {
        BLOCK_SCOPED_VARIABLE
    } else {
        FUNCTION_SCOPED_VARIABLE
    };

    // 2. Get identifier(s) from declaration
    // Handle destructuring patterns

    // 3. Declare symbol
    self.declare_symbol(&name, flags, node, is_exported);

    // 4. Bind initializer
    if let Some(init) = initializer {
        self.bind_node(arena, init);
    }

    // 5. Create flow assignment node
    self.create_flow_assignment(node);
}
```

### Function Declarations (`state.rs`)

```rust
fn bind_function_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    // 1. Create FUNCTION symbol
    self.declare_symbol(&name, FUNCTION, node, is_exported);

    // 2. Enter function scope
    self.enter_scope(ContainerKind::Function, node);

    // 3. Declare 'arguments' pseudo-variable
    self.declare_symbol("arguments", FUNCTION_SCOPED_VARIABLE, ...);

    // 4. Bind parameters
    for param in parameters {
        self.bind_parameter(arena, param);
    }

    // 5. Bind body with fresh control flow
    self.bind_callable_body(arena, body);

    // 6. Exit scope
    self.exit_scope(arena);
}
```

### Class Declarations (`state.rs`)

```rust
fn bind_class_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    // 1. Create CLASS symbol (+ ABSTRACT flag if needed)
    let flags = CLASS | if is_abstract { ABSTRACT } else { 0 };
    let symbol_id = self.declare_symbol(&name, flags, node, is_exported);

    // 2. Enter class scope
    self.enter_scope(ContainerKind::Class, node);

    // 3. Bind each member
    for member in members {
        match member.kind {
            MethodDeclaration => {
                let flags = METHOD | get_modifier_flags(member);
                self.declare_symbol(&name, flags, member, false);
            }
            PropertyDeclaration => {
                let flags = PROPERTY | get_modifier_flags(member);
                self.declare_symbol(&name, flags, member, false);
            }
            GetAccessor => {
                self.declare_symbol(&name, GET_ACCESSOR, member, false);
            }
            SetAccessor => {
                self.declare_symbol(&name, SET_ACCESSOR, member, false);
            }
            Constructor => {
                self.declare_symbol("constructor", CONSTRUCTOR, member, false);
            }
        }
    }

    // 4. Exit scope (saves members to symbol.members)
    self.exit_scope(arena);
}
```

### Module/Namespace Binding (`state.rs`)

```rust
fn bind_module_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    // Special cases:

    // 1. Global augmentation (declare global)
    if is_global_augmentation {
        self.in_global_augmentation = true;
        // ... bind declarations ...
        self.in_global_augmentation = false;
        return;
    }

    // 2. Ambient module declaration (declare module "pkg")
    if is_string_literal_name {
        self.declared_modules.insert(module_name);
    }

    // 3. Shorthand ambient module (declare module "foo" without body)
    if body.is_none() {
        self.shorthand_ambient_modules.insert(module_name);
        return;
    }

    // 4. Regular module
    let flags = VALUE_MODULE | NAMESPACE_MODULE;
    self.declare_symbol(&name, flags, node, is_exported);
    self.enter_scope(ContainerKind::Module, node);
    self.bind_statements(arena, body);
    self.populate_module_exports();  // line 3691
    self.exit_scope(arena);
}
```

## Symbol Declaration (`state.rs`)

### 📍 KEY: declare_symbol

```rust
fn declare_symbol(
    &mut self,
    name: &str,
    flags: u32,
    declaration: NodeIndex,
    is_exported: bool,
) -> SymbolId {
    // 1. Check if symbol already exists
    if let Some(existing) = self.current_scope.get(name) {
        // 2. Check if merge is allowed
        if self.can_merge_flags(existing.flags, flags) {
            // Merge: add declaration, combine flags
            existing.flags |= flags;
            existing.declarations.push(declaration);
            return existing.id;
        } else {
            // Error: duplicate identifier
            self.error(/* TS2300 */);
        }
    }

    // 3. Create new symbol
    let symbol = Symbol {
        flags,
        escaped_name: name.to_string(),
        declarations: vec![declaration],
        value_declaration: if is_value_flag(flags) { declaration } else { NONE },
        // ...
    };

    let id = self.symbols.alloc(symbol);
    self.current_scope.set(name, id);
    self.node_symbols.insert(declaration.index(), id);

    id
}
```

### Symbol Merge Rules (`state.rs`)

```rust
fn can_merge_flags(existing: u32, new: u32) -> bool {
    // Interfaces can merge with interfaces
    if both_are(INTERFACE) { return true; }

    // Classes can merge with interfaces
    if is(existing, CLASS) && is(new, INTERFACE) { return true; }

    // Functions can merge (overloads)
    if both_are(FUNCTION) { return true; }

    // Modules can merge with functions/classes/enums/modules
    if is(existing, VALUE_MODULE) && is(new, FUNCTION | CLASS | REGULAR_ENUM) {
        return true;
    }

    // Static and instance members can coexist
    // ...

    false
}
```

## Control Flow Graph

### FlowNode (`binder.rs`)

```rust
pub struct FlowNode {
    pub flags: u32,                  // Flow type
    pub id: FlowNodeId,
    pub antecedent: Vec<FlowNodeId>, // Predecessors
    pub node: NodeIndex,             // Associated AST node
}

// Flow flags
pub const UNREACHABLE: u32 = 1 << 0;
pub const START: u32 = 1 << 1;
pub const BRANCH_LABEL: u32 = 1 << 2;
pub const LOOP_LABEL: u32 = 1 << 3;
pub const ASSIGNMENT: u32 = 1 << 4;
pub const TRUE_CONDITION: u32 = 1 << 5;
pub const FALSE_CONDITION: u32 = 1 << 6;
pub const SWITCH_CLAUSE: u32 = 1 << 7;
pub const ARRAY_MUTATION: u32 = 1 << 8;
pub const CALL: u32 = 1 << 9;
```

### Flow Node Creation (`state.rs`)

```rust
fn create_branch_label(&mut self) -> FlowNodeId     // BRANCH_LABEL
fn create_loop_label(&mut self) -> FlowNodeId       // LOOP_LABEL
fn create_flow_condition(&mut self, ...) -> FlowNodeId  // TRUE/FALSE_CONDITION
fn create_flow_assignment(&mut self, ...) -> FlowNodeId // ASSIGNMENT
fn create_switch_clause_flow(&mut self, ...) -> FlowNodeId // SWITCH_CLAUSE
```

### If Statement Flow (`state.rs`)

```
                     [entry]
                        │
                        ▼
                   [condition]
                    /       \
            TRUE /           \ FALSE
                ▼             ▼
         [then_branch]  [else_branch]
                \           /
                 \         /
                  ▼       ▼
              [BRANCH_LABEL] (join)
```

### Loop Flow (`state.rs`)

```
                 [entry]
                    │
                    ▼
             ┌─[LOOP_LABEL]◄────┐
             │      │           │
             │      ▼           │
             │  [condition]     │
             │   /       \      │
             │ TRUE     FALSE   │
             │  │         │     │
             │  ▼         │     │
             │ [body]─────┘     │
             │  │               │
             │  └───────────────┘
             │
             ▼
         [exit_label]
```

## Import/Export Handling

### Import Declaration Binding (`state.rs`)

```rust
fn bind_import_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    let module_specifier = get_module_specifier(arena, node);

    // Default import: import X from "mod"
    if let Some(default_binding) = get_default_binding(arena, node) {
        let sym = self.declare_symbol(&name, ALIAS, default_binding, false);
        sym.import_module = Some(module_specifier.clone());
        sym.import_name = Some("default".to_string());
    }

    // Named imports: import { foo, bar as baz } from "mod"
    for specifier in named_bindings {
        let local_name = get_local_name(specifier);
        let imported_name = get_imported_name(specifier);

        let sym = self.declare_symbol(&local_name, ALIAS, specifier, false);
        sym.import_module = Some(module_specifier.clone());
        sym.import_name = Some(imported_name);
    }

    // Namespace import: import * as ns from "mod"
    if let Some(ns_binding) = get_namespace_binding(arena, node) {
        let sym = self.declare_symbol(&name, ALIAS, ns_binding, false);
        sym.import_module = Some(module_specifier.clone());
    }
}
```

### Export Declaration Binding (`state.rs`)

```rust
fn bind_export_declaration(&mut self, arena: &NodeArena, node: NodeIndex) {
    // export default expr
    if is_default_export {
        self.bind_node(arena, expression);
        // Mark local symbol as exported
    }

    // export { foo, bar }
    for specifier in export_specifiers {
        let sym = self.declare_symbol(&name, EXPORT_VALUE, specifier, true);
    }

    // export { foo } from "mod"  (re-export)
    if has_module_specifier {
        self.reexports.entry(file)
            .or_default()
            .insert(name, (module, original_name));
    }

    // export * from "mod"  (wildcard)
    if is_wildcard {
        self.wildcard_reexports.entry(file)
            .or_default()
            .push(module);
    }
}
```

### Re-export Resolution (`state.rs`)

```rust
fn resolve_import_with_reexports(&self, module: &str, name: &str) -> Option<SymbolId> {
    // 1. Direct lookup in module_exports
    if let Some(exports) = self.module_exports.get(module) {
        if let Some(sym) = exports.get(name) {
            return Some(sym);
        }
    }

    // 2. Check named re-exports
    if let Some(reexports) = self.reexports.get(module) {
        if let Some((target_module, original_name)) = reexports.get(name) {
            return self.resolve_import_with_reexports(target_module, original_name);
        }
    }

    // 3. Check wildcard re-exports
    if let Some(wildcards) = self.wildcard_reexports.get(module) {
        for target_module in wildcards {
            if let Some(sym) = self.resolve_import_with_reexports(target_module, name) {
                return Some(sym);
            }
        }
    }

    None
}
```

## Symbol Resolution

### resolve_identifier (`state.rs`)

```rust
pub fn resolve_identifier(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<SymbolId> {
    // 1. Find enclosing scope
    let scope_id = self.find_enclosing_scope(arena, node_idx)?;

    // 2. Walk up scope chain
    let mut current = Some(scope_id);
    while let Some(scope_id) = current {
        let scope = &self.scopes[scope_id.0 as usize];
        if let Some(sym) = scope.table.get(&name) {
            return Some(sym);
        }
        current = if scope.parent == ScopeId::NONE { None } else { Some(scope.parent) };
    }

    // 3. Fallback: check file_locals
    if let Some(sym) = self.file_locals.get(&name) {
        return Some(sym);
    }

    // 4. Fallback: check lib_binders
    for lib in &self.lib_binders {
        if let Some(sym) = lib.file_locals.get(&name) {
            return Some(sym);
        }
    }

    // 5. Resolve imports
    self.resolve_import_if_needed(symbol)
}
```

## Global Augmentations

### Tracking (`state.rs, 3581-3605`)

```rust
pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>
in_global_augmentation: bool
```

**Purpose**: Track `declare global` declarations that merge with lib.d.ts

```typescript
// In a module file:
declare global {
    interface Array<T> {
        customMethod(): void;  // Adds to built-in Array
    }
}
```

**Flow**:
1. Detect `declare global` or `global` namespace
2. Set `in_global_augmentation = true`
3. Interface/type declarations add to `global_augmentations` map
4. At check time, augmentations merge with lib types

## BinderState Structure (`state.rs`)

```rust
pub struct BinderState {
    // Symbol storage
    pub symbols: SymbolArena,
    pub current_scope: SymbolTable,
    scope_stack: Vec<SymbolTable>,
    pub file_locals: SymbolTable,
    pub declared_modules: FxHashSet<String>,

    // Control flow
    pub flow_nodes: FlowNodeArena,
    current_flow: FlowNodeId,

    // Scope chain
    scope_chain: Vec<ScopeContext>,
    pub scopes: Vec<Scope>,
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    pub node_symbols: FxHashMap<u32, SymbolId>,
    pub node_flow: FxHashMap<u32, FlowNodeId>,

    // Module resolution
    pub module_exports: FxHashMap<String, SymbolTable>,
    pub reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    pub shorthand_ambient_modules: FxHashSet<String>,

    // Global augmentations
    pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>,

    // Lib file chain
    lib_binders: Vec<Arc<BinderState>>,

    // Debugging
    pub debugger: ModuleResolutionDebugger,
}
```

## Resolved Design Decisions

### ✅ Default Export Handling (`state.rs`)

Default export handling is fully implemented. When `export default X` is encountered:

1. The exported expression/declaration is bound to visit inner references
2. A synthetic "default" export symbol is created with `ALIAS | EXPORT_VALUE` flags
3. The symbol is added to `file_locals` for cross-file import resolution
4. The underlying local symbol (if any) is also marked as exported

```rust
// Synthesize a "default" export symbol for cross-file import resolution.
// This enables `import X from './file'` to resolve the default export.
let default_sym_id = self.symbols.alloc(
    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
    "default".to_string(),
);
```

### ✅ Flow Analysis - Await/Yield Points (`state.rs`)

Await and yield expressions now properly generate flow nodes for control flow analysis:

```rust
// When binding await/yield expressions:
// 1. Create appropriate flow node (AWAIT_POINT or YIELD_POINT)
// 2. Link to current flow graph
// 3. Traverse into the inner expression

fn create_flow_await_point(&mut self, await_expr: NodeIndex) -> FlowNodeId {
    let id = self.flow_nodes.alloc(flow_flags::AWAIT_POINT);
    // ... link to antecedents ...
}

fn create_flow_yield_point(&mut self, yield_expr: NodeIndex) -> FlowNodeId {
    let id = self.flow_nodes.alloc(flow_flags::YIELD_POINT);
    // ... link to antecedents ...
}
```

This enables control flow analysis to account for async suspension points in generators and async functions.

### ✅ Local Symbol Shadowing (`state.rs`)

Local declarations now properly shadow lib symbols. When a local declaration has the same name as a lib symbol:

1. Check if existing symbol is from lib (using `CHECKER_SYMBOL_BASE` offset)
2. If so, create a new local symbol that shadows the lib symbol
3. Update scope to point to the local symbol

This allows code like `const Array = []` to work correctly without conflicting with the global `Array` type.

## Known Gaps

### ⚠️ GAP: Import Resolution Requires External Setup

**Location**: `state.rs`

```rust
// Method exists but depends on pre-populated module_exports
// Checker/runner must call populate_module_exports() for each file
```

**Impact**: Binder doesn't do file system resolution; requires external module resolver

### ⚠️ GAP: Array Mutation Flow (`state.rs`)

```rust
fn create_flow_array_mutation(&mut self, ...) -> FlowNodeId
```

**Issue**: Method exists but `bind_node()` doesn't call it for array mutations
**Impact**: `arr.push(x)` doesn't create flow node for narrowing

### ⚠️ GAP: Type-Only Import Validation (`state.rs`)

```rust
pub is_type_only: bool,  // Tracked on symbols
```

**Issue**: No validation that type-only imports aren't used as values
**Impact**: `import type { X } from 'mod'; new X()` may not error properly

### ⚠️ GAP: Shorthand Ambient Modules (`state.rs`)

```rust
pub shorthand_ambient_modules: FxHashSet<String>
```

**Issue**: `declare module "foo"` without body tracked, but symbol not created
**Impact**: Module resolves to `any` type but binding is incomplete

## Validation Methods

### validate_symbol_table (`state.rs`)

```rust
pub fn validate_symbol_table(&self) -> Vec<ValidationError> {
    // Check for broken symbol links
    // Check for orphaned symbols (no declarations)
    // Check for invalid value_declarations
}
```

### validate_global_symbols (`state.rs`)

```rust
// Expected global symbols
const EXPECTED_GLOBAL_SYMBOLS: &[&str] = &[
    "Object", "Function", "Array", "String", "Number", "Boolean",
    "Symbol", "BigInt", "Error", "Map", "Set", "WeakMap", "WeakSet",
    "Promise", "Reflect", "Proxy", "eval", "isNaN", "isFinite",
    "parseFloat", "parseInt", "Infinity", "NaN", "undefined", "console",
];

pub fn validate_global_symbols(&self) -> Vec<&'static str> {
    // Returns list of missing expected symbols
}
```

---

**Previous**: [02-parser.md](./02-parser.md) - Parser Module
**Next**: [04-checker.md](./04-checker.md) - Checker Module
