# Parser Module Deep Dive

The parser transforms tokens into an Abstract Syntax Tree (AST) stored in a cache-optimized `NodeArena`. It implements a recursive descent parser with precedence climbing for expressions.

## File Structure

```
src/parser/
├── mod.rs           Module definition and re-exports
├── base.rs          Shared types (NodeIndex, NodeList, TextRange)
├── node.rs          Cache-optimized AST node representation (~5,300 LOC)
├── state.rs         Main parser implementation (~10,800 LOC)
├── flags.rs         Node and modifier flags
└── parse_rules/
    ├── mod.rs       Module organization
    └── utils.rs     Common parsing utilities
```

## Core Design: Thin Node Architecture

### 📍 KEY: 16-Byte Node Header (`node.rs`)

`Node` struct fields (16 bytes total, `#[repr(C)]`):
- `kind: u16` - SyntaxKind enum
- `flags: u16` - NodeFlags
- `pos: u32` - Start byte position
- `end: u32` - End byte position
- `data_index: u32` - Index into typed data pool (`NO_DATA = u32::MAX`)

**Cache optimization**:
- 16 bytes per node header
- 4 nodes fit per 64-byte cache line
- 13x better cache locality vs. 208-byte fat nodes

### NodeArena (`node.rs`)

`NodeArena` struct contains:
- `nodes: Vec<Node>` - All node headers
- 40+ typed data pools by category:
  - Names: `identifiers`, `qualified_names`
  - Expressions: `binary_exprs`, `call_exprs`, `property_accesses`
  - Statements: `if_statements`, `blocks`, `for_statements`
  - Types: `type_refs`, `union_types`, `function_types`
  - And many more...

### Capacity Heuristics (`node.rs`)

Pre-allocated pool sizes based on typical code patterns:
- ~25% identifiers (`nodes_capacity / 4`)
- ~12% each for common types (`binary_exprs`, `call_exprs`, `blocks` at `/8`)
- ~6% for less common types (`functions` at `/16`)
- 1 source file (usually)

## ParserState Structure (`state.rs`)

`ParserState` key fields:
- `scanner: ScannerState` - Tokenization state
- `arena: NodeArena` - AST storage
- `current_token: SyntaxKind` - Current token being parsed
- `context_flags: u32` - Packed context flags
- `parse_diagnostics: Vec<Diagnostic>` - Error tracking
- `last_error_pos: Option<usize>` - Cascading error prevention
- `emit_recursion_depth: u32` - Recursion protection

### Context Flags (`state.rs`)

Context flags packed into `u32`:
- `CONTEXT_FLAG_ASYNC` - Inside async function
- `CONTEXT_FLAG_GENERATOR` - Inside generator
- `CONTEXT_FLAG_STATIC_BLOCK` - Inside class static block
- `CONTEXT_FLAG_PARAMETER_DEFAULT` - In parameter default value

## Statement Parsing

### Entry Point (`state.rs`)

`parse_source_file() -> NodeIndex` - Main entry point

### Statement Dispatch (`state.rs`)

30+ match arms for statement parsing:

| Token | Handler | Purpose |
|-------|---------|---------|
| `{` | `parse_block()` | Block statements |
| `var`/`let`/`const` | `parse_variable_statement()` | Variable declarations |
| `function` | `parse_function_declaration()` | Function declarations |
| `class` | `parse_class_declaration()` | Class declarations |
| `if` | `parse_if_statement()` | Conditional statements |
| `while`/`do`/`for` | `parse_*_statement()` | Loop statements |
| `switch` | `parse_switch_statement()` | Switch statements |
| `try` | `parse_try_statement()` | Exception handling |
| `return`/`break`/`continue` | `parse_*_statement()` | Control flow |
| `import` | `parse_import_declaration()` | ES6 imports |
| `export` | `parse_export_declaration()` | ES6 exports |
| `interface` | `parse_interface_declaration()` | TypeScript interfaces |
| `type` | `parse_type_alias_declaration()` | Type aliases |
| `enum` | `parse_enum_declaration()` | Enums |
| `namespace`/`module` | `parse_module_declaration()` | Namespaces |
| `@` | `parse_decorated_declaration()` | Decorators |
| Default | `parse_expression_statement()` | Expression statements |

### Declaration Parsing Methods

| Declaration | Method | Key Details |
|-------------|--------|-------------|
| Functions | `parse_function_declaration()` | Async, generators, overloads |
| Parameters | `parse_parameter()`, `parse_parameter_list()` | Destructuring, defaults, rest |
| Classes | `parse_class_declaration()`, `parse_class_expression()` | Members, heritage, constructors |
| Interfaces | `parse_interface_declaration()` | Type members, signatures |
| Type aliases | `parse_type_alias_declaration()` | Generic constraints |
| Enums | `parse_enum_declaration()` | Const enums, string enums |
| Modules | `parse_module_declaration()` | Namespaces, ambient modules |
| Imports/Exports | `parse_import_declaration()`, `parse_export_declaration()` | All ES6 module syntax |

## Expression Parsing: Precedence Climbing

### Expression Hierarchy (`state.rs`)

```
parse_expression()
  └─ parse_assignment_expression()     [precedence 2]
       ├─ check is_start_of_arrow_function()
       │   ├─ async arrow: parse_async_arrow_function_expression()
       │   └─ sync arrow: parse_arrow_function_expression_with_async()
       └─ parse_binary_expression(min_precedence=2)
           ├─ parse_unary_expression()
           └─ precedence climbing loop
```

### 📍 KEY: Operator Precedence Table (`state.rs`)

Precedence levels (higher = tighter binding) via `get_binary_operator_precedence()`:
1. Comma (`,`)
2. Assignment (`=`, `+=`, `-=`, `*=`, `/=`, ...)
3. Ternary (`? :`)
4. Logical OR (`||`, `??`)
5. Logical AND (`&&`)
6. Bitwise OR (`|`)
7. Bitwise XOR (`^`)
8. Bitwise AND (`&`)
9. Equality (`==`, `===`, `!=`, `!==`)
10. Relational (`<`, `>`, `<=`, `>=`, `instanceof`, `in`, `as`, `satisfies`)
11. Shift (`<<`, `>>`, `>>>`)
12. Additive (`+`, `-`)
13. Multiplicative (`*`, `/`, `%`)
14. Exponentiation (`**`)

### Precedence Climbing Algorithm (`state.rs`)

`parse_binary_expression(min_precedence) -> NodeIndex`:
- Parses left operand via `parse_unary_expression()`
- Loops while current operator precedence >= min_precedence
- Uses `is_right_associative()` for assignment and exponentiation
- Creates binary expression nodes via `create_binary_expression()`

### Primary Expressions (`state.rs`)

`parse_primary_expression() -> NodeIndex` dispatches by token:
- Identifiers: `parse_identifier()`, `parse_this_expression()`, `parse_super_expression()`
- Literals: `parse_literal()`, `parse_boolean_literal()`, `parse_null_literal()`
- Compound: `parse_array_literal()`, `parse_object_literal()`, `parse_parenthesized_expression()`
- Functions: `parse_function_expression()`, `parse_class_expression()`, `parse_async_expression()`
- Templates: `parse_template()`
- JSX: `parse_jsx_element_or_type_assertion()`

## Type Parsing (`state.rs`)

### Type Hierarchy

```
parse_type()
  └─ parse_conditional_type()          T extends U ? X : Y
       └─ parse_union_type()           T | U
            └─ parse_intersection_type() T & U
                 └─ parse_primary_type()
                      ├─ Keyword types (void, any, unknown, etc.)
                      ├─ Literal types (123, "string", true)
                      ├─ Type operators (keyof, typeof, readonly)
                      ├─ Infer types (infer T)
                      ├─ Tuple types ([T, U])
                      ├─ Object/Mapped types ({ [K in T]: V })
                      ├─ Function types ((x: T) => U)
                      └─ Type references (Module.Type<Args>)
```

### Key Type Parsing Methods

| Method | Purpose |
|--------|---------|
| `parse_type()` | Main entry point |
| `parse_return_type()` | With type predicates |
| `parse_conditional_type()` | `T extends U ? X : Y` |
| `parse_union_type()` | `T \| U` |
| `parse_intersection_type()` | `T & U` |
| `parse_tuple_type()` | `[T, U, ...V]` |
| `parse_mapped_type()` | `{ [K in T]: V }` |
| `parse_type_arguments()` | `<T, U>` |
| `parse_function_type()` | `(x: T) => U` |

## Ambiguous Syntax Resolution

### 1. Arrow Functions vs Comparisons (`state.rs`)

```typescript
(x) => y     // Arrow function
(x) > y      // Binary comparison
```

**Resolution**:
- `look_ahead_is_simple_arrow_function()` 
- Checks for `=>` after `)` without intervening line break (ASI)

### 2. Generic Arrows (`state.rs`)

```typescript
<T>(x: T) => T   // Generic arrow function
<T> = value      // Could be JSX in different context
```

**Resolution**:
- Skip nested `<>` pairs
- Check for `(` after closing `>`

### 3. JSX vs Type Assertions (`state.rs`)

```typescript
// In .tsx files:
<Component />   // JSX element

// In .ts files:
<Type>expr      // Type assertion
```

**Resolution**: File extension determines behavior (`.tsx`/`.jsx` → JSX)

### 4. Type Arguments vs Comparisons

```typescript
foo<T>(x)       // Type arguments + call
foo < T[x]      // Comparison: foo < (T[x])
```

**Resolution**: `try_parse_type_arguments_for_call()` with backtracking

### 5. Mapped Types vs Object Types (`state.rs`)

```typescript
{ a: Type }           // Object type
{ [K in Keys]: Type } // Mapped type
```

**Resolution**: Lookahead for `in` keyword after `[`

### 6. Index Signatures vs Computed Properties (`state.rs`)

```typescript
{ [key: string]: Type }  // Index signature
{ [expr]: value }        // Computed property
```

**Resolution**: Lookahead for `:` vs `=` after `[`

## Error Recovery

### Cascading Error Prevention (`state.rs`)

Uses `last_error_pos: Option<usize>` to track last error position.

**Rules**:
- Don't report missing `)` when already reported error at same position
- Suppress missing closing tokens if line break present (ASI)
- Force error for `(` followed by `{` pattern

### Resynchronization (`state.rs`)

`resync_after_error()`:
- Skips tokens while tracking brace/paren/bracket depth
- Finds next sync point (statement boundary)
- Maximum 1000 iterations to prevent infinite loops

**Sync Points**:
- Statement start keywords (`var`, `if`, `function`, etc.)
- Control structure boundaries (`else`, `case`, `finally`, `catch`)
- Comma token (in declaration/object lists)

### Recursion Protection (`state.rs`)

`MAX_RECURSION_DEPTH = 1000` (defined in `limits.rs` as `MAX_PARSER_RECURSION_DEPTH`)

`enter_recursion() -> bool`:
- Increments `emit_recursion_depth`
- Returns `false` and emits error if limit exceeded

## Speculative Parsing with Backtracking

### Scanner State Snapshot

- `scanner.save_state()` captures scanner position
- Save `current_token` locally
- On failure: `scanner.restore_state(snapshot)` and restore token

### Arena Truncation (`state.rs`)

- Save `arena.nodes.len()` and `parse_diagnostics.len()` before speculation
- On failure: truncate both to discard speculatively created nodes

## JSX Parsing (`state.rs`)

### JSX Element Structure

```typescript
<Component attr={expr} {...spread}>
  {children}
  <Nested />
  Text content
</Component>
```

### Key Methods

| Method | Purpose |
|--------|---------|
| `parse_jsx_element_or_self_closing_or_fragment()` | Main entry |
| `parse_jsx_element_name()` | Tag name parsing |
| `parse_jsx_attributes()` | Attribute list |
| `parse_jsx_children()` | Child content |

### JSX Name Patterns

- `<div />` - Built-in (lowercase)
- `<Component />` - Component (PascalCase)
- `<ns:name />` - Namespaced
- `<Module.Component />` - Property access

## Resolved Design Decisions

### ✅ Expression Parsing Architecture (`parse_rules/mod.rs`)

Expression parsing logic is implemented directly in `state.rs` using methods on `ParserState`
for optimal performance and simpler control flow. The precedence climbing algorithm for binary
expressions and all primary/unary expression parsing are integrated into the main parser state.

This design was intentional: a separate expressions module with a context-based API was evaluated
but rejected in favor of the direct method approach for better performance and maintainability.

### ✅ JSX Fragment Detection

JSX fragment detection (`<>`) is performed inline during `parse_jsx_opening_or_self_closing_or_fragment`
rather than via a separate lookahead function. This is more efficient since no backtracking is needed
when we can check for `>` immediately after consuming `<`.

### ✅ Incremental Parsing (`state.rs`)

`parse_source_file_statements_from_offset() -> IncrementalParseResult`

Incremental parsing is fully implemented and tested. The method parses statements from a given
byte offset, enabling partial re-parsing for IDE scenarios. Tests cover:
- Parsing from middle of file
- Parsing from start (offset 0)
- Handling offsets beyond EOF (clamped to source length)
- Tracking reparse_start position
- Recovery from syntax errors during incremental parse

### ✅ Expression Statement Recovery

Expression statement recovery has been enhanced with:
- `is_expression_boundary()` - Detects natural expression stopping points (`;`, `}`, `)`, `]`, `,`, `:`, etc.)
- `create_missing_expression()` - Creates placeholder nodes for missing expressions to maintain valid AST structure
- `try_recover_binary_rhs()` - Attempts to recover from missing right-hand operands in binary expressions
- Conditional expression recovery creates placeholders for missing true/false branches

These improvements ensure the parser produces a structurally valid AST even in the presence of errors,
enabling better IDE support and error reporting.

## Performance Patterns

### 1. Pre-allocated Capacity

Estimate: ~1 node per 20 source characters (`source_text.len() / 20`)

### 2. Comment Caching (`state.rs`)

`get_comment_ranges()` caches comment ranges once during parsing (O(N) scan), avoiding rescanning on every hover/documentation request.

### 3. Loop Protection

| Method | Limit | Purpose |
|--------|-------|---------|
| `resync_after_error()` | 1000 iterations | Prevent infinite loops |
| `resync_to_next_expression_boundary()` | 100 iterations | Expression recovery |
| Arrow function lookahead | depth check | Nested parens |
| Generic arrow lookahead | depth check | Angle brackets |

---

**Previous**: [01-scanner.md](./01-scanner.md) - Scanner Module
**Next**: [03-binder.md](./03-binder.md) - Binder Module
