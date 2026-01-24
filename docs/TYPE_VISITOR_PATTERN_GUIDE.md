# Type Visitor Pattern - Usage Guide

This guide demonstrates how to use the Type Visitor Pattern to replace repetitive TypeKey match statements throughout the codebase.

## Overview

The Type Visitor Pattern (implemented in `src/solver/visitor.rs`) provides:
- **TypeVisitor trait**: Generic visitor interface with methods for all 24 TypeKey variants
- **Built-in visitors**: TypeKindVisitor, TypeCollectorVisitor, TypePredicateVisitor
- **Convenience functions**: `is_type_kind()`, `collect_referenced_types()`, `test_type()`

## Benefits

- **Centralized logic**: All type handling in one place
- **Type-safe**: Compiler ensures all variants are handled
- **Easier to extend**: Add new visitors without modifying existing code
- **Composable**: Visitors can be combined and chained

---

## Example 1: Type Classification

### BEFORE (Repetitive match statements)

```rust
// In array_type.rs:31
pub fn is_mutable_array_type(&self, type_id: TypeId) -> bool {
    matches!(self.ctx.types.lookup(type_id), Some(TypeKey::Array(_)))
}

// In callable_type.rs:32
pub fn is_callable_type(&self, type_id: TypeId) -> bool {
    matches!(self.ctx.types.lookup(type_id), Some(TypeKey::Callable(_)))
}

// In union_type.rs:28
pub fn is_union_type(&self, type_id: TypeId) -> bool {
    matches!(self.ctx.types.lookup(type_id), Some(TypeKey::Union(_)))
}

// Repeated for every type kind...
```

### AFTER (Using TypeKindVisitor)

```rust
use crate::solver::visitor::{is_type_kind, TypeKind};

// All type checks use the same pattern
pub fn is_mutable_array_type(&self, type_id: TypeId) -> bool {
    is_type_kind(&self.ctx.types, type_id, TypeKind::Array)
}

pub fn is_callable_type(&self, type_id: TypeId) -> bool {
    is_type_kind(&self.ctx.types, type_id, TypeKind::Function)
}

pub fn is_union_type(&self, type_id: TypeId) -> bool {
    is_type_kind(&self.ctx.types, type_id, TypeKind::Union)
}
```

**Benefits**:
- Consistent API across all type checks
- No repetitive match statements
- Compiler enforces completeness

---

## Example 2: Custom Type Predicates

### BEFORE (Inline match statements)

```rust
// Checking for string literal types
pub fn is_string_literal(&self, type_id: TypeId) -> bool {
    match self.ctx.types.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::String(_))) => true,
        _ => false,
    }
}

// Checking for boolean literal types
pub fn is_boolean_literal(&self, type_id: TypeId) -> bool {
    match self.ctx.types.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::Boolean(_))) => true,
        _ => false,
    }
}
```

### AFTER (Using TypePredicateVisitor)

```rust
use crate::solver::visitor::test_type;
use crate::solver::{TypeKey, LiteralValue};

pub fn is_string_literal(&self, type_id: TypeId) -> bool {
    test_type(&self.ctx.types, type_id, |key| {
        matches!(key, TypeKey::Literal(LiteralValue::String(_)))
    })
}

pub fn is_boolean_literal(&self, type_id: TypeId) -> bool {
    test_type(&self.ctx.types, type_id, |key| {
        matches!(key, TypeKey::Literal(LiteralValue::Boolean(_)))
    })
}
```

**Benefits**:
- More concise
- Predicate function is clear and focused
- Can be inlined for one-off checks

---

## Example 3: Collecting Type Dependencies

### BEFORE (Manual collection with match)

```rust
pub fn get_array_referenced_types(&self, type_id: TypeId) -> Vec<TypeId> {
    let mut refs = Vec::new();

    if let Some(TypeKey::Array(elem_type)) = self.ctx.types.lookup(type_id) {
        refs.push(elem_type);
        // Recursively collect from element type?
        // Need more match statements...
    }

    refs
}
```

### AFTER (Using TypeCollectorVisitor)

```rust
use crate::solver::visitor::collect_referenced_types;

pub fn get_array_referenced_types(&self, type_id: TypeId) -> Vec<TypeId> {
    let refs = collect_referenced_types(&self.ctx.types, type_id);
    refs.into_iter().collect()
}
```

**Benefits**:
- Handles all type complexities automatically
- Includes indirect dependencies
- No need to manually handle each TypeKey variant

---

## Example 4: Complex Type Analysis

### BEFORE (Nested match statements)

```rust
pub fn analyze_generic_function(&self, type_id: TypeId) -> TypeAnalysis {
    match self.ctx.types.lookup(type_id) {
        Some(TypeKey::Callable(shape_id)) => {
            let shape = self.ctx.types.callable_shape(shape_id);
            let has_type_params = shape.call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());

            let returns_union = shape.call_signatures
                .iter()
                .any(|sig| {
                    match self.ctx.types.lookup(sig.return_type) {
                        Some(TypeKey::Union(_)) => true,
                        _ => false,
                    }
                });

            TypeAnalysis {
                is_callable: true,
                has_type_params,
                returns_union,
            }
        }
        Some(TypeKey::Function(shape_id)) => {
            // Similar logic duplicated...
        }
        _ => TypeAnalysis::default(),
    }
}
```

### AFTER (Using custom visitor)

```rust
use crate::solver::visitor::TypeVisitor;

pub fn analyze_generic_function(&self, type_id: TypeId) -> TypeAnalysis {
    struct TypeAnalyzer {
        analysis: TypeAnalysis,
    }

    impl TypeVisitor for TypeAnalyzer {
        type Output = TypeAnalysis;

        fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
            let shape = self.ctx.types.callable_shape(&shape_id);
            self.analysis.is_callable = true;
            self.analysis.has_type_params = shape.call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
            self.analysis.clone()
        }

        fn visit_function(&mut self, shape_id: u32) -> Self::Output {
            // Reuse logic from visit_callable
            self.visit_callable(shape_id)
        }

        fn visit_union(&mut self, _list_id: u32) -> Self::Output {
            self.analysis.returns_union = true;
            self.analysis.clone()
        }

        fn default_output() -> Self::Output {
            TypeAnalysis::default()
        }
    }

    let mut analyzer = TypeAnalyzer {
        analysis: TypeAnalysis::default(),
    };
    analyzer.visit_type(&self.ctx.types, type_id)
}
```

**Benefits**:
- Complex logic encapsulated in visitor
- Can maintain state across traversal
- Reusable for similar analyses

---

## Example 5: Type Transformation

### BEFORE (Imperative transformation)

```rust
pub fn strip_readonly(&self, type_id: TypeId) -> TypeId {
    match self.ctx.types.lookup(type_id) {
        Some(TypeKey::ReadonlyType(inner)) => *inner,
        Some(TypeKey::Union(list_id)) => {
            let members = self.ctx.types.type_list(list_id);
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.strip_readonly(m))
                .collect();
            self.ctx.types.union(new_members)
        }
        Some(TypeKey::Intersection(list_id)) => {
            let members = self.ctx.types.type_list(list_id);
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.strip_readonly(m))
                .collect();
            self.ctx.types.intersection(new_members)
        }
        _ => type_id,
    }
}
```

### AFTER (Using transformation visitor)

```rust
use crate::solver::visitor::TypeVisitor;

pub fn strip_readonly(&self, type_id: TypeId) -> TypeId {
    struct ReadonlyStripper<'a, 'b> {
        state: &'a CheckerState<'b>,
    }

    impl TypeVisitor for ReadonlyStripper<'_, '_> {
        type Output = TypeId;

        fn visit_readonly_type(&mut self, inner_type: TypeId) -> Self::Output {
            // Recursively strip readonly from inner type
            self.visit_type(self.state.ctx.types, inner_type)
        }

        fn visit_union(&mut self, list_id: u32) -> Self::Output {
            let members = self.state.ctx.types.type_list(list_id);
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.visit_type(self.state.ctx.types, m))
                .collect();
            self.state.ctx.types.union(new_members)
        }

        fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
            let members = self.state.ctx.types.type_list(list_id);
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.visit_type(self.state.ctx.types, m))
                .collect();
            self.state.ctx.types.intersection(new_members)
        }

        fn visit_type(&mut self, types: &TypeInterner, type_id: TypeId) -> Self::Output {
            match types.lookup(type_id) {
                Some(type_key) => self.visit_type_key(types, type_key),
                None => type_id,
            }
        }

        fn default_output() -> Self::Output {
            // Return original type ID by default
            self.current_type_id
        }
    }

    let mut stripper = ReadonlyStripper { state: self };
    stripper.visit_type(&self.ctx.types, type_id)
}
```

**Benefits**:
- Recursive transformation is automatic
- Handles all type variants
- Easy to add new transformations

---

## Built-in Visitors

### TypeKindVisitor

Classify types into broad categories:

```rust
use crate::solver::visitor::{is_type_kind, TypeKind};

let is_primitive = is_type_kind(&types, type_id, TypeKind::Primitive);
let is_object = is_type_kind(&types, type_id, TypeKind::Object);
let is_function = is_type_kind(&types, type_id, TypeKind::Function);
```

### TypeCollectorVisitor

Collect all TypeIds referenced by a type:

```rust
use crate::solver::visitor::collect_referenced_types;

let dependencies = collect_referenced_types(&types, type_id);
for dep_type_id in dependencies {
    println!("Type depends on: {:?}", dep_type_id);
}
```

### TypePredicateVisitor

Test types against custom predicates:

```rust
use crate::solver::visitor::test_type;
use crate::solver::TypeKey;

let is_numeric = test_type(&types, type_id, |key| {
    matches!(key, TypeKey::Intrinsic(IntrinsicKind::Number))
});
```

---

## Creating Custom Visitors

### Example: Count Array Nesting Depth

```rust
use crate::solver::visitor::TypeVisitor;

struct ArrayDepthCounter {
    current_depth: usize,
    max_depth: usize,
}

impl TypeVisitor for ArrayDepthCounter {
    type Output = usize;

    fn visit_array(&mut self, element_type: TypeId) -> Self::Output {
        self.current_depth += 1;
        self.max_depth = self.max_depth.max(self.current_depth);

        // Recursively check element type
        let result = self.visit_type(types, element_type);

        self.current_depth -= 1;
        result
    }

    fn default_output() -> Self::Output {
        self.max_depth
    }
}

// Usage
let mut counter = ArrayDepthCounter {
    current_depth: 0,
    max_depth: 0,
};
let depth = counter.visit_type(&types, array_type_id);
```

### Example: Extract All String Literals

```rust
use crate::solver::visitor::TypeVisitor;
use crate::solver::LiteralValue;

struct StringLiteralExtractor {
    literals: Vec<String>,
}

impl TypeVisitor for StringLiteralExtractor {
    type Output = Vec<String>;

    fn visit_literal(&mut self, value: &LiteralValue) -> Self::Output {
        if let LiteralValue::String(s) = value {
            self.literals.push(s.clone());
        }
        self.literals.clone()
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = types.type_list(list_id);
        for &member in members {
            self.visit_type(types, member);
        }
        self.literals.clone()
    }

    fn default_output() -> Self::Output {
        self.literals.clone()
    }
}
```

---

## Migration Strategy

To migrate existing code to use the visitor pattern:

1. **Identify repetitive patterns**: Look for multiple functions with similar `match self.ctx.types.lookup()` patterns

2. **Choose the right visitor**:
   - Type classification → TypeKindVisitor
   - Dependency collection → TypeCollectorVisitor
   - Custom predicates → TypePredicateVisitor
   - Complex logic → Custom visitor

3. **Replace match statements**:
   ```rust
   // Before
   match self.ctx.types.lookup(type_id) {
       Some(TypeKey::Array(_)) => true,
       _ => false,
   }

   // After
   is_type_kind(&self.ctx.types, type_id, TypeKind::Array)
   ```

4. **Test thoroughly**: Ensure behavior is identical

5. **Refactor**: Extract common patterns into utility functions

---

## Performance Considerations

The visitor pattern has minimal overhead:

- **Zero-cost abstraction**: Visitors compile down to efficient code
- **No dynamic dispatch**: All types are known at compile time
- **Inlinable**: Simple visitors can be inlined by the compiler

For performance-critical code, consider:

- Using `test_type()` for simple predicates (can be inlined)
- Caching visitor results for repeated checks
- Custom visitors with optimized logic

---

## Summary

The Type Visitor Pattern provides:

✅ **Consistency**: Uniform API for type operations
✅ **Safety**: Compiler enforces completeness
✅ **Maintainability**: Centralized type logic
✅ **Extensibility**: Easy to add new operations
✅ **Composability**: Visitors can be combined

Replace repetitive match statements with the visitor pattern to write cleaner, more maintainable type checking code.
