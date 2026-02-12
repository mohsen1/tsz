# Session: Comment Preservation for ES5 Class Methods

**Date**: 2026-02-12
**Goal**: Improve emit test pass rate (Slice 1: Comment preservation)
**Status**: ✅ Partial success - JSDoc comments now preserved for ES5 class methods

## Problem

JSDoc comments (/** ... */) on class methods were being stripped during ES5 class transformation:

```typescript
// Input
class C {
  /**
   * Returns bar
   */
  public static foo(): string {
    return "bar";
  }
}

// Expected output
var C = /** @class */ (function () {
    function C() {
    }
    /**
     * Returns bar
     */
    C.foo = function () {
        return "bar";
    };
    return C;
}());

// Actual output (before fix)
var C = /** @class */ (function () {
    function C() {
    }
    C.foo = function () {  // ❌ Comment missing!
        return "bar";
    };
    return C;
}());
```

## Root Cause

The ES5 class transformation system was missing comment handling:

1. **IR nodes lacked comment storage** - `StaticMethod` and `PrototypeMethod` variants had no field for comments
2. **Transform didn't extract comments** - `ES5ClassTransformer` didn't look for JSDoc
3. **Printer couldn't emit what it didn't have** - IRPrinter had no comment data
4. **Source text wasn't auto-extracted** - Printer required explicit `set_source_text()` call

### Technical Challenge

TypeScript's `get_leading_comment_ranges(text, pos)` scans **forward** from `pos`, but JSDoc comments appear **before** method declarations. The method's `node.pos` points to where "public" starts (after the comment ends).

Solution: Scan **backwards** from `node.pos` to find the class opening brace or previous member, then scan forward to find comments that end before the method starts.

## Solution

### 1. Added Comment Storage to IR Nodes

**File**: `crates/tsz-emitter/src/transforms/ir.rs`

```rust
/// Static method assignment: `ClassName.method = function() {...};`
StaticMethod {
    class_name: String,
    method_name: IRMethodName,
    function: Box<IRNode>,
    /// Leading JSDoc or block comment from the original method declaration
    leading_comment: Option<String>,  // ← Added
},

/// Prototype method assignment: `ClassName.prototype.method = function() {...};`
PrototypeMethod {
    class_name: String,
    method_name: IRMethodName,
    function: Box<IRNode>,
    /// Leading JSDoc or block comment from the original method declaration
    leading_comment: Option<String>,  // ← Added
},
```

### 2. Implemented Comment Extraction

**File**: `crates/tsz-emitter/src/transforms/class_es5_ir.rs`

Added `extract_leading_comment()` method that:
- Scans backwards from method position to class opening brace or previous member
- Finds comments that end before the method starts
- Filters for JSDoc-style comments (`/**` ... `*/`)

```rust
fn extract_leading_comment(&self, node: &Node) -> Option<String> {
    let source_text = self.source_text?;

    // Scan backwards to find opening brace or end of previous member
    let mut search_pos = node.pos as usize;
    let bytes = source_text.as_bytes();
    let mut brace_depth = 0;

    while search_pos > 0 {
        let ch = bytes[search_pos - 1];
        if ch == b'}' {
            brace_depth += 1;
        } else if ch == b'{' {
            if brace_depth == 0 {
                break;  // Found class opening brace
            }
            brace_depth -= 1;
        } else if ch == b';' && brace_depth == 0 {
            break;  // Found end of previous member
        }
        search_pos -= 1;
    }

    // Scan forward from search_pos to find comments
    let comments = get_leading_comment_ranges(source_text, search_pos);

    // Return last JSDoc comment that ends before node.pos
    for comment in comments.iter().rev() {
        if comment.end <= node.pos {
            let text = &source_text[comment.pos as usize..comment.end as usize];
            if text.starts_with("/**") {
                return Some(text.to_string());
            }
        }
    }

    None
}
```

### 3. Auto-Extract Source Text from Arena

**File**: `crates/tsz-emitter/src/printer.rs`

Modified `Printer::print()` to automatically extract source text from the arena's SourceFile:

```rust
pub fn print(&mut self, root: NodeIndex) {
    // ... configure settings ...

    // Extract source text from arena if not already set (for comment preservation)
    if self.inner.source_text.is_none() {
        if let Some(node) = self.inner.arena.get(root) {
            if let Some(source_file) = self.inner.arena.get_source_file(node) {
                let text_ref: &'a str = &source_file.text;
                self.inner.source_text = Some(text_ref);
            }
        }
    }

    self.inner.emit(root);
}
```

### 4. Normalized Comment Indentation

**File**: `crates/tsz-emitter/src/transforms/ir_printer.rs`

Added `emit_multiline_comment()` helper that formats JSDoc with proper indentation:

```rust
fn emit_multiline_comment(&mut self, comment: &str) {
    let mut first = true;
    for line in comment.split('\n') {
        if !first {
            self.write_line();
            self.write_indent();
        }
        // Strip leading whitespace, then add one space before * or */
        let trimmed = line.trim_start();
        if !first && (trimmed.starts_with('*') || trimmed.starts_with('/')) {
            self.write(" ");  // One space before continuation lines
        }
        self.write(trimmed.trim_end());
        first = false;
    }
}
```

Then emit comments before method assignments:

```rust
IRNode::StaticMethod {
    class_name,
    method_name,
    function,
    leading_comment,
} => {
    // Emit leading JSDoc comment if present
    if let Some(comment) = leading_comment {
        self.emit_multiline_comment(comment);
        self.write_line();
        self.write_indent();
    }
    self.write(class_name);
    self.emit_method_name(method_name);
    self.write(" = ");
    self.emit_node(function);
    self.write(";");
}
```

## Results

### Test: commentBeforeStaticMethod1
**Status**: ✅ **PASSING**

**Before**:
```javascript
var C = /** @class */ (function () {
    function C() {
    }
    C.foo = function () {  // ❌ Comment missing
        return "bar";
    };
    return C;
}());
```

**After**:
```javascript
var C = /** @class */ (function () {
    function C() {
    }
    /**
     * Returns bar
     */
    C.foo = function () {  // ✅ Comment preserved with correct indentation
        return "bar";
    };
    return C;
}());
```

### Overall Progress

**Comment-related tests**: 6/50 passing (12%)
- Before this session: 3/20 passing (15%) on initial sample
- After JSDoc implementation: 6/50 passing (12%) on broader sample
- Specific win: `commentBeforeStaticMethod1` and similar tests now pass

## Remaining Work (Slice 1: Comment Preservation)

### High Priority
1. **Inline/trailing comments in expressions** - Comments within call arguments, etc.
2. **End-of-file comments** - Trailing comments after all statements
3. **Line comments forcing newlines** - `s.map(// comment\nfunction...)` formatting

### Medium Priority
4. **Comments in empty parameter lists** - `function foo(/* comment */) {}`
5. **Comments on ambient declarations** - Preserving pinned comments
6. **AMD module comments** - Special handling for AMD directives

### Requires Different Approach
- Comments in array literals, object literals
- Comments in parenthesized expressions
- Comments in destructuring patterns

### Key Insight

The foundation is now in place for comment preservation in the IR-based transform system:
- IR nodes can store comments
- Extracting comments from source works
- Auto-detection of source text enables it everywhere
- Comment formatting matches TypeScript output

Future comment preservation work can follow this pattern:
1. Add `leading_comment`/`trailing_comment` fields to relevant IR nodes
2. Extract comments during transformation
3. Emit comments in IR printer

## Commit

**Hash**: 38f9e18f3 (after rebase)
**Message**: `feat(emit): preserve JSDoc comments for ES5 class methods`

**Files Changed**:
- `crates/tsz-emitter/src/transforms/ir.rs` - Added comment fields
- `crates/tsz-emitter/src/transforms/class_es5_ir.rs` - Comment extraction
- `crates/tsz-emitter/src/transforms/class_es5.rs` - Pass source_text to transformer
- `crates/tsz-emitter/src/transforms/es5.rs` - Updated IR construction sites
- `crates/tsz-emitter/src/transforms/ir_printer.rs` - Comment emission
- `crates/tsz-emitter/src/printer.rs` - Auto-extract source text
