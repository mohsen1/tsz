pub mod emit_utils;
pub mod passes;

use std::fmt;

/// Represents the Lowered Intermediate Representation (LIR).
///
/// This structure serves as the Abstract Syntax Tree for the output C code.
/// It is designed to be simple enough to easily emit to C strings, but structured
/// enough to allow for transformation passes (e.g., dead code elimination, constant folding).
#[derive(Debug, Clone, PartialEq)]
pub enum LirNode {
    /// A top-level file or translation unit. Contains a list of declarations and functions.
    Module(Vec<LirNode>),

    /// A struct definition.
    StructDef {
        name: String,
        /// List of (type, name) pairs for fields.
        fields: Vec<(String, String)>,
    },

    /// A function definition.
    Function {
        return_type: String,
        name: String,
        /// List of (type, name) pairs for arguments.
        args: Vec<(String, String)>,
        body: Box<LirNode>,
    },

    /// A block of code { ... }.
    Block(Vec<LirNode>),

    /// Variable declaration with optional initialization.
    VarDecl {
        vtype: String,
        name: String,
        init: Option<Box<LirNode>>,
    },

    /// An assignment operation: lhs = rhs
    Assignment {
        lhs: Box<LirNode>,
        rhs: Box<LirNode>,
    },

    /// A function call or expression.
    Call {
        func: String,
        args: Vec<LirNode>,
    },

    /// An if statement (else optional).
    If {
        condition: Box<LirNode>,
        then_branch: Box<LirNode>,
        else_branch: Option<Box<LirNode>>,
    },

    /// A while loop.
    While {
        condition: Box<LirNode>,
        body: Box<LirNode>,
    },

    /// A Return statement.
    Return(Option<Box<LirNode>>),

    /// A binary operation (e.g., +, -, ==).
    BinOp {
        left: Box<LirNode>,
        op: String,
        right: Box<LirNode>,
    },

    /// A unary operation (e.g., -, !, *).
    UnOp {
        op: String,
        operand: Box<LirNode>,
    },

    /// A literal value (integer, float, string, char).
    Literal(Literal),

    /// A reference to a variable.
    Identifier(String),

    /// Field access on a struct or pointer.
    MemberAccess {
        object: Box<LirNode>,
        field: String,
        is_pointer: bool, // True for ->, False for .
    },

    /// A C-style comment.
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),
}

impl fmt::Display for LirNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Delegate to the emitter utility for the actual string generation
        write!(f, "{}", emit_utils::emit_node(self))
    }
}
```

### `src/transforms/emit_utils.rs`
This file contains the logic to transform the `LirNode` AST into a valid C string. It replaces direct string manipulation with structural rendering.

```rust
//
