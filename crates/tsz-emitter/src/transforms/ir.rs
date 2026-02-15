//! Lowered IR (Intermediate Representation) for Transforms
//!
//! This module defines a tree-structured IR that transforms produce instead of strings.
//! The IR nodes represent JavaScript constructs that the printer can emit.
//!
//! # Architecture
//!
//! Transforms (ES5 class, async, etc.) analyze AST nodes and produce IR trees.
//! The printer then walks these IR trees and emits JavaScript strings.
//!
//! Benefits:
//! - Clean separation between transform logic and string emission
//! - IR is testable independently
//! - Printer can apply formatting consistently
//! - Future optimizations (minification, pretty-print) only need to change the printer
//!
//! # IR Structure
//!
//! The IR is a tree of `IRNode` variants. Each variant represents a JavaScript
//! construct (expression, statement, declaration) that can be emitted.

use tsz_parser::parser::NodeIndex;

/// Intermediate Representation node for transformed JavaScript
#[derive(Debug, Clone)]
pub enum IRNode {
    // =========================================================================
    // Literals
    // =========================================================================
    /// Numeric literal: `42`, `3.14`
    NumericLiteral(String),

    /// String literal: `"hello"`, `'world'`
    StringLiteral(String),

    /// Boolean literal: `true`, `false`
    BooleanLiteral(bool),

    /// Null literal: `null`
    NullLiteral,

    /// Undefined: `void 0`
    Undefined,

    // =========================================================================
    // Identifiers
    // =========================================================================
    /// Identifier: `foo`, `_bar`
    Identifier(String),

    /// This keyword: `this` or `_this` (for captures)
    This { captured: bool },

    /// Super keyword
    Super,

    // =========================================================================
    // Expressions
    // =========================================================================
    /// Binary expression: `left op right`
    BinaryExpr {
        left: Box<Self>,
        operator: String,
        right: Box<Self>,
    },

    /// Unary prefix expression: `!x`, `-x`, `++x`
    PrefixUnaryExpr {
        operator: String,
        operand: Box<Self>,
    },

    /// Unary postfix expression: `x++`, `x--`
    PostfixUnaryExpr {
        operand: Box<Self>,
        operator: String,
    },

    /// Call expression: `callee(args)`
    CallExpr {
        callee: Box<Self>,
        arguments: Vec<Self>,
    },

    /// New expression: `new Callee(args)`
    NewExpr {
        callee: Box<Self>,
        arguments: Vec<Self>,
        explicit_arguments: bool,
    },

    /// Property access: `object.property`
    PropertyAccess { object: Box<Self>, property: String },

    /// Element access: `object[index]`
    ElementAccess { object: Box<Self>, index: Box<Self> },

    /// Conditional expression: `cond ? then : else`
    ConditionalExpr {
        condition: Box<Self>,
        when_true: Box<Self>,
        when_false: Box<Self>,
    },

    /// Parenthesized expression: `(expr)`
    Parenthesized(Box<Self>),

    /// Comma expression: `(a, b, c)`
    CommaExpr(Vec<Self>),

    /// Array literal: `[a, b, c]`
    ArrayLiteral(Vec<Self>),

    /// Spread element: `...expr`
    SpreadElement(Box<Self>),

    /// Object literal: `{ key: value, ... }`
    ObjectLiteral {
        properties: Vec<IRProperty>,
        /// Source range (pos, end) for single-line vs multiline detection
        source_range: Option<(u32, u32)>,
    },

    /// Function expression: `function name(params) { body }`
    FunctionExpr {
        name: Option<String>,
        parameters: Vec<IRParam>,
        body: Vec<Self>,
        /// Whether body is a single expression (for arrow conversion)
        is_expression_body: bool,
        /// Source range of the body block (pos, end) for single-line detection
        body_source_range: Option<(u32, u32)>,
    },

    /// Logical OR: `left || right`
    LogicalOr { left: Box<Self>, right: Box<Self> },

    /// Logical AND: `left && right`
    LogicalAnd { left: Box<Self>, right: Box<Self> },

    // =========================================================================
    // Statements
    // =========================================================================
    /// Variable declaration: `var x = value;`
    VarDecl {
        name: String,
        initializer: Option<Box<Self>>,
    },

    /// Multiple variable declarations: `var a = 1, b = 2;`
    VarDeclList(Vec<Self>),

    /// Expression statement: `expr;`
    ExpressionStatement(Box<Self>),

    /// Return statement: `return expr;`
    ReturnStatement(Option<Box<Self>>),

    /// If statement: `if (cond) { then } else { else }`
    IfStatement {
        condition: Box<Self>,
        then_branch: Box<Self>,
        else_branch: Option<Box<Self>>,
    },

    /// Block statement: `{ statements }`
    Block(Vec<Self>),

    /// Empty statement: `;`
    EmptyStatement,

    /// Switch statement
    SwitchStatement {
        expression: Box<Self>,
        cases: Vec<IRSwitchCase>,
    },

    /// For statement: `for (init; cond; incr) { body }`
    ForStatement {
        initializer: Option<Box<Self>>,
        condition: Option<Box<Self>>,
        incrementor: Option<Box<Self>>,
        body: Box<Self>,
    },

    /// While statement: `while (cond) { body }`
    WhileStatement {
        condition: Box<Self>,
        body: Box<Self>,
    },

    /// Do-while statement: `do { body } while (cond)`
    DoWhileStatement {
        body: Box<Self>,
        condition: Box<Self>,
    },

    /// Try statement: `try { block } catch (e) { handler } finally { finalizer }`
    TryStatement {
        try_block: Box<Self>,
        catch_clause: Option<IRCatchClause>,
        finally_block: Option<Box<Self>>,
    },

    /// Throw statement: `throw expr;`
    ThrowStatement(Box<Self>),

    /// Break statement: `break;` or `break label;`
    BreakStatement(Option<String>),

    /// Continue statement: `continue;` or `continue label;`
    ContinueStatement(Option<String>),

    /// Labeled statement: `label: stmt`
    LabeledStatement { label: String, statement: Box<Self> },

    // =========================================================================
    // Declarations
    // =========================================================================
    /// Function declaration: `function name(params) { body }`
    FunctionDecl {
        name: String,
        parameters: Vec<IRParam>,
        body: Vec<Self>,
        /// Source range of the body block (for preserving single-line formatting)
        body_source_range: Option<(u32, u32)>,
    },

    // =========================================================================
    // ES5 Class Transform Specific
    // =========================================================================
    /// IIFE pattern for ES5 class:
    /// `var ClassName = /** @class */ (function (_super) { ... }(BaseClass));`
    ES5ClassIIFE {
        name: String,
        base_class: Option<Box<Self>>,
        body: Vec<Self>,
        /// `WeakMap` declarations for private fields (before the IIFE)
        weakmap_decls: Vec<String>,
        /// `WeakMap` instantiations (after the IIFE)
        weakmap_inits: Vec<String>,
    },

    /// __extends helper call: `__extends(ClassName, _super);`
    ExtendsHelper { class_name: String },

    /// Prototype method assignment: `ClassName.prototype.method = function() {...};`
    PrototypeMethod {
        class_name: String,
        method_name: IRMethodName,
        function: Box<Self>,
        /// Leading `JSDoc` or block comment from the original method declaration
        leading_comment: Option<String>,
        /// Trailing comment from the original method declaration line
        trailing_comment: Option<String>,
    },

    /// Static method assignment: `ClassName.method = function() {...};`
    StaticMethod {
        class_name: String,
        method_name: IRMethodName,
        function: Box<Self>,
        /// Leading `JSDoc` or block comment from the original method declaration
        leading_comment: Option<String>,
        /// Trailing comment from the original method declaration line
        trailing_comment: Option<String>,
    },

    /// Object.defineProperty for getters/setters
    DefineProperty {
        target: Box<Self>,
        property_name: IRMethodName,
        descriptor: IRPropertyDescriptor,
    },

    // =========================================================================
    // Async Transform Specific
    // =========================================================================
    /// __awaiter helper call
    AwaiterCall {
        this_arg: Box<Self>,
        generator_body: Box<Self>,
    },

    /// __generator helper body
    GeneratorBody {
        /// Whether this uses switch/case (has await) or simple return
        has_await: bool,
        /// Generator state machine cases
        cases: Vec<IRGeneratorCase>,
    },

    /// Generator operation: `[opcode, value]`
    GeneratorOp {
        opcode: u32,
        value: Option<Box<Self>>,
        comment: Option<String>,
    },

    /// _`a.sent()` - get the sent value in generator
    GeneratorSent,

    /// _a.label - the label property
    GeneratorLabel,

    // =========================================================================
    // Private Field Helpers
    // =========================================================================
    /// __classPrivateFieldGet(receiver, weakmap, "f")
    PrivateFieldGet {
        receiver: Box<Self>,
        weakmap_name: String,
    },

    /// __classPrivateFieldSet(receiver, weakmap, value, "f")
    PrivateFieldSet {
        receiver: Box<Self>,
        weakmap_name: String,
        value: Box<Self>,
    },

    /// WeakMap.set for private field init: `_weakmap.set(this, value);`
    WeakMapSet {
        weakmap_name: String,
        key: Box<Self>,
        value: Box<Self>,
    },

    // =========================================================================
    // Special
    // =========================================================================
    /// Raw JavaScript string (escape hatch for complex cases)
    Raw(String),

    /// Comment: `/* text */` or `// text`
    Comment { text: String, is_block: bool },

    /// Trailing comment that should be emitted on the same line as the previous node.
    /// Used for comments like `M.x = ""; //comment` inside namespace bodies.
    /// The text includes delimiters (e.g., `//comment` or `/* comment */`).
    TrailingComment(String),

    /// Sequence of statements/nodes
    Sequence(Vec<Self>),

    /// Reference to an original AST node (for passthrough)
    ASTRef(NodeIndex),

    /// Reference to an original AST node with constrained source range.
    /// Used when the parser's node.end extends into a parent block's closing brace.
    ASTRefRange(NodeIndex, u32),

    // =========================================================================
    // Module IR Nodes
    // =========================================================================
    /// "use strict" directive
    UseStrict,

    /// `Object.defineProperty(exports, "__esModule", { value: true });`
    EsesModuleMarker,

    /// `exports.name = void 0;` (export initialization)
    ExportInit { name: String },

    /// `var module = require("module");` (require statement)
    RequireStatement {
        var_name: String,
        module_spec: String,
    },

    /// `import foo from "module";` -> `var foo = module.foo;` (default import)
    DefaultImport {
        var_name: String,
        module_var: String,
    },

    /// `import * as ns from "module";` -> `var ns = require("module");` (namespace import)
    NamespaceImport {
        var_name: String,
        module_var: String,
    },

    /// `import { foo } from "module";` -> `var foo = module.foo;` (named import)
    NamedImport {
        var_name: String,
        module_var: String,
        import_name: String,
    },

    /// `export default value;` -> `exports.default = value;`
    ExportAssignment { name: String },

    /// `export { foo as bar } from "module";` (re-export)
    ReExportProperty {
        export_name: String,
        module_var: String,
        import_name: String,
    },

    // =========================================================================
    // Enum / Namespace IR Nodes
    // =========================================================================
    /// Enum IIFE: `(function (E) { ... })(E || (E = {}))`
    EnumIIFE {
        name: String,
        members: Vec<EnumMember>,
    },

    /// Namespace IIFE: `(function (NS) { ... })(NS || (NS = {}))`
    NamespaceIIFE {
        name: String,
        name_parts: Vec<String>,
        body: Vec<Self>,
        is_exported: bool,
        attach_to_exports: bool,
        /// Whether to emit the `var name;` declaration for this namespace.
        /// Set to false when merging with a class/function/enum that already declared it.
        should_declare_var: bool,
        /// Parent namespace name for qualified binding: `NS = Parent.NS || (Parent.NS = {})`
        parent_name: Option<String>,
        /// Renamed IIFE parameter name when a member collides with the namespace name.
        /// E.g., namespace A { export class A {} } => `(function (A_1) { ... A_1.A = A; })`
        /// Only the function parameter and namespace exports use this name;
        /// the var declaration and argument still use the original name.
        param_name: Option<String>,
        /// Skip automatic indentation when this node is in a Sequence (after the first child).
        /// Used for nested namespace IIFEs that should align with their siblings rather than
        /// being indented as regular statements. This prevents double-indentation when a
        /// namespace IIFE follows a class/enum/function in a parent namespace body.
        skip_sequence_indent: bool,
    },

    /// Namespace export: `NS.foo = ...;`
    NamespaceExport {
        namespace: String,
        name: String,
        value: Box<Self>,
    },
}

/// Enum member representation for IR
#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub value: EnumMemberValue,
}

/// Enum member value representation
#[derive(Debug, Clone)]
pub enum EnumMemberValue {
    /// Auto-incremented numeric value
    Auto(i64),
    /// Explicit numeric value
    Numeric(i64),
    /// String value
    String(String),
    /// Computed expression (not a simple literal)
    Computed(Box<IRNode>),
}

/// Property in an object literal
#[derive(Debug, Clone)]
pub struct IRProperty {
    pub key: IRPropertyKey,
    pub value: IRNode,
    pub kind: IRPropertyKind,
}

/// Object property key
#[derive(Debug, Clone)]
pub enum IRPropertyKey {
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(String),
    Computed(Box<IRNode>),
}

/// Object property kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IRPropertyKind {
    Init,
    Get,
    Set,
}

/// Method name (for prototype/static assignments)
#[derive(Debug, Clone)]
pub enum IRMethodName {
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(String),
    Computed(Box<IRNode>),
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct IRParam {
    pub name: String,
    pub rest: bool,
    pub default_value: Option<Box<IRNode>>,
}

/// Switch case
#[derive(Debug, Clone)]
pub struct IRSwitchCase {
    pub test: Option<IRNode>, // None for default case
    pub statements: Vec<IRNode>,
}

/// Catch clause
#[derive(Debug, Clone)]
pub struct IRCatchClause {
    pub param: Option<String>,
    pub body: Vec<IRNode>,
}

/// Property descriptor for Object.defineProperty
#[derive(Debug, Clone)]
pub struct IRPropertyDescriptor {
    pub get: Option<Box<IRNode>>,
    pub set: Option<Box<IRNode>>,
    pub enumerable: bool,
    pub configurable: bool,
}

/// Generator case (for async state machine)
#[derive(Debug, Clone)]
pub struct IRGeneratorCase {
    pub label: u32,
    pub statements: Vec<IRNode>,
}

// =========================================================================
// Builder helpers for IR construction
// =========================================================================

impl IRNode {
    /// Create an identifier node
    pub fn id(name: impl Into<String>) -> Self {
        Self::Identifier(name.into())
    }

    /// Create a string literal
    pub fn string(s: impl Into<String>) -> Self {
        Self::StringLiteral(s.into())
    }

    /// Create a numeric literal
    pub fn number(n: impl Into<String>) -> Self {
        Self::NumericLiteral(n.into())
    }

    /// Create a call expression
    pub fn call(callee: Self, args: Vec<Self>) -> Self {
        Self::CallExpr {
            callee: Box::new(callee),
            arguments: args,
        }
    }

    /// Create a property access
    pub fn prop(object: Self, property: impl Into<String>) -> Self {
        Self::PropertyAccess {
            object: Box::new(object),
            property: property.into(),
        }
    }

    /// Create an element access
    pub fn elem(object: Self, index: Self) -> Self {
        Self::ElementAccess {
            object: Box::new(object),
            index: Box::new(index),
        }
    }

    /// Create a binary expression
    pub fn binary(left: Self, op: impl Into<String>, right: Self) -> Self {
        Self::BinaryExpr {
            left: Box::new(left),
            operator: op.into(),
            right: Box::new(right),
        }
    }

    /// Create an assignment expression
    pub fn assign(target: Self, value: Self) -> Self {
        Self::BinaryExpr {
            left: Box::new(target),
            operator: "=".to_string(),
            right: Box::new(value),
        }
    }

    /// Create a var declaration
    pub fn var_decl(name: impl Into<String>, init: Option<Self>) -> Self {
        Self::VarDecl {
            name: name.into(),
            initializer: init.map(Box::new),
        }
    }

    /// Create a return statement
    pub fn ret(expr: Option<Self>) -> Self {
        Self::ReturnStatement(expr.map(Box::new))
    }

    /// Create a function expression
    pub const fn func_expr(name: Option<String>, params: Vec<IRParam>, body: Vec<Self>) -> Self {
        Self::FunctionExpr {
            name,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Create a function declaration
    pub fn func_decl(name: impl Into<String>, params: Vec<IRParam>, body: Vec<Self>) -> Self {
        Self::FunctionDecl {
            name: name.into(),
            parameters: params,
            body,
            body_source_range: None,
        }
    }

    /// Create `this` reference
    pub const fn this() -> Self {
        Self::This { captured: false }
    }

    /// Create `_this` reference (captured)
    pub const fn this_captured() -> Self {
        Self::This { captured: true }
    }

    /// Create `void 0`
    pub const fn void_0() -> Self {
        Self::Undefined
    }

    /// Wrap in parentheses
    pub fn paren(self) -> Self {
        Self::Parenthesized(Box::new(self))
    }

    /// Create a block
    pub const fn block(stmts: Vec<Self>) -> Self {
        Self::Block(stmts)
    }

    /// Create an expression statement
    pub fn expr_stmt(expr: Self) -> Self {
        Self::ExpressionStatement(Box::new(expr))
    }

    /// Create an object literal
    pub const fn object(props: Vec<IRProperty>) -> Self {
        Self::ObjectLiteral {
            properties: props,
            source_range: None,
        }
    }

    /// Create an object literal with source range for formatting
    pub const fn object_with_source(props: Vec<IRProperty>, source_range: (u32, u32)) -> Self {
        Self::ObjectLiteral {
            properties: props,
            source_range: Some(source_range),
        }
    }

    /// Create an empty object literal
    pub const fn empty_object() -> Self {
        Self::ObjectLiteral {
            properties: Vec::new(),
            source_range: None,
        }
    }

    /// Create an array literal
    pub const fn array(elements: Vec<Self>) -> Self {
        Self::ArrayLiteral(elements)
    }

    /// Create an empty array literal
    pub const fn empty_array() -> Self {
        Self::ArrayLiteral(Vec::new())
    }

    /// Create a logical OR expression: `left || right`
    pub fn logical_or(left: Self, right: Self) -> Self {
        Self::LogicalOr {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a logical AND expression: `left && right`
    pub fn logical_and(left: Self, right: Self) -> Self {
        Self::LogicalAnd {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a sequence of statements
    pub const fn sequence(nodes: Vec<Self>) -> Self {
        Self::Sequence(nodes)
    }

    /// Create a new expression: `new Constructor(args)`
    pub fn new_expr(callee: Self, args: Vec<Self>, explicit_args: bool) -> Self {
        Self::NewExpr {
            callee: Box::new(callee),
            arguments: args,
            explicit_arguments: explicit_args,
        }
    }
}

impl IRParam {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rest: false,
            default_value: None,
        }
    }

    pub fn rest(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rest: true,
            default_value: None,
        }
    }

    pub fn with_default(mut self, default: IRNode) -> Self {
        self.default_value = Some(Box::new(default));
        self
    }
}

impl IRProperty {
    /// Create a simple property with identifier key: `{ key: value }`
    pub fn init(key: impl Into<String>, value: IRNode) -> Self {
        Self {
            key: IRPropertyKey::Identifier(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }

    /// Create a property with string literal key: `{ "key": value }`
    pub fn init_string(key: impl Into<String>, value: IRNode) -> Self {
        Self {
            key: IRPropertyKey::StringLiteral(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }

    /// Create a getter property
    pub fn getter(key: impl Into<String>, get: IRNode) -> Self {
        Self {
            key: IRPropertyKey::Identifier(key.into()),
            value: get,
            kind: IRPropertyKind::Get,
        }
    }

    /// Create a setter property
    pub fn setter(key: impl Into<String>, set: IRNode) -> Self {
        Self {
            key: IRPropertyKey::Identifier(key.into()),
            value: set,
            kind: IRPropertyKind::Set,
        }
    }
}
