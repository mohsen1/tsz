# Parser Module Deep Dive

The parser transforms tokens into an Abstract Syntax Tree (AST) stored in a cache-optimized `NodeArena`. It implements a recursive descent parser with precedence climbing for expressions.

## File Structure

```
src/parser/
├── mod.rs                    Module definition and re-exports
├── base.rs                   Shared types (NodeIndex, NodeList, TextRange)
├── node.rs          (1,470+) Cache-optimized AST node representation
├── state.rs         (10,737) Main parser implementation
├── flags.rs                  Node and modifier flags
└── parse_rules/
    ├── mod.rs                Module organization
    └── utils.rs              Common parsing utilities
```

## Core Design: Thin Node Architecture

### 📍 KEY: 16-Byte Node Header (`node.rs`)

```rust
pub struct Node {
    pub kind: u16,        // SyntaxKind enum
    pub flags: u16,       // NodeFlags
    pub pos: u32,         // Start byte position
    pub end: u32,         // End byte position
    pub data_index: u32,  // Index into typed data pool (u32::MAX = NO_DATA)
}
```

**Cache optimization**:
- 16 bytes per node header
- 4 nodes fit per 64-byte cache line
- 13x better cache locality vs. 208-byte fat nodes

### NodeArena (`node.rs`)

```rust
pub struct NodeArena {
    pub nodes: Vec<Node>,           // All node headers

    // 40+ typed data pools by category:
    // Names
    pub identifiers: Vec<IdentifierData>,
    pub qualified_names: Vec<QualifiedNameData>,

    // Expressions
    pub binary_exprs: Vec<BinaryExpressionData>,
    pub call_exprs: Vec<CallExpressionData>,

    // Statements
    pub if_statements: Vec<IfStatementData>,
    pub blocks: Vec<BlockData>,

    // Types
    pub type_refs: Vec<TypeReferenceData>,
    pub union_types: Vec<CompositeTypeData>,

    // ... 40+ more pools
}
```

### Capacity Heuristics (`node.rs`)

Pre-allocated pool sizes based on typical code patterns:

```rust
// ~25% identifiers
identifiers: nodes_capacity / 4,
// ~12% each for common types
binary_exprs: nodes_capacity / 8,
call_exprs: nodes_capacity / 8,
blocks: nodes_capacity / 8,
// ~6% for less common types
functions: nodes_capacity / 16,
// 1 source file (usually)
source_files: 1,
```

## ParserState Structure (`state.rs`)

```rust
pub struct ParserState {
    pub scanner: ScannerState,
    pub arena: NodeArena,
    current_token: SyntaxKind,

    // Context flags (packed into u32)
    context_flags: u32,

    // Error tracking
    parse_diagnostics: Vec<Diagnostic>,
    last_error_pos: Option<usize>,

    // Performance
    emit_recursion_depth: u32,
}
```

### Context Flags (`state.rs`)

```rust
const CONTEXT_FLAG_ASYNC: u32 = 1;          // Inside async function
const CONTEXT_FLAG_GENERATOR: u32 = 2;       // Inside generator
const CONTEXT_FLAG_STATIC_BLOCK: u32 = 4;    // Inside class static block
const CONTEXT_FLAG_PARAMETER_DEFAULT: u32 = 8; // In parameter default
```

## Statement Parsing

### Entry Point (`state.rs`)

```rust
pub fn parse_source_file(&mut self) -> NodeIndex
```

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

### Declaration Parsing Locations

| Declaration | Lines | Key Details |
|-------------|-------|-------------|
| Functions | 1811-1992 | Async, generators, overloads |
| Parameters | 2036-2169 | Destructuring, defaults, rest |
| Classes | 2169-2595 | Members, heritage, constructors |
| Interfaces | 3597-4011 | Type members, signatures |
| Type aliases | 4084-4253 | Generic constraints |
| Enums | 4084-4253 | Const enums, string enums |
| Modules | 4253-4549 | Namespaces, ambient modules |
| Imports/Exports | 4577-5151 | All ES6 module syntax |

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

```rust
// Precedence levels (higher = tighter binding)
1:   Comma (,)
2:   Assignment (=, +=, -=, *=, /=, ...)
3:   Ternary (? :)
4:   Logical OR (||, ??)
5:   Logical AND (&&)
6:   Bitwise OR (|)
7:   Bitwise XOR (^)
8:   Bitwise AND (&)
9:   Equality (==, ===, !=, !==)
10:  Relational (<, >, <=, >=, instanceof, in, as, satisfies)
11:  Shift (<<, >>, >>>)
12:  Additive (+, -)
13:  Multiplicative (*, /, %)
14:  Exponentiation (**)
```

### Precedence Climbing Algorithm (`state.rs`)

```rust
fn parse_binary_expression(&mut self, min_precedence: u32) -> NodeIndex {
    let mut left = self.parse_unary_expression();

    loop {
        let precedence = get_binary_operator_precedence(self.current_token);
        if precedence < min_precedence {
            break;
        }

        let operator = self.current_token;
        self.next_token();

        // Right-associative for assignment and exponentiation
        let right_precedence = if is_right_associative(operator) {
            precedence
        } else {
            precedence + 1
        };

        let right = self.parse_binary_expression(right_precedence);
        left = self.create_binary_expression(left, operator, right);
    }

    left
}
```

### Primary Expressions (`state.rs:7009`)

```rust
fn parse_primary_expression(&mut self) -> NodeIndex {
    match self.current_token {
        // Identifiers and keywords
        Identifier => self.parse_identifier(),
        ThisKeyword => self.parse_this_expression(),
        SuperKeyword => self.parse_super_expression(),

        // Literals
        NumericLiteral | StringLiteral => self.parse_literal(),
        TrueKeyword | FalseKeyword => self.parse_boolean_literal(),
        NullKeyword => self.parse_null_literal(),

        // Compound expressions
        OpenBracket => self.parse_array_literal(),
        OpenBrace => self.parse_object_literal(),
        OpenParen => self.parse_parenthesized_expression(),

        // Functions and classes
        FunctionKeyword => self.parse_function_expression(),
        ClassKeyword => self.parse_class_expression(),
        AsyncKeyword => self.parse_async_expression(),

        // Templates
        TemplateHead | NoSubstitutionTemplate => self.parse_template(),

        // JSX
        LessThan => self.parse_jsx_element_or_type_assertion(),

        // ...
    }
}
```

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

```rust
last_error_pos: Option<usize>,  // Track last error position
```

**Rules**:
- Don't report missing `)` when already reported error at same position
- Suppress missing closing tokens if line break present (ASI)
- Force error for `(` followed by `{` pattern

### Resynchronization (`state.rs`)

```rust
fn resync_after_error(&mut self) {
    // Skip tokens while tracking brace/paren/bracket depth
    // Find next sync point (statement boundary)
    // Maximum 1000 iterations to prevent infinite loops
}
```

**Sync Points**:
- Statement start keywords (`var`, `if`, `function`, etc.)
- Control structure boundaries (`else`, `case`, `finally`, `catch`)
- Comma token (in declaration/object lists)

### Recursion Protection (`state.rs`)

```rust
const MAX_RECURSION_DEPTH: u32 = 1000;

fn enter_recursion(&mut self) -> bool {
    self.emit_recursion_depth += 1;
    if self.emit_recursion_depth > MAX_RECURSION_DEPTH {
        self.error(/* TS error 1005 */);
        return false;
    }
    true
}
```

## Speculative Parsing with Backtracking

### Scanner State Snapshot

```rust
let snapshot = self.scanner.save_state();
let current = self.current_token;

// ... speculative parsing ...

// On failure:
self.scanner.restore_state(snapshot);
self.current_token = current;
```

### Arena Truncation (`state.rs`)

```rust
let saved_arena_len = self.arena.nodes.len();
let saved_diagnostics_len = self.parse_diagnostics.len();

// ... speculative parsing ...

// On failure: discard created nodes
self.arena.nodes.truncate(saved_arena_len);
self.parse_diagnostics.truncate(saved_diagnostics_len);
```

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

### JSX Name Patterns (`state.rs:10416`)

```typescript
<div />              // Built-in (lowercase)
<Component />        // Component (PascalCase)
<ns:name />          // Namespaced
<Module.Component /> // Property access
```

## Known Gaps

### ⚠️ GAP: Expressions Module Disabled (`parse_rules/mod.rs`)

```rust
// expressions module has incompatible API - commented out until fixed
// mod expressions;
```

**Issue**: Full expressions module not integrated, all logic inline in `state.rs`
**Impact**: Code organization, harder to maintain

### ⚠️ GAP: Incremental Parsing (`state.rs`)

```rust
pub fn parse_source_file_statements_from_offset(...) -> ...
```

**Issue**: Method exists but appears to have limited testing
**Impact**: Incremental parsing infrastructure may not be fully utilized

### ⚠️ GAP: Dead JSX Code (`state.rs:10270`)

```rust
#[allow(dead_code)] // Infrastructure for JSX parsing
fn look_ahead_is_jsx_fragment(&mut self) -> bool
```

**Issue**: JSX fragment lookahead implemented but possibly unused

### ⚠️ GAP: Expression Statement Recovery

**Issue**: Minimal recovery for complex expression statements 
**Impact**: May not recover well from certain expression errors

## Performance Patterns

### 1. Pre-allocated Capacity

```rust
// Estimate: ~1 node per 20 source characters
let node_count_estimate = source_text.len() / 20;
```

### 2. Comment Caching (`state.rs`)

```rust
// Cache comment ranges once during parsing (O(N) scan)
// Avoids rescanning on every hover/documentation request
let comments = get_comment_ranges(self.scanner.source_text());
```

### 3. Loop Protection

| Location | Limit | Purpose |
|----------|-------|---------|
| `resync_after_error()` | 1000 iterations | Prevent infinite loops |
| `resync_to_next_expression_boundary()` | 100 iterations | Expression recovery |
| Arrow function lookahead | depth check | Nested parens |
| Generic arrow lookahead | depth check | Angle brackets |

---

**Previous**: [01-scanner.md](./01-scanner.md) - Scanner Module
**Next**: [03-binder.md](./03-binder.md) - Binder Module
