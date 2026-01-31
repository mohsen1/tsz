# Binder Module Deep Dive

The binder transforms the AST into a symbol table and control flow graph. It establishes the semantic foundation for type checking by resolving names to symbols and tracking variable scope.

## File Structure

```
src/binder/
├── mod.rs              Shared types and flags (~600 LOC)
├── state.rs            Main binder implementation (~3,100 LOC)
└── state_binding.rs    Declaration binding logic (~2,400 LOC)
```

## Core Data Structures

### Symbol (`binder/mod.rs`)

`Symbol` struct key fields:
- `flags: u32` - Kind and properties (see symbol_flags module)
- `escaped_name: String` - Symbol name
- `declarations: Vec<NodeIndex>` - All declarations
- `value_declaration: NodeIndex` - First value declaration
- `parent: SymbolId`, `id: SymbolId` - Symbol relationships
- `exports: Option<Box<SymbolTable>>` - For modules/namespaces
- `members: Option<Box<SymbolTable>>` - For classes/interfaces
- `is_exported`, `is_type_only` - Export/import modifiers
- `decl_file_idx`, `import_module`, `import_name` - Cross-file resolution

### SymbolId (`binder/mod.rs`)

`SymbolId(u32)` - Newtype wrapper with `NONE = u32::MAX` sentinel

### 📍 KEY: Symbol Flags (`binder.rs` - `symbol_flags` module)

**Declaration type flags:**
- `FUNCTION_SCOPED_VARIABLE` (1 << 0) - var, parameter
- `BLOCK_SCOPED_VARIABLE` (1 << 1) - let, const
- `PROPERTY`, `ENUM_MEMBER`, `FUNCTION`, `CLASS`, `INTERFACE`
- `CONST_ENUM`, `REGULAR_ENUM`, `VALUE_MODULE`, `NAMESPACE_MODULE`
- `TYPE_LITERAL`, `OBJECT_LITERAL`, `METHOD`, `CONSTRUCTOR`
- `GET_ACCESSOR`, `SET_ACCESSOR`, `SIGNATURE`, `TYPE_PARAMETER`
- `TYPE_ALIAS`, `EXPORT_VALUE`, `ALIAS`, `PROTOTYPE`, `EXPORT_STAR`
- `OPTIONAL`, `TRANSIENT`, `ASSIGNMENT`, `MODULE_EXPORTS`

**Modifier flags:** `PRIVATE`, `PROTECTED`, `ABSTRACT`, `STATIC` (bits 28-31)

**Composite flags:** `ENUM`, `VARIABLE`, `VALUE`, `TYPE`, `NAMESPACE`, `MODULE`, `ACCESSOR`

### SymbolArena (`binder/mod.rs`)

`SymbolArena` struct:
- `symbols: Vec<Symbol>` - Storage
- `base_offset: u32` - For checker-local symbols
- `CHECKER_SYMBOL_BASE = 0x10000000` - Offset for checker symbols

## Scope Management

### Scope Structure (`binder/mod.rs`)

`Scope` struct fields:
- `parent: ScopeId` - Parent scope
- `table: SymbolTable` - Local symbols
- `kind: ContainerKind` - Scope type
- `container_node: NodeIndex` - AST node

`ContainerKind` enum: `SourceFile`, `Function`, `Module`, `Class`, `Block`

### Scope Stack (`state.rs`)

`enter_scope(kind, node)`:
- Pushes to `scope_chain` (ScopeContext)
- Pushes to `scope_stack` (SymbolTable)
- Calls `enter_persistent_scope()` for checker access

`exit_scope(arena)`:
- Saves `exports` for modules, `members` for classes
- Pops from `scope_stack`
- Calls `exit_persistent_scope()`

## Binding Process

### Entry Point (`state.rs`)

`bind_source_file(arena, root)` - Main entry point

### Two-Pass Approach

**Pass 1: Collect hoisted declarations**
- `collect_hoisted_declarations(arena, root)` - Recursive scan
- `process_hoisted_functions()` - Declare function symbols
- `process_hoisted_vars()` - Declare var symbols

**Pass 2: Bind statements**
- Iterates through statements calling `bind_node(arena, statement)`

### Hoisting Mechanism (`state.rs`)

JavaScript's hoisting behavior for `var` and function declarations allows use before textual declaration.

**Key methods:**
- `collect_hoisted_declarations()` - Recursive scan collecting var (not let/const) and function declarations
- `process_hoisted_functions()` - Declares function symbols with `FUNCTION` flag
- `process_hoisted_vars()` - Declares var symbols with `FUNCTION_SCOPED_VARIABLE` flag

## Declaration Binding

### Variable Declarations (`state.rs`)

`bind_variable_declaration(arena, node)`:
1. Determines scope type: `BLOCK_SCOPED_VARIABLE` (let/const) or `FUNCTION_SCOPED_VARIABLE` (var)
2. Gets identifier(s), handling destructuring patterns
3. Calls `declare_symbol(name, flags, node, is_exported)`
4. Binds initializer via `bind_node()`
5. Creates flow assignment via `create_flow_assignment()`

### Function Declarations (`state.rs`)

`bind_function_declaration(arena, node)`:
1. Creates `FUNCTION` symbol via `declare_symbol()`
2. Enters function scope via `enter_scope(ContainerKind::Function, node)`
3. Declares `"arguments"` pseudo-variable
4. Binds parameters via `bind_parameter()`
5. Binds body via `bind_callable_body()`
6. Exits scope via `exit_scope()`

### Class Declarations (`state.rs`)

`bind_class_declaration(arena, node)`:
1. Creates `CLASS` symbol (with `ABSTRACT` flag if applicable)
2. Enters class scope via `enter_scope(ContainerKind::Class, node)`
3. Binds each member with appropriate flags (`METHOD`, `PROPERTY`, `GET_ACCESSOR`, `SET_ACCESSOR`, `CONSTRUCTOR`)
4. Exits scope (saves members to `symbol.members`)

### Module/Namespace Binding (`state.rs`)

`bind_module_declaration(arena, node)` handles special cases:
1. **Global augmentation** (`declare global`): Sets `in_global_augmentation` flag during binding
2. **Ambient module** (`declare module "pkg"`): Adds to `declared_modules`
3. **Shorthand ambient** (no body): Adds to `shorthand_ambient_modules`
4. **Regular module**: Creates symbol with `VALUE_MODULE | NAMESPACE_MODULE`, enters scope, binds statements, calls `populate_module_exports()`, exits scope

## Symbol Declaration (`state.rs`)

### 📍 KEY: declare_symbol

`declare_symbol(name, flags, declaration, is_exported) -> SymbolId`:
1. Checks if symbol already exists in `current_scope`
2. If exists, checks if merge is allowed via `can_merge_flags()`
   - If mergeable: combines flags, adds declaration
   - If not: emits TS2300 duplicate identifier error
3. Creates new symbol with given flags and declaration
4. Registers in `current_scope`, `node_symbols`

### Symbol Merge Rules (`state.rs`)

`can_merge_flags(existing, new) -> bool` allows merging:
- Interfaces with interfaces
- Classes with interfaces
- Functions with functions (overloads)
- Modules with functions/classes/enums/modules
- Static and instance members can coexist

## Control Flow Graph

### FlowNode (`binder/mod.rs`)

`FlowNode` struct fields:
- `flags: u32` - Flow type from `flow_flags` module
- `id: FlowNodeId` - Unique identifier
- `antecedent: Vec<FlowNodeId>` - Predecessors in CFG
- `node: NodeIndex` - Associated AST node

**Flow flags** (`flow_flags` module):
`UNREACHABLE`, `START`, `BRANCH_LABEL`, `LOOP_LABEL`, `ASSIGNMENT`, `TRUE_CONDITION`, `FALSE_CONDITION`, `SWITCH_CLAUSE`, `ARRAY_MUTATION`, `CALL`, `REDUCE_LABEL`, `REFERENCED`, `AWAIT_POINT`, `YIELD_POINT`

### Flow Node Creation (`state.rs`)

Key methods:
- `create_branch_label() -> FlowNodeId` - BRANCH_LABEL
- `create_loop_label() -> FlowNodeId` - LOOP_LABEL
- `create_flow_condition() -> FlowNodeId` - TRUE/FALSE_CONDITION
- `create_flow_assignment() -> FlowNodeId` - ASSIGNMENT
- `create_switch_clause_flow() -> FlowNodeId` - SWITCH_CLAUSE
- `create_flow_await_point() -> FlowNodeId` - AWAIT_POINT
- `create_flow_yield_point() -> FlowNodeId` - YIELD_POINT

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

`bind_import_declaration(arena, node)` handles all import forms:
- **Default import** (`import X from "mod"`): Creates `ALIAS` symbol with `import_name = "default"`
- **Named imports** (`import { foo, bar as baz } from "mod"`): Creates `ALIAS` symbols with `import_module` and `import_name`
- **Namespace import** (`import * as ns from "mod"`): Creates `ALIAS` symbol with `import_module`

### Export Declaration Binding (`state.rs`)

`bind_export_declaration(arena, node)` handles:
- **export default expr**: Binds expression, marks local symbol as exported
- **export { foo, bar }**: Creates `EXPORT_VALUE` symbols
- **Re-exports** (`export { foo } from "mod"`): Adds to `reexports` map
- **Wildcard** (`export * from "mod"`): Adds to `wildcard_reexports`

### Re-export Resolution (`state.rs`)

`resolve_import_with_reexports(module, name) -> Option<SymbolId>`:
1. Direct lookup in `module_exports`
2. Check named `reexports` (recursive resolution)
3. Check `wildcard_reexports` (recursive resolution)

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
