# Research Team 2: LSP Inlay Hints - Type Hints Implementation

**Date:** 2026-01-30
**Team:** Research Team 2
**Subject:** Adding Type Hints to Inlay Hints Implementation
**Status:** Research Complete

---

## Executive Summary

This report investigates the implementation of type hints for the LSP Inlay Hints feature in the TypeScript compiler (tsz). The current implementation supports parameter name hints but lacks type hints for implicit types (e.g., `let x = 1` → `let x: number = 1`).

**Key Findings:**
- Infrastructure for parameter hints is fully functional
- Type hint infrastructure is partially implemented (placeholder exists)
- Integration with `TypeInterner` and `CheckerState` is required
- No LSP server integration exists yet (needs wiring)
- Clear implementation path identified using existing patterns from `HoverProvider`

---

## 1. Current Inlay Hints Architecture

### 1.1 File Location
**Path:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

### 1.2 Current Implementation Structure

#### Data Structures

```rust
/// Kind of inlay hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlayHintKind {
    #[serde(rename = "parameter")]
    Parameter,     // Works ✅
    #[serde(rename = "type")]
    Type,          // Placeholder only ⚠️
    #[serde(rename = "generic")]
    Generic,       // Not implemented
}

/// An inlay hint - an inline annotation in the source code.
pub struct InlayHint {
    pub position: Position,      // Where to show the hint
    pub label: String,           // e.g., ": number"
    pub kind: InlayHintKind,
    pub tooltip: Option<String>,
}
```

#### InlayHintsProvider (Current State)

```rust
pub struct InlayHintsProvider<'a> {
    pub arena: &'a NodeArena,           // AST nodes ✅
    pub binder: &'a BinderState,        // Symbols ✅
    pub line_map: &'a LineMap,          // Position conversion ✅
    pub source: &'a str,                // Source text ✅
    // MISSING: TypeInterner ❌
    // MISSING: CheckerState ❌
}
```

### 1.3 Working Feature: Parameter Name Hints

**Status:** ✅ Fully Implemented

The parameter hints feature is complete and operational. Here's how it works:

**Flow:**
1. **AST Traversal:** `collect_hints` walks the AST tree
2. **Call Detection:** Identifies `CALL_EXPRESSION` nodes
3. **Symbol Resolution:**
   - Uses `self.binder.resolve_identifier()` to get the function's SymbolId
   - Retrieves the function's declaration node
4. **Parameter Extraction:**
   - `get_parameter_names()` extracts parameter names from function declaration
   - Handles `FunctionDeclaration`, `MethodDeclaration`, `FunctionExpression`
5. **Smart Filtering:**
   - Skips hints when argument name matches parameter name
   - Example: `foo(a)` where param is `a` → no hint shown
6. **Hint Generation:**
   - Creates `InlayHint` with label `: paramName`
   - Positions at the start of each argument

**Example:**
```typescript
function greet(name: string, age: number) {
  // ...
}

greet("Alice", 30);
//     ^^^^^^^ Parameter hint: "name"
//              ^^ Parameter hint: "age"
```

---

## 2. Type Hints Implementation Requirements

### 2.1 What's Needed for Type Hints

To implement type hints like `let x: number = 1`, we need:

1. **Type Information Access**
   - Need `TypeInterner` to store/retrieve type definitions
   - Need `CheckerState` to perform type inference

2. **Type Resolution**
   - Detect variable declarations without explicit type annotations
   - Infer type from initializer expression
   - Format type to human-readable string

3. **Hint Positioning**
   - Place hint after variable identifier
   - Format as `: type`

### 2.2 Missing Components

The current `InlayHintsProvider` lacks:

```rust
// Current (incomplete)
pub struct InlayHintsProvider<'a> {
    pub arena: &'a NodeArena,
    pub binder: &'a BinderState,
    pub line_map: &'a LineMap,
    pub source: &'a str,
    // ❌ Missing: pub interner: &'a TypeInterner
    // ❌ Missing: pub file_name: String
}
```

### 2.3 Placeholder Implementation

The current `collect_type_hints` method is a stub:

```rust
fn collect_type_hints(&self, _decl_idx: NodeIndex, _hints: &mut Vec<InlayHint>) {
    // Type hints require type inference which needs the TypeInterner.
    // For now, this is a placeholder. Full implementation would:
    // 1. Check if the variable declaration has no type annotation
    // 2. Get the inferred type from the checker
    // 3. Add a hint showing the inferred type
    //
    // This requires access to the TypeInterner and CheckerState,
    // which would need to be added to InlayHintsProvider.
}
```

---

## 3. TypeInterner & CheckerState Integration

### 3.1 What is TypeInterner?

**Location:** `/Users/mohsenazimi/code/tsz/src/solver/intern.rs`

The `TypeInterner` is a type storage and deduplication system:

**Key Features:**
- Stores all types in the system (primitives, objects, functions, etc.)
- Assigns unique `TypeId` to each distinct type
- Enables efficient type comparison (pointer equality)
- Provides type lookups: `lookup(type_id) -> Option<&TypeKey>`

**Type IDs:**
```rust
pub struct TypeId(pub u32);  // Unique identifier

// Intrinsic types (pre-defined):
TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN, etc.
```

**Type Keys (the actual type definitions):**
```rust
pub enum TypeKey {
    Intrinsic(IntrinsicKind),
    Literal(LiteralValue),
    Object(ObjectShapeId),
    Array(TypeId),
    Union(TypeListId),
    Function(FunctionShapeId),
    // ... and many more
}
```

### 3.2 What is CheckerState?

**Location:** `/Users/mohsenazimi/code/tsz/src/checker/state.rs`

The `CheckerState` orchestrates type checking:

**Key Capabilities:**
- **Type Inference:** `get_type_of_node(NodeIndex) -> TypeId`
- **Type Formatting:** `format_type(TypeId) -> String`
- **Symbol Resolution:** `get_type_of_symbol(SymbolId) -> TypeId`
- **Caching:** Maintains node type cache for performance

**Usage Pattern:**
```rust
let mut checker = CheckerState::new(
    arena,
    binder,
    interner,
    file_name,
    options,
);

// Get type of an AST node
let type_id = checker.get_type_of_node(node_idx);

// Format to string
let type_str = checker.format_type(type_id);
```

### 3.3 How format_type Works

**Location:** `/Users/mohsenazimi/code/tsz/src/checker/state_type_environment.rs:1396`

```rust
pub fn format_type(&self, type_id: TypeId) -> String {
    let mut formatter = TypeFormatter::with_symbols(
        self.ctx.types,
        &self.ctx.binder.symbols
    );
    formatter.format(type_id)
}
```

**TypeFormatter Capabilities:**
- Primitives: `number`, `string`, `boolean`, etc.
- Objects: `{ x: number; y: string }`
- Arrays: `number[]`, `string[][]`
- Tuples: `[number, string, boolean]`
- Unions: `string | number | null`
- Functions: `(x: number, y: string) => boolean`
- Generics: `Map<string, number>`
- Recursive types with depth limiting
- Union/intersection truncation for readability

---

## 4. Variable Declaration Structure

### 4.1 AST Node Structure

**Location:** `/Users/mohsenazimi/code/tsz/src/parser/node.rs:618`

```rust
pub struct VariableDeclarationData {
    pub name: NodeIndex,            // Identifier or BindingPattern
    pub exclamation_token: bool,    // Definite assignment assertion
    pub type_annotation: NodeIndex, // TypeNode (optional) ⚠️ KEY
    pub initializer: NodeIndex,     // Expression (optional)
}
```

### 4.2 Key Detection Logic

To determine if we need a type hint:

```rust
let Some(decl) = self.arena.get_variable_declaration(node) else {
    return;  // Not a variable declaration
};

// ⚠️ CRITICAL: Skip if already has type annotation
if !decl.type_annotation.is_none() {
    return;  // No hint needed, type is explicit
}

// ⚠️ CRITICAL: Need initializer to infer type
if decl.initializer.is_none() {
    return;  // Can't infer type without initializer
}

// Safe to add type hint
```

### 4.3 Type Inference Strategy

```rust
// Get the inferred type from the initializer
let type_id = checker.get_type_of_node(decl_idx);

// Format to string
let type_str = checker.format_type(type_id);

// Filter unhelpful types
if type_str == "any" || type_str == "unknown" {
    return;  // Don't show unhelpful hints
}
```

---

## 5. Implementation Plan

### 5.1 Step 1: Update InlayHintsProvider Structure

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

```rust
use crate::solver::TypeInterner;

pub struct InlayHintsProvider<'a> {
    pub arena: &'a NodeArena,
    pub binder: &'a BinderState,
    pub line_map: &'a LineMap,
    pub source: &'a str,
    // NEW FIELDS ✅
    pub interner: &'a TypeInterner,
    pub file_name: String,
}

impl<'a> InlayHintsProvider<'a> {
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source: &'a str,
        interner: &'a TypeInterner,  // Add parameter
        file_name: String,            // Add parameter
    ) -> Self {
        InlayHintsProvider {
            arena,
            binder,
            line_map,
            source,
            interner,  // Initialize
            file_name, // Initialize
        }
    }
}
```

### 5.2 Step 2: Implement collect_type_hints

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

```rust
fn collect_type_hints(
    &self,
    decl_idx: NodeIndex,
    hints: &mut Vec<InlayHint>,
    checker: &mut CheckerState,  // Requires checker parameter
) {
    let Some(node) = self.arena.get(decl_idx) else { return; };
    let Some(decl) = self.arena.get_variable_declaration(node) else { return; };

    // 1. Skip if it already has a type annotation
    if !decl.type_annotation.is_none() {
        return;
    }

    // 2. Skip if it doesn't have an initializer
    if decl.initializer.is_none() {
        return;
    }

    // 3. Get the inferred type of the declaration
    let type_id = checker.get_type_of_node(decl_idx);

    // 4. Format the type to string
    let type_text = checker.format_type(type_id);

    // 5. Filter out unhelpful types
    if type_text == "any" || type_text == "unknown" {
        return;
    }

    // 6. Determine position (after the identifier name)
    if let Some(name_node) = self.arena.get(decl.name) {
        let pos = self.line_map.offset_to_position(name_node.end, self.source);

        hints.push(InlayHint::new(
            pos,
            format!(": {}", type_text),
            InlayHintKind::Type,
        ));
    }
}
```

### 5.3 Step 3: Update Method Signatures

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

```rust
// Update provide_inlay_hints to create checker
pub fn provide_inlay_hints(&self, root: NodeIndex, range: Range) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // ... existing range calculation ...

    // Initialize CheckerState
    let options = CheckerOptions {
        strict: false,  // Or get from project config
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        self.arena,
        self.binder,
        self.interner,
        self.file_name.clone(),
        options,
    );

    // Pass checker to collect_hints
    self.collect_hints(root, range_start, range_end, &mut hints, &mut checker);

    hints
}

// Update collect_hints to accept checker
fn collect_hints(
    &self,
    node_idx: NodeIndex,
    range_start: u32,
    range_end: u32,
    hints: &mut Vec<InlayHint>,
    checker: &mut CheckerState,  // NEW PARAMETER
) {
    // ... existing logic ...

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        self.collect_type_hints(node_idx, hints, checker);  // Pass checker
    }

    // Recurse with checker
    for child_idx in self.arena.get_children(node_idx) {
        self.collect_hints(child_idx, range_start, range_end, hints, checker);
    }
}

// Update collect_parameter_hints (no checker needed, but signature must match)
fn collect_parameter_hints(
    &self,
    call_idx: NodeIndex,
    hints: &mut Vec<InlayHint>,
    _checker: &mut CheckerState,  // Unused but required for signature
) {
    // ... existing implementation unchanged ...
}
```

### 5.4 Step 4: Update Project Integration

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/project.rs`

Add a new method to the `Project` struct (currently missing):

```rust
impl Project {
    /// Get inlay hints for a file within a range.
    pub fn get_inlay_hints(&mut self, file_name: &str, range: Range) -> Option<Vec<InlayHint>> {
        let file = self.files.get(file_name)?;

        let provider = InlayHintsProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.source_text(),
            &file.type_interner,  // NEW: Pass interner
            file.file_name().to_string(),  // NEW: Pass file name
        );

        Some(provider.provide_inlay_hints(file.root(), range))
    }
}
```

**Note:** The `ProjectFile` struct already has `type_interner` field (line 75 in project.rs), so this is ready to use.

### 5.5 Step 5: LSP Server Integration

The LSP server integration is currently missing. The pattern should follow other LSP features:

**Pattern from HoverProvider:**
```rust
// In src/lsp/hover.rs
pub fn get_hover(
    &self,
    root: NodeIndex,
    position: Position,
    type_cache: &mut Option<TypeCache>,  // Persistent cache
) -> Option<HoverInfo>
```

**Recommended Inlay Hints Integration:**
```rust
// In src/lsp/project.rs
pub fn get_inlay_hints(
    &mut self,
    file_name: &str,
    range: Range,
    type_cache: &mut Option<TypeCache>,  // Reuse cache for performance
) -> Option<Vec<InlayHint>>
```

This allows the checker to reuse computed types across multiple inlay hint requests.

---

## 6. Testing Strategy

### 6.1 Unit Tests

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

Current tests (lines 283-304) only test the data structures:

```rust
#[test]
fn test_inlay_hint_parameter() {
    let position = Position::new(0, 10);
    let hint = InlayHint::parameter(position, "paramName".to_string());
    assert_eq!(hint.label, ": paramName");
}

#[test]
fn test_inlay_hint_type() {
    let position = Position::new(0, 10);
    let hint = InlayHint::type_hint(position, "number".to_string());
    assert_eq!(hint.label, ": number");
}
```

### 6.2 Integration Tests Needed

Create a new test file: `/Users/mohsenazimi/code/tsz/src/lsp/tests/inlay_hints_tests.rs`

```rust
#[test]
fn test_type_hints_for_variables() {
    let source = r#"
let x = 1;
let y = "hello";
const z = true;
"#;

    let project = create_test_project(source);
    let hints = project.get_inlay_hints("test.ts", full_range());

    assert_eq!(hints.len(), 3);
    assert_eq!(hints[0].label, ": number");
    assert_eq!(hints[1].label, ": string");
    assert_eq!(hints[2].label, ": boolean");
}

#[test]
fn test_no_hint_with_explicit_type() {
    let source = r#"
let x: number = 1;
"#;

    let project = create_test_project(source);
    let hints = project.get_inlay_hints("test.ts", full_range());

    assert_eq!(hints.len(), 0);  // No hint needed
}

#[test]
fn test_no_hint_without_initializer() {
    let source = r#"
let x;
"#;

    let project = create_test_project(source);
    let hints = project.get_inlay_hints("test.ts", full_range());

    assert_eq!(hints.len(), 0);  // Can't infer type
}

#[test]
fn test_complex_type_hints() {
    let source = r#"
let arr = [1, 2, 3];
let obj = { x: 1, y: "hello" };
let fn = (a: number) => a * 2;
"#;

    let project = create_test_project(source);
    let hints = project.get_inlay_hints("test.ts", full_range());

    assert_eq!(hints[0].label, ": number[]");
    assert_eq!(hints[1].label, ": { x: number; y: string }");
    assert_eq!(hints[2].label, ": (a: number) => number");
}

#[test]
fn test_parameter_and_type_hints_together() {
    let source = r#"
function greet(name: string, age: number) {
    let msg = "Hello";
    return msg;
}

greet("Alice", 30);
"#;

    let project = create_test_project(source);
    let hints = project.get_inlay_hints("test.ts", full_range());

    // Should have both type hints and parameter hints
    assert!(hints.iter().any(|h| h.label == ": string"));  // msg type
    assert!(hints.iter().any(|h| h.label == "name: "));   // param hint
    assert!(hints.iter().any(|h| h.label == "age: "));    // param hint
}
```

### 6.3 Fourslash Tests

Follow the pattern from `/Users/mohsenazimi/code/tsz/src/lsp/tests/fourslash_tests.rs`:

```typescript
// @tsz
// @filename: test.ts
let x = 1;/*here*/
let y: number = 2;/*no-hint*/

// @ expectations
// test.ts
//   hint at "/*here*/" - "x: number"
//   no hint at "/*no-hint*/"
```

---

## 7. Performance Considerations

### 7.1 Type Caching

**Critical for Performance:**

Creating a `CheckerState` is expensive. The LSP should maintain a persistent type cache:

```rust
pub fn get_inlay_hints(
    &mut self,
    file_name: &str,
    range: Range,
    type_cache: &mut Option<TypeCache>,  // REUSE THIS
) -> Option<Vec<InlayHint>> {
    let file = self.files.get(file_name)?;

    let mut checker = if let Some(cache) = type_cache.take() {
        CheckerState::with_cache(
            file.arena(),
            file.binder(),
            &file.type_interner,
            file.file_name().to_string(),
            cache,
        )
    } else {
        CheckerState::new(
            file.arena(),
            file.binder(),
            &file.type_interner,
            file.file_name().to_string(),
            CheckerOptions::default(),
        )
    };

    // ... compute hints ...

    // Save cache for next request
    *type_cache = Some(checker.extract_cache());
    Some(hints)
}
```

### 7.2 Incremental Computation

**Optimization Strategy:**

1. **Cache by Node:** The checker already has `node_types: FxHashMap<u32, TypeId>`
2. **Range Filtering:** Only traverse nodes within the requested range
3. **Lazy Evaluation:** Only compute types for nodes being hinted

### 7.3 Benchmark Targets

Based on similar features (hover, completions):

| Metric | Target |
|--------|--------|
| Cold start (no cache) | < 50ms |
| Warm start (with cache) | < 10ms |
| Memory overhead | < 5MB per file |

---

## 8. Comparison with Existing Features

### 8.1 HoverProvider Similarities

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/hover.rs`

The `HoverProvider` is the closest analog to what we need:

**Similarities:**
- Both need type information (`TypeInterner`, `CheckerState`)
- Both need to format types for display
- Both use `type_cache` for performance
- Both work with node positions and ranges

**Key Differences:**
- Hover: Shows type at cursor position (single node)
- Inlay Hints: Shows types for all variables in range (multiple nodes)

**Code Pattern to Follow:**
```rust
// From hover.rs:164-165
let type_id = checker.get_type_of_symbol(symbol_id);
let type_string = checker.format_type(type_id);
```

### 8.2 Completions Similarities

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/completions.rs`

Completions also use type checking:

```rust
// From completions.rs:478
let type_id = checker.get_type_of_node(expr_idx);

// From completions.rs:491
item = item.with_detail(checker.format_type(info.type_id));
```

---

## 9. Code Examples

### 9.1 Complete Type Hint Examples

**Example 1: Basic Types**
```typescript
let x = 1;           // Hint: let x: number = 1;
let y = "hello";     // Hint: let y: string = "hello";
let z = true;        // Hint: let z: boolean = true;
let a = null;        // Hint: let a: null = null;
let b = undefined;   // Hint: let b: undefined = undefined;
```

**Example 2: Complex Types**
```typescript
let arr = [1, 2, 3];              // Hint: let arr: number[] = [1, 2, 3];
let tuple = [1, "hello"];         // Hint: let tuple: [number, string] = [1, "hello"];
let obj = { x: 1, y: "hello" };   // Hint: let obj: { x: number; y: string } = { x: 1, y: "hello" };
let fn = (a) => a * 2;            // Hint: let fn: (a: number) => number = (a) => a * 2;
```

**Example 3: No Hint Cases**
```typescript
let x: number = 1;          // No hint (explicit type)
let y;                      // No hint (no initializer)
const z = something;        // Hint shown unless type is 'any'
```

**Example 4: Combined with Parameter Hints**
```typescript
function greet(name: string, age: number) {
    let message = "Hello";  // Hint: message: string
    return message;
}

greet("Alice", 30);
//  ^^^^^^^ Hint: name
//          ^^ Hint: age
```

### 9.2 Edge Cases

**Inferred Types from Context:**
```typescript
let x = Math.random() < 0.5 ? "yes" : "no";  // Hint: x: string
let y = [1, 2, 3].map(n => n * 2);          // Hint: y: number[]
```

**Generic Types:**
```typescript
let arr = [1, 2, 3];           // Hint: arr: number[]
let first = arr[0];            // Hint: first: number | undefined
let map = new Map();           // Hint: map: Map<unknown, unknown>
```

**Object Literals:**
```typescript
let obj = {
    x: 1,
    y: "hello",
    z: true
};  // Hint: obj: { x: number; y: string; z: boolean }
```

**Function Types:**
```typescript
let fn = (a: number, b: string) => a.toString();  // Hint: fn: (a: number, b: string) => string
let arrow = x => x * 2;                           // Hint: arrow: (x: number) => number
```

---

## 10. Potential Issues and Solutions

### 10.1 Performance: Checker Initialization

**Issue:** Creating a new `CheckerState` for every inlay hint request is expensive.

**Solution:** Reuse the type cache across requests:
```rust
// In Project struct
pub struct Project {
    // ...
    pub type_cache: FxHashMap<String, TypeCache>,
}

// Reuse cache
let cache = self.type_cache.entry(file_name.to_string()).or_insert_with(|| {
    // Initialize once
});
```

### 10.2 Cyclic Types

**Issue:** Some types are recursive and could cause infinite loops in formatting.

**Solution:** `TypeFormatter` already has depth limiting:
```rust
pub struct TypeFormatter<'a> {
    max_depth: u32,  // Default: 5
    current_depth: u32,
    // ...
}
```

### 10.3 Type Display Length

**Issue:** Complex types (e.g., large object types) could clutter the editor.

**Solution:** TypeFormatter has truncation:
```rust
pub struct TypeFormatter<'a> {
    max_union_members: usize,  // Default: 5
    // ...
}

// Example:
type Complex = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
// Displayed as: "1 | 2 | 3 | 4 | 5 | ..."
```

### 10.4 Destructuring Patterns

**Issue:** `let { x, y } = obj;` - what hint to show?

**Approach:** Show hints for each binding:
```typescript
let { x, y } = obj;
//   ^ Hint: x: number
//      ^ Hint: y: string
```

### 10.5 Type Parameters (Generics)

**Issue:** Should we show inferred type parameters?

**Example:**
```typescript
function identity<T>(x: T): T { return x; }
let result = identity(42);
```

**Decision:** For MVP, skip generic parameter hints (they're complex and rarely needed). Focus on variable type hints first.

---

## 11. Integration with WASM API

### 11.1 Current WASM Language Service

**File:** `/Users/mohsenazimi/code/tsz/src/wasm_api/language_service.rs`

The `TsLanguageService` already has:
- `TypeInterner` (line 27)
- `getCompletionsAtPosition` (line 67)
- `getQuickInfoAtPosition` (line 109)

### 11.2 Adding Inlay Hints to WASM API

```rust
#[wasm_bindgen(js_name = getInlayHints)]
pub fn get_inlay_hints(&self, start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> String {
    let range = Range {
        start: Position { line: start_line, character: start_char },
        end: Position { line: end_line, character: end_char },
    };

    let provider = InlayHintsProvider::new(
        &self.arena,
        &self.binder,
        &self.line_map,
        &self.source_text,
        &self.interner,
        self.file_name.clone(),
    );

    let hints = provider.provide_inlay_hints(self.root_idx, range);

    // Convert to JSON-serializable format
    let result: Vec<InlayHintJson> = hints
        .into_iter()
        .map(|h| InlayHintJson {
            position: PositionJson {
                line: h.position.line,
                character: h.position.character,
            },
            label: h.label,
            kind: match h.kind {
                InlayHintKind::Parameter => "parameter",
                InlayHintKind::Type => "type",
                InlayHintKind::Generic => "generic",
            }.to_string(),
        })
        .collect();

    serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string())
}
```

---

## 12. Dependencies and Imports

### 12.1 Required Imports

**File:** `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs`

Add these imports:
```rust
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::solver::TypeInterner;
```

### 12.2 Module Dependencies

The implementation depends on:
- `crate::binder::BinderState` ✅ (already imported)
- `crate::parser::{NodeIndex, NodeArena}` ✅ (already imported)
- `crate::solver::TypeInterner` ❌ (needs import)
- `crate::checker::state::CheckerState` ❌ (needs import)
- `crate::checker::context::CheckerOptions` ❌ (needs import)

---

## 13. Implementation Checklist

### Phase 1: Core Implementation ✅ (High Priority)
- [ ] Update `InlayHintsProvider` struct to include `TypeInterner` and `file_name`
- [ ] Implement `collect_type_hints` method
- [ ] Update `collect_hints` to pass `CheckerState` through recursion
- [ ] Update `provide_inlay_hints` to create and use `CheckerState`
- [ ] Add type filtering (skip `any`, `unknown`)

### Phase 2: Integration ✅ (High Priority)
- [ ] Add `get_inlay_hints` method to `Project` struct
- [ ] Wire up LSP server handler (in `bin/tsz_lsp.rs` or equivalent)
- [ ] Add WASM API method (`getInlayHints`)
- [ ] Update type cache reusability

### Phase 3: Testing ✅ (Medium Priority)
- [ ] Create unit tests for type hints
- [ ] Create integration tests with full type scenarios
- [ ] Create fourslash tests for visual validation
- [ ] Add performance benchmarks

### Phase 4: Edge Cases ✅ (Low Priority)
- [ ] Handle destructuring patterns
- [ ] Handle binding patterns (arrays, objects)
- [ ] Filter hints for extremely long types
- [ ] Add configuration options (enable/disable, type filtering)

---

## 14. Configuration Options

### 14.1 TypeScript Compiler Options

Consider respecting these tsconfig options:
```json
{
  "compilerOptions": {
    "noImplicitAny": true,        // Affects when to show hints
    "strictNullChecks": true,     // Affects union types
    "strict": true                // Master strict mode
  }
}
```

### 14.2 LSP-Specific Configuration

```typescript
// In tsconfig or LSP settings
{
  "typescript.inlayHints.enabled": true,
  "typescript.inlayHints.variableTypes.enabled": true,
  "typescript.inlayHints.parameterNames.enabled": true,
  "typescript.inlayHints.variableTypes.exclude": ["any", "unknown"]
}
```

---

## 15. Documentation Needs

### 15.1 Code Documentation

Add module-level documentation:
```rust
//! Inlay Hints for the LSP.
//!
//! Features:
//! - Parameter name hints for function calls
//! - Type hints for variables with implicit types
//!
//! ## Type Hints
//!
//! Type hints show the inferred type of variables without explicit type annotations.
//!
//! Example:
//! ```typescript
//! let x = 1;  // Shows: let x: number = 1
//! ```
//!
//! ## Implementation
//!
//! Type hints require:
//! - [`TypeInterner`] for type storage
//! - [`CheckerState`] for type inference
//! - Persistent type cache for performance
```

### 15.2 User Documentation

Document for end users:
- How to enable inlay hints
- What the different hint types mean
- Configuration options
- Keyboard shortcuts (if any)

---

## 16. References

### 16.1 Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `/Users/mohsenazimi/code/tsz/src/lsp/inlay_hints.rs` | Inlay hints implementation | 306 |
| `/Users/mohsenazimi/code/tsz/src/lsp/hover.rs` | Similar pattern to follow | ~300 |
| `/Users/mohsenazimi/code/tsz/src/checker/state.rs` | Type checker orchestration | ~13,000 |
| `/Users/mohsenazimi/code/tsz/src/solver/format.rs` | Type formatting | ~500 |
| `/Users/mohsenazimi/code/tsz/src/lsp/project.rs` | Project integration | ~1,500 |

### 16.2 Related Work

- **VS Code Inlay Hints:** https://code.visualstudio.com/api/language-extensions/programmatic-language-features#inlay-hints
- **LSP Specification:** https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_inlayHint
- **TypeScript's Implementation:** https://github.com/microsoft/TypeScript/blob/main/src/services/utilities.ts

### 16.3 Design Patterns Used

1. **Builder Pattern:** `InlayHint::new()`, `InlayHint::parameter()`, `InlayHint::type_hint()`
2. **Visitor Pattern:** AST traversal in `collect_hints`
3. **Strategy Pattern:** Different hint collection strategies (parameter vs type)
4. **Cache-Aside Pattern:** Type cache reusal for performance

---

## 17. Conclusion

### 17.1 Summary

The inlay hints type hints feature is **well-structured and ready for implementation**:

- ✅ Parameter hints are fully functional (proven concept)
- ✅ Infrastructure exists (AST, binder, position handling)
- ✅ Type system is mature (TypeInterner, CheckerState, formatting)
- ✅ Clear implementation path identified
- ⚠️ Missing LSP server integration (needs wiring)
- ⚠️ Placeholder type hint implementation needs filling

### 17.2 Estimated Effort

| Task | Estimate |
|------|----------|
| Core implementation (Phase 1) | 2-4 hours |
| Integration (Phase 2) | 2-3 hours |
| Testing (Phase 3) | 2-3 hours |
| **Total** | **6-10 hours** |

### 17.3 Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Performance degradation | Medium | Use type cache, limit range traversal |
| Complex type display | Low | TypeFormatter has built-in limits |
| Destructuring patterns | Low | Handle in Phase 4 |
| LSP integration unknowns | Medium | Follow existing patterns (hover, completions) |

### 17.4 Next Steps

1. **Immediate:** Implement Phase 1 (core implementation)
2. **Short-term:** Integrate with Project and LSP server (Phase 2)
3. **Medium-term:** Add comprehensive tests (Phase 3)
4. **Long-term:** Handle edge cases and add configuration (Phase 4)

---

## 18. Appendix

### 18.1 Complete Code Example

**Final Implementation of collect_type_hints:**

```rust
fn collect_type_hints(
    &self,
    decl_idx: NodeIndex,
    hints: &mut Vec<InlayHint>,
    checker: &mut CheckerState,
) {
    use crate::parser::syntax_kind_ext;

    let Some(node) = self.arena.get(decl_idx) else {
        return;
    };

    // Only process variable declarations
    if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
        return;
    }

    let Some(decl) = self.arena.get_variable_declaration(node) else {
        return;
    };

    // Skip if has explicit type annotation
    if !decl.type_annotation.is_none() {
        return;
    }

    // Skip if no initializer (can't infer type)
    if decl.initializer.is_none() {
        return;
    }

    // Get inferred type
    let type_id = checker.get_type_of_node(decl_idx);

    // Format type to string
    let type_text = checker.format_type(type_id);

    // Filter unhelpful types
    if type_text == "any" || type_text == "unknown" {
        return;
    }

    // Position hint after variable name
    let Some(name_node) = self.arena.get(decl.name) else {
        return;
    };

    let pos = self.line_map.offset_to_position(name_node.end, self.source);

    hints.push(InlayHint::new(
        pos,
        format!(": {}", type_text),
        InlayHintKind::Type,
    ));
}
```

### 18.2 Gemini Analysis Summary

The Gemini analysis provided excellent insights into:
1. The working parameter hint implementation
2. The exact structure needed for type hints
3. Integration points with TypeInterner and CheckerState
4. Code patterns to follow from HoverProvider

Key quote from Gemini:
> "The current implementation for parameter hints works by resolving the symbol of the function being called and looking up its declaration... To support type hints, the `Binder` is insufficient because it only tracks scope and symbol existence, not types. You need the **Checker** subsystem."

---

**End of Report**

**Prepared by:** Research Team 2
**Date:** 2026-01-30
**Status:** Ready for Implementation
