# Research Report: LSP Signature Help for Incomplete Member Calls

**Research Team:** Team 1  
**Date:** 2025-01-30  
**Issue:** Signature help fails for incomplete member method calls like `obj.method(|`

---

## Executive Summary

The signature help implementation in `src/lsp/signature_help.rs` successfully handles complete calls and incomplete direct calls (e.g., `func(|`), but fails for incomplete member access calls (e.g., `obj.method(|`). The root cause is how the parser represents incomplete AST nodes and how the signature help provider resolves the callee expression.

---

## 1. Current Implementation Analysis

### 1.1 Signature Help Flow

The signature help implementation follows this workflow:

```rust
// src/lsp/signature_help.rs:157-174
fn get_signature_help_internal(...) -> Option<SignatureHelp> {
    // 1. Find node at cursor position
    let leaf_node = find_node_at_or_before_offset(self.arena, offset, self.source_text);
    
    // 2. Walk up to find CallExpression or NewExpression
    let (call_node_idx, call_expr, call_kind) = self.find_containing_call(leaf_node)?;
    
    // 3. Determine active parameter index
    let active_parameter = self.determine_active_parameter(call_node_idx, call_expr, offset);
    
    // 4. Resolve the symbol being called
    let mut walker = ScopeWalker::new(self.arena, self.binder);
    let symbol_id = walker.resolve_node(root, call_expr.expression);
    
    // 5. Get type of symbol/expression
    let callee_type = if let Some(symbol_id) = symbol_id {
        checker.get_type_of_symbol(symbol_id)
    } else {
        checker.get_type_of_node(call_expr.expression)
    };
    
    // 6. Extract signatures from type
    let signatures = self.get_signatures_from_type(callee_type, &checker, call_kind);
}
```

### 1.2 Call Expression Discovery

The `find_containing_call` function (lines 265-300) walks up the AST from the cursor position to find a `CallExpression` or `NewExpression`. This works correctly even for incomplete calls because:

1. The parser creates CallExpression nodes even when the argument list is empty or incomplete
2. The `node.end` position may extend to EOF for incomplete nodes
3. The function successfully finds the call node

### 1.3 Symbol Resolution

The critical issue is in symbol resolution (lines 179-184):

```rust
let mut walker = crate::lsp::resolver::ScopeWalker::new(self.arena, self.binder);
let symbol_id = if let Some(scope_cache) = scope_cache {
    walker.resolve_node_cached(root, call_expr.expression, scope_cache, scope_stats)
} else {
    walker.resolve_node(root, call_expr.expression)
};
```

**Key Limitation:** `ScopeWalker` is a *syntax-only* resolver. It only resolves `Identifier` nodes to symbols by walking scope chains. It **cannot** resolve `PropertyAccessExpression` nodes (e.g., `obj.method`) because:

1. Property access requires type information to look up the member on the object's type
2. The binder doesn't create symbols for property access members
3. `ScopeWalker::resolve_node` returns `None` for non-identifier expressions

---

## 2. Root Cause of Incomplete Member Call Failure

### 2.1 The Problem Chain

For incomplete code like:
```typescript
interface Obj { method(a: number, b: string): void; }
declare const obj: Obj;
obj.method(
```

The following occurs:

1. **Parser creates a CallExpression** with:
   - `expression`: PropertyAccessExpression (`obj.method`)
   - `arguments`: Empty NodeList (no arguments parsed yet)
   - `node.end`: At or near EOF (incomplete)

2. **Symbol resolution fails**:
   - `call_expr.expression` is a PropertyAccessExpression (not an Identifier)
   - `ScopeWalker::resolve_node` only handles identifiers
   - Returns `None`

3. **Type checking fallback fails**:
   ```rust
   let callee_type = if let Some(symbol_id) = symbol_id {
       checker.get_type_of_symbol(symbol_id)
   } else {
       checker.get_type_of_node(call_expr.expression)  // ← This line
   };
   ```

4. **Why type checking fails**:
   - `checker.get_type_of_node` attempts to type-check the expression
   - For `obj.method`, it needs to:
     a. Resolve type of `obj` → `Obj`
     b. Look up `method` property on `Obj`
     c. Get the type of `method` → `(a: number, b: string) => void`
   - **But**: In incomplete code, the parser may not have created complete nodes
   - The property access expression may be malformed or missing critical data
   - The checker cannot reliably determine the type

### 2.2 Why Complete Member Calls Work

For complete code like:
```typescript
obj.method(1, "x")
```

The flow succeeds because:
1. The parser creates complete, well-formed AST nodes
2. Even though `resolve_node` returns `None` for the property access
3. `checker.get_type_of_node` can successfully type-check the complete expression
4. The type checker finds `obj: Obj` → `method: (a, b) => void`

### 2.3 Why Incomplete Direct Calls Work

For incomplete code like:
```typescript
function add(a: number, b: number): number { return a + b; }
add(
```

The flow succeeds because:
1. `call_expr.expression` is an `Identifier` (`add`)
2. `ScopeWalker::resolve_node` successfully resolves `add` to its symbol
3. `checker.get_type_of_symbol(symbol_id)` returns the function type
4. Signature help works

---

## 3. Code Changes Needed

### 3.1 Enhanced Symbol Resolution for Property Access

**Location:** `src/lsp/signature_help.rs`, lines 218-233

**Current Code:**
```rust
let access_docs = if call_kind == CallKind::Call {
    self.signature_documentation_for_property_access(root, call_expr.expression)
} else {
    None
};

let (callee_type, docs) = if let Some(symbol_id) = symbol_id {
    (
        checker.get_type_of_symbol(symbol_id),
        access_docs.or_else(|| {
            self.signature_documentation_for_symbol(root, symbol_id, call_kind)
        }),
    )
} else {
    (checker.get_type_of_node(call_expr.expression), access_docs)
};
```

**Problem:** For incomplete property access, `checker.get_type_of_node` may fail.

**Solution:** Add a fallback mechanism that uses the existing JSDoc infrastructure:

1. The codebase already has `signature_documentation_for_property_access` (line 792-884)
2. This function:
   - Parses the property access to get the property name
   - Finds class declarations that have this method
   - Extracts JSDoc from matching method declarations
3. We can extend this to also extract type information

### 3.2 Implementation Approach

#### Option A: Enhance Type Checking for Incomplete Nodes

Make the type checker more resilient to incomplete AST nodes:

```rust
// In get_signature_help_internal, replace lines 224-233:
let (callee_type, docs) = if let Some(symbol_id) = symbol_id {
    (
        checker.get_type_of_symbol(symbol_id),
        access_docs.or_else(|| {
            self.signature_documentation_for_symbol(root, symbol_id, call_kind)
        }),
    )
} else {
    // For property access or other complex expressions,
    // try type-checking first, then fall back to JSDoc-based approach
    let type_from_checker = checker.get_type_of_node(call_expr.expression);
    
    if type_from_checker == TypeId::ERROR || type_from_checker == TypeId::UNKNOWN {
        // Type checking failed - try JSDoc extraction for property access
        if let Some(access_docs) = access_docs {
            // Extract type from JSDoc/declaration metadata
            if let Some(inferred_type) = self.infer_type_from_docs(call_expr.expression, &access_docs) {
                (inferred_type, Some(access_docs))
            } else {
                (type_from_checker, access_docs)
            }
        } else {
            (type_from_checker, None)
        }
    } else {
        (type_from_checker, access_docs)
    }
};
```

**Pros:**
- Leverages existing JSDoc infrastructure
- Works for incomplete code
- No changes to type checker needed

**Cons:**
- Only works for documented methods
- Requires methods to be in class declarations (doesn't work for interface types)
- May not infer correct overload

#### Option B: Parse Incomplete Calls More Robustly

Enhance the parser to create better AST nodes for incomplete calls:

**In parser:** Ensure that when parsing `obj.method(`, the parser creates:
1. A complete PropertyAccessExpression with proper `expression` and `name` nodes
2. A CallExpression with the PropertyAccessExpression as its callee
3. Properly initialized node positions even when incomplete

**In signature help:** Add special handling for property access:

```rust
fn get_signatures_from_expression(
    &self,
    expr: NodeIndex,
    checker: &CheckerState,
    call_kind: CallKind,
) -> Vec<SignatureCandidate> {
    // Check if this is a property access expression
    if let Some(expr_node) = self.arena.get(expr) {
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            // Try to resolve the property access symbol
            if let Some((object_type, property_name)) = 
                self.resolve_property_access(expr, checker) {
                // Look up the property on the object type
                return self.get_signatures_from_property(
                    object_type, 
                    &property_name, 
                    checker,
                    call_kind
                );
            }
        }
    }
    
    // Fall back to regular type checking
    let type_id = checker.get_type_of_node(expr);
    self.get_signatures_from_type(type_id, checker, call_kind)
}
```

**Pros:**
- More robust solution
- Works for any typed object, not just classes
- Can handle overloads correctly

**Cons:**
- Requires parser changes
- More complex implementation
- May require type checker enhancements for incomplete nodes

#### Option C: Use Binder Information for Property Members (Recommended)

The binder already tracks property declarations. We can leverage this:

```rust
fn resolve_property_access_symbol(
    &self,
    access_expr: NodeIndex,
) -> Option<SymbolId> {
    let Some(access_node) = self.arena.get(access_expr) else {
        return None;
    };
    let Some(access) = self.arena.get_access_expr(access_node) else {
        return None;
    };
    
    // Get property name
    let property_name = self.arena
        .get_identifier_text(access.name_or_argument)
        .or_else(|| self.arena.get_literal_text(access.name_or_argument))?;
    
    // Get object type
    let object_type = self.get_type_of_expression(access.expression)?;
    
    // Look up property symbol from binder's type information
    self.binder.find_property_member(object_type, &property_name)
}
```

**Pros:**
- Works with binder's existing symbol table
- No parser changes needed
- Leverages existing infrastructure

**Cons:**
- Requires type information which may be incomplete
- May need binder enhancements

---

## 4. Recommended Implementation Strategy

### Phase 1: Quick Fix (JSDoc-Based)

1. **Modify `get_signature_help_internal`** to handle property access specially
2. **When type checking fails** for property access, extract signatures from JSDoc
3. **Use existing `signature_documentation_for_property_access`** infrastructure
4. **Add fallback type inference** from declaration metadata

**Files to modify:**
- `src/lsp/signature_help.rs` (lines 218-233)
- Add new method: `infer_type_from_property_access`

**Estimated effort:** 2-3 hours

### Phase 2: Robust Solution (Binder-Based)

1. **Extend binder** to track property member symbols
2. **Add `resolve_property_access_symbol`** method to signature help provider
3. **Enhance type checker** to handle incomplete property access expressions
4. **Add comprehensive tests** for various incomplete scenarios

**Files to modify:**
- `src/lsp/signature_help.rs`
- `src/binder/mod.rs` (optional, for property tracking)
- `src/checker/state.rs` (for incomplete node handling)

**Estimated effort:** 1-2 days

### Phase 3: Comprehensive Testing

Add test cases for:
- `obj.method(|` - incomplete member call
- `obj.staticMethod(|` - incomplete static method call
- `this.method(|` - incomplete this member call
- `obj?.method(|` - incomplete optional chain call
- `obj.method<a>(|` - incomplete generic method call
- Nested incomplete calls: `obj.method1(obj.method2(|`

---

## 5. Testing Strategy

### 5.1 Unit Tests

Extend `src/lsp/tests/signature_help_tests.rs`:

```rust
#[test]
fn test_signature_help_incomplete_member_call() {
    // Currently ignored - make this pass
    let source = "interface Obj { method(a: number, b: string): void; }\ndeclare const obj: Obj;\nobj.method(";
    // ... test implementation
}

#[test]
fn test_signature_help_incomplete_this_member_call() {
    let source = "class Foo { method(x: number): void {} }\nconst foo = new Foo();\nfoo.method(";
    // ... test implementation
}

#[test]
fn test_signature_help_incomplete_static_call() {
    let source = "class Foo { static method(x: number): void {} }\nFoo.method(";
    // ... test implementation
}
```

### 5.2 Integration Tests

Create fourslash tests for real-world scenarios:
- Method calls with incomplete arguments
- Chained method calls
- Generic method calls
- Optional chaining

### 5.3 Regression Tests

Ensure existing tests continue to pass:
- Direct function calls (complete and incomplete)
- Member calls with complete arguments
- Overload selection
- JSDoc preservation

---

## 6. Code Examples

### Example 1: Enhanced Type Resolution

```rust
// In signature_help.rs
fn get_signatures_from_expression(
    &self,
    expr: NodeIndex,
    checker: &mut CheckerState,
    call_kind: CallKind,
) -> Vec<SignatureCandidate> {
    // Try type checking first
    let type_id = checker.get_type_of_node(expr);
    
    // If type checking succeeded, use it
    if !self.is_error_type(type_id) {
        return self.get_signatures_from_type(type_id, checker, call_kind);
    }
    
    // For property access, try JSDoc-based fallback
    if let Some(expr_node) = self.arena.get(expr) {
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(docs) = self.signature_documentation_for_property_access(expr, expr) {
                // Convert JSDoc to signature candidates
                return self.signatures_from_docs(&docs);
            }
        }
    }
    
    Vec::new()
}
```

### Example 2: Property Access Resolution

```rust
fn resolve_incomplete_property_access(
    &self,
    access_expr: NodeIndex,
    checker: &mut CheckerState,
) -> Option<TypeId> {
    let Some(access) = self.arena.get_access_expr(
        self.arena.get(access_expr)?
    )? else {
        return None;
    };
    
    // Get property name
    let property_name = self.arena
        .get_identifier_text(access.name_or_argument)?;
    
    // Get object type (may be incomplete)
    let object_type = checker.get_type_of_node(access.expression);
    
    // If we have a valid object type, look up the property
    if !self.is_error_type(object_type) {
        if let Some(members) = self.get_type_members(object_type) {
            if let Some(&member_type) = members.get(&property_name) {
                return Some(member_type);
            }
        }
    }
    
    None
}
```

---

## 7. Implementation Checklist

- [ ] Enable the ignored test `test_signature_help_incomplete_member_call`
- [ ] Add property access handling in `get_signature_help_internal`
- [ ] Implement JSDoc-based type inference fallback
- [ ] Add tests for incomplete this member calls
- [ ] Add tests for incomplete static method calls
- [ ] Add tests for incomplete optional chaining calls
- [ ] Verify existing tests still pass
- [ ] Document the implementation approach
- [ ] Add fourslash tests for real-world scenarios

---

## 8. Open Questions

1. **Type checker robustness:** Should we enhance the type checker to handle incomplete nodes, or use a JSDoc-based fallback?

2. **Interface types:** The JSDoc approach only finds methods in class declarations. How should we handle interface-based type annotations?

3. **Generic methods:** How should we handle incomplete generic method calls like `obj.method<number>(|`?

4. **Performance:** Will the additional property access resolution impact signature help performance?

---

## 9. Related Code References

- **Test file:** `/Users/mohsenazimi/code/tsz/src/lsp/tests/signature_help_tests.rs:153`
- **Implementation:** `/Users/mohsenazimi/code/tsz/src/lsp/signature_help.rs`
- **Node utilities:** `/Users/mohsenazimi/code/tsz/src/lsp/utils.rs`
- **Symbol resolution:** `/Users/mohsenazimi/code/tsz/src/lsp/resolver.rs`
- **JSDoc handling:** `/Users/mohsenazimi/code/tsz/src/lsp/jsdoc.rs`

---

## 10. Conclusion

The signature help implementation is well-structured and handles most cases correctly. The failure for incomplete member calls is a specific edge case where:

1. The parser creates an incomplete CallExpression
2. Symbol resolution fails for PropertyAccessExpression
3. Type checking may not succeed for incomplete nodes

The recommended approach is to add a JSDoc-based fallback mechanism (Phase 1) for a quick fix, followed by a more robust binder-based solution (Phase 2) for comprehensive coverage.

The existing infrastructure for handling property access JSDoc (lines 792-884) provides a solid foundation for the fallback mechanism, requiring minimal changes to implement the initial fix.
