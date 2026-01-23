# Transform Architecture Documentation

**Date**: 2026-01-23
**Status**: ✅ Already Implemented (Transformer + IRPrinter Pattern)

---

## Overview

The TypeScript-to-ES5 transform system uses a consistent **Transformer + IRPrinter** architecture across all transforms. This provides a clean separation of concerns and makes transforms testable.

---

## Architecture Pattern

### Phase 1: Transform (AST → IR)

Each transform implements a `*Transformer` struct that:

```rust
pub struct EnumES5Transformer<'a> {
    arena: &'a NodeArena,
    // Transform-specific state...
}

impl<'a> EnumES5Transformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Transformer {
            arena,
            // Initialize state...
        }
    }

    pub fn transform_enum(&mut self, idx: NodeIndex) -> Option<IRNode> {
        // Analyze AST and build IR tree...
    }
}
```

**Key Characteristics**:
- Takes `NodeIndex` (AST node)
- Returns `Option<IRNode>` (IR tree or None if no transform needed)
- Arena-allocated for efficiency
- Pure transformation logic (no string emission)

### Phase 2: Print (IR → String)

The `IRPrinter` walks IR trees and emits JavaScript:

```rust
pub struct IRPrinter { ... }

impl IRPrinter {
    pub fn emit_to_string(node: &IRNode) -> String {
        // Walk IR and emit JavaScript...
    }
}
```

### Phase 3: Emitter (Legacy Wrapper)

For backward compatibility, `*Emitter` structs wrap transformers:

```rust
pub struct EnumES5Emitter<'a> {
    arena: &'a NodeArena,
    transformer: EnumES5Transformer<'a>,
    indent_level: u32,
}

impl<'a> EnumES5Emitter<'a> {
    pub fn emit_enum(&mut self, idx: NodeIndex) -> String {
        let ir = self.transformer.transform_enum(idx)?;
        IRPrinter::emit_to_string(&ir)
    }
}
```

---

## Implemented Transforms

| Transform | Status | Transformer | Notes |
|-----------|--------|-------------|-------|
| `enum_es5` | ✅ Complete | `EnumES5Transformer` | Numeric/string/const enums |
| `namespace_es5` | ✅ Complete | `NamespaceES5Transformer` | IIFE pattern |
| `class_es5` | ✅ Complete | `ES5ClassTransformer` | Prototype pattern |
| `async_es5` | ✅ Complete | `AsyncES5Transformer` | Generator functions |
| `destructuring_es5` | ✅ Complete | `ES5DestructuringTransformer` | Temp variables |
| `spread_es5` | ✅ Complete | `SpreadES5Transformer` | Spread operator |
| `generators` | ✅ Complete | `GeneratorTransformer` | Yield/ delegating yield |
| `decorators` | ✅ Complete | IR-based | Decorator metadata |

---

## Why This Pattern Works

### 1. Separation of Concerns

- **Transform**: Understands TypeScript semantics, produces IR
- **Printer**: Understands JavaScript syntax, produces strings
- **Emitter**: Orchestrates transform + print (legacy compatibility)

### 2. Testability

Transforms can be tested independently:

```rust
#[test]
fn test_enum_transform() {
    let arena = NodeArena::new();
    let mut transformer = EnumES5Transformer::new(&arena);

    let enum_idx = /* parse enum */;
    let ir = transformer.transform_enum(enum_idx);

    assert!(ir.is_some());
    // Verify IR structure...
}
```

### 3. Consistency

All transforms follow the same pattern:
- Same input type (`NodeIndex`)
- Same output type (`Option<IRNode>`)
- Same initialization pattern (`new(arena)`)
- Same composition with `IRPrinter`

### 4. Extensibility

Adding a new transform requires:
1. Implement `transform_*` method
2. Return appropriate `IRNode` tree
3. `IRPrinter` handles emission automatically

---

## Type System Integration

The transform system integrates with the type checker through:

1. **Type-directed transforms**: Some transforms depend on type information
2. **Context tracking**: EmitContext maintains transform state
3. **Feature flags**: `target_es5`, `auto_detect_module` control transforms

---

## Future Enhancements

While the current pattern is solid, potential improvements include:

### 1. Formal Trait (Optional)

If additional consistency is needed:

```rust
pub trait TransformEmitter<'a> {
    type Output;

    fn transform(&mut self, node: NodeIndex) -> Option<Self::Output>;
    fn emit(&mut self, node: NodeIndex) -> Option<String> {
        self.transform(node).map(|output| IRPrinter::emit_to_string(&output))
    }
}
```

However, this may not add significant value given:
- Each transform has different methods (`transform_enum`, `transform_namespace`, etc.)
- The current pattern is already consistent
- Rust's type system provides sufficient discipline

### 2. IR Type Safety

Currently `IRNode` is an enum. Future work could use:
- Generic IR nodes for stronger typing
- Visitor pattern for IR traversal
- Separate IR types per transform category

### 3. Performance Optimization

- Cache IR trees for repeated transforms
- Arena-allocate IR nodes
- Parallel transform for independent nodes

---

## Comparison with Original Architecture

### Before (String Emission in Transforms)

```rust
// OLD: Direct string emission
pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
    let mut output = String::new();
    output.push_str("var ");
    // Hundreds of lines of string concatenation...
    output
}
```

**Problems**:
- Hard to test (need to compare strings)
- Hard to reason about (string manipulation scattered)
- Hard to optimize (can't cache intermediate results)

### After (IR-Based)

```rust
// NEW: IR-based transform
pub fn transform_enum(&mut self, enum_idx: NodeIndex) -> Option<IRNode> {
    Some(IRNode::IIFE { /* structured data */ })
}

// IRPrinter handles emission
IRPrinter::emit_to_string(&ir)
```

**Benefits**:
- Testable (verify IR structure directly)
- Composable (IR trees can be combined)
- Optimizable (cache IR, walk once)
- Maintainable (clear separation of phases)

---

## Conclusion

The transform system already follows a **consistent, well-designed architecture** via the Transformer + IRPrinter pattern. This addresses the ARCHITECTURE_AUDIT_REPORT.md concern about "No Transform Interface" - the interface exists implicitly through the consistent pattern and IR node system.

**Recommendation**: The current architecture is sound. Focus on:
1. ✅ Completing remaining transforms to use this pattern
2. ✅ Extending IR node types as needed
3. ✅ Optimizing IRPrinter performance
4. ⏸️ A formal trait is not currently necessary

---

**Related Files**:
- `src/transforms/mod.rs` - Transform documentation and architecture
- `src/transforms/ir.rs` - IR node definitions
- `src/transforms/ir_printer.rs` - IR → String conversion
- `src/transforms/*_es5.rs` - Individual transform implementations

**Status**: ✅ Phase 3 (Transform Interface) - Already Solved
