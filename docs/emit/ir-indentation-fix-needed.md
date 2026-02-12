# IR Layer Indentation Fix Needed

## Problem

Nested namespace IIFEs following class exports get 8 spaces of indentation instead of 4.

## Root Cause

In `IRPrinter::emit()` (ir_printer.rs:106-113), when emitting a `Sequence` node:

```rust
if let IRNode::Sequence(nodes) = node {
    for (i, child) in nodes.iter().enumerate() {
        if i > 0 {
            self.write_line();
            self.write_indent();  // ← Problem: adds indent_level * 4 spaces
        }
        self.emit_node(child);
    }
}
```

**Flow for parent namespace A containing [class Point, namespace Point]:**

1. Parent namespace IIFE body is a Sequence: `[class_Point, namespace_Point]`
2. IRPrinter increases indent_level when entering IIFE body
3. First child (class) emits without write_indent()
4. Second child (namespace) gets write_line() + write_indent()
5. write_indent() adds extra indentation based on current indent_level
6. Result: namespace IIFE has extra indentation

## Why indent_level=0 Doesn't Help

Setting `indent_level=0` in declarations.rs only affects the INITIAL indent level. Once IRPrinter starts emitting the parent namespace IIFE:
- It calls `increase_indent()` when entering the IIFE body
- Now indent_level = 1
- Sequence children after the first get write_indent() with indent_level=1
- This adds 4 extra spaces

## Attempted Fixes That Don't Work

1. **Strip whitespace from first line** - The spaces are added at write time, not in the string
2. **Set indent_level to 0** - Already done, but it gets increased during IIFE emission
3. **Handle in declarations.rs** - Too late, the indentation is already in the output string

## Proper Fix Approaches

### Option 1: Special-case namespace IIFEs in IRPrinter

Modify `emit_node()` to detect when emitting a namespace IIFE and skip the write_indent() that would normally be added for sequence children.

**Pros**: Minimal changes, focused fix
**Cons**: Special-casing in the printer feels hacky

### Option 2: Add IR node metadata

Add a flag to IRNode like `skip_sequence_indent: bool` that tells the printer not to add indentation for this node when it's in a sequence.

```rust
IRNode::NamespaceIIFE {
    skip_sequence_indent: true,
    // ...
}
```

**Pros**: Clean, declarative approach
**Cons**: Requires IR node structure changes

### Option 3: Change namespace IR structure

Instead of emitting the namespace as a child in the parent's body sequence, emit it as a sibling statement after the parent closes.

**Pros**: May be more correct architecturally
**Cons**: Large refactoring, may break other things

## Recommended Approach

**Option 2** (IR metadata) is cleanest:

1. Add `skip_sequence_indent` field to relevant IRNode variants
2. Set this flag when creating namespace IIFE nodes
3. Check this flag in `IRPrinter::emit()` before calling write_indent()

## Code Locations

**Problem occurs**:
- `crates/tsz-emitter/src/transforms/ir_printer.rs:106-113` (Sequence emission)

**Where to add fix**:
- `crates/tsz-emitter/src/transforms/ir.rs` (IRNode definition - add metadata field)
- `crates/tsz-emitter/src/transforms/namespace_es5_ir.rs` (Set flag when creating namespace IR)
- `crates/tsz-emitter/src/transforms/ir_printer.rs:106-113` (Check flag before write_indent)

## Test Case

```typescript
namespace A {
    export class Point {
        static Origin: Point = { x: 0, y: 0 };
    }
    export namespace Point {
        var Origin = "";
    }
}
```

Expected: 4 spaces before `(function (Point) {`
Actual: 8 spaces

## Implementation Sketch

```rust
// In ir.rs:
pub enum IRNode {
    NamespaceIIFE {
        skip_sequence_indent: bool,  // ← Add this
        // ... other fields
    },
    // ...
}

// In namespace_es5_ir.rs:
fn create_namespace_iife(...) -> IRNode {
    IRNode::NamespaceIIFE {
        skip_sequence_indent: true,  // ← Set true for nested namespaces
        // ...
    }
}

// In ir_printer.rs:
if let IRNode::Sequence(nodes) = node {
    for (i, child) in nodes.iter().enumerate() {
        if i > 0 {
            self.write_line();
            // Check if child wants to skip indentation
            let should_indent = match child {
                IRNode::NamespaceIIFE { skip_sequence_indent, .. } => !skip_sequence_indent,
                _ => true,
            };
            if should_indent {
                self.write_indent();
            }
        }
        self.emit_node(child);
    }
}
```

## Status

- **Issue identified**: Yes
- **Root cause found**: Yes
- **Fix designed**: Yes
- **Implementation**: Pending

This fix requires modifying core IR structures and should be done carefully with full test coverage.
