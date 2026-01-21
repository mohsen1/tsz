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

use crate::parser::NodeIndex;

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
        left: Box<IRNode>,
        operator: String,
        right: Box<IRNode>,
    },

    /// Unary prefix expression: `!x`, `-x`, `++x`
    PrefixUnaryExpr {
        operator: String,
        operand: Box<IRNode>,
    },

    /// Unary postfix expression: `x++`, `x--`
    PostfixUnaryExpr {
        operand: Box<IRNode>,
        operator: String,
    },

    /// Call expression: `callee(args)`
    CallExpr {
        callee: Box<IRNode>,
        arguments: Vec<IRNode>,
    },

    /// New expression: `new Callee(args)`
    NewExpr {
        callee: Box<IRNode>,
        arguments: Vec<IRNode>,
    },

    /// Property access: `object.property`
    PropertyAccess {
        object: Box<IRNode>,
        property: String,
    },

    /// Element access: `object[index]`
    ElementAccess {
        object: Box<IRNode>,
        index: Box<IRNode>,
    },

    /// Conditional expression: `cond ? then : else`
    ConditionalExpr {
        condition: Box<IRNode>,
        when_true: Box<IRNode>,
        when_false: Box<IRNode>,
    },

    /// Parenthesized expression: `(expr)`
    Parenthesized(Box<IRNode>),

    /// Comma expression: `(a, b, c)`
    CommaExpr(Vec<IRNode>),

    /// Array literal: `[a, b, c]`
    ArrayLiteral(Vec<IRNode>),

    /// Object literal: `{ key: value, ... }`
    ObjectLiteral(Vec<IRProperty>),

    /// Function expression: `function name(params) { body }`
    FunctionExpr {
        name: Option<String>,
        parameters: Vec<IRParam>,
        body: Vec<IRNode>,
        /// Whether body is a single expression (for arrow conversion)
        is_expression_body: bool,
    },

    /// Logical OR: `left || right`
    LogicalOr {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },

    /// Logical AND: `left && right`
    LogicalAnd {
        left: Box<IRNode>,
        right: Box<IRNode>,
    },

    // =========================================================================
    // Statements
    // =========================================================================
    /// Variable declaration: `var x = value;`
    VarDecl {
        name: String,
        initializer: Option<Box<IRNode>>,
    },

    /// Multiple variable declarations: `var a = 1, b = 2;`
    VarDeclList(Vec<IRNode>),

    /// Expression statement: `expr;`
    ExpressionStatement(Box<IRNode>),

    /// Return statement: `return expr;`
    ReturnStatement(Option<Box<IRNode>>),

    /// If statement: `if (cond) { then } else { else }`
    IfStatement {
        condition: Box<IRNode>,
        then_branch: Box<IRNode>,
        else_branch: Option<Box<IRNode>>,
    },

    /// Block statement: `{ statements }`
    Block(Vec<IRNode>),

    /// Empty statement: `;`
    EmptyStatement,

    /// Switch statement
    SwitchStatement {
        expression: Box<IRNode>,
        cases: Vec<IRSwitchCase>,
    },

    /// For statement: `for (init; cond; incr) { body }`
    ForStatement {
        initializer: Option<Box<IRNode>>,
        condition: Option<Box<IRNode>>,
        incrementor: Option<Box<IRNode>>,
        body: Box<IRNode>,
    },

    /// While statement: `while (cond) { body }`
    WhileStatement {
        condition: Box<IRNode>,
        body: Box<IRNode>,
    },

    /// Do-while statement: `do { body } while (cond)`
    DoWhileStatement {
        body: Box<IRNode>,
        condition: Box<IRNode>,
    },

    /// Try statement: `try { block } catch (e) { handler } finally { finalizer }`
    TryStatement {
        try_block: Box<IRNode>,
        catch_clause: Option<IRCatchClause>,
        finally_block: Option<Box<IRNode>>,
    },

    /// Throw statement: `throw expr;`
    ThrowStatement(Box<IRNode>),

    /// Break statement: `break;` or `break label;`
    BreakStatement(Option<String>),

    /// Continue statement: `continue;` or `continue label;`
    ContinueStatement(Option<String>),

    /// Labeled statement: `label: stmt`
    LabeledStatement {
        label: String,
        statement: Box<IRNode>,
    },

    // =========================================================================
    // Declarations
    // =========================================================================
    /// Function declaration: `function name(params) { body }`
    FunctionDecl {
        name: String,
        parameters: Vec<IRParam>,
        body: Vec<IRNode>,
    },

    // =========================================================================
    // ES5 Class Transform Specific
    // =========================================================================
    /// IIFE pattern for ES5 class:
    /// `var ClassName = /** @class */ (function (_super) { ... }(BaseClass));`
    ES5ClassIIFE {
        name: String,
        base_class: Option<Box<IRNode>>,
        body: Vec<IRNode>,
        /// WeakMap declarations for private fields (before the IIFE)
        weakmap_decls: Vec<String>,
        /// WeakMap instantiations (after the IIFE)
        weakmap_inits: Vec<String>,
    },

    /// __extends helper call: `__extends(ClassName, _super);`
    ExtendsHelper { class_name: String },

    /// Prototype method assignment: `ClassName.prototype.method = function() {...};`
    PrototypeMethod {
        class_name: String,
        method_name: IRMethodName,
        function: Box<IRNode>,
    },

    /// Static method assignment: `ClassName.method = function() {...};`
    StaticMethod {
        class_name: String,
        method_name: IRMethodName,
        function: Box<IRNode>,
    },

    /// Object.defineProperty for getters/setters
    DefineProperty {
        target: Box<IRNode>,
        property_name: String,
        descriptor: IRPropertyDescriptor,
    },

    // =========================================================================
    // Async Transform Specific
    // =========================================================================
    /// __awaiter helper call
    AwaiterCall {
        this_arg: Box<IRNode>,
        generator_body: Box<IRNode>,
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
        value: Option<Box<IRNode>>,
        comment: Option<String>,
    },

    /// _a.sent() - get the sent value in generator
    GeneratorSent,

    /// _a.label - the label property
    GeneratorLabel,

    // =========================================================================
    // Private Field Helpers
    // =========================================================================
    /// __classPrivateFieldGet(receiver, weakmap, "f")
    PrivateFieldGet {
        receiver: Box<IRNode>,
        weakmap_name: String,
    },

    /// __classPrivateFieldSet(receiver, weakmap, value, "f")
    PrivateFieldSet {
        receiver: Box<IRNode>,
        weakmap_name: String,
        value: Box<IRNode>,
    },

    /// WeakMap.set for private field init: `_weakmap.set(this, value);`
    WeakMapSet {
        weakmap_name: String,
        key: Box<IRNode>,
        value: Box<IRNode>,
    },

    // =========================================================================
    // Special
    // =========================================================================
    /// Raw JavaScript string (escape hatch for complex cases)
    Raw(String),

    /// Comment: `/* text */` or `// text`
    Comment { text: String, is_block: bool },

    /// Sequence of statements/nodes
    Sequence(Vec<IRNode>),

    /// Reference to an original AST node (for passthrough)
    ASTRef(NodeIndex),

    // =========================================================================
    // Module Transform Specific (CommonJS)
    // =========================================================================
    /// Use strict directive: `"use strict";`
    UseStrict,

    /// ES6 module marker comment: `/// <reference types="node" />` style marker
    EsesModuleMarker,

    /// Export initialization: Object.create for exports
    ExportInit { name: String },

    /// Require statement: `var_name = require("module_spec")`
    RequireStatement {
        var_name: String,
        module_spec: String,
    },

    /// Default import: `var_name = module_var.default;`
    DefaultImport {
        var_name: String,
        module_var: String,
    },

    /// Namespace import: `var_name = module_var;`
    NamespaceImport {
        var_name: String,
        module_var: String,
    },

    /// Named import: `var_name = module_var.import_name;`
    NamedImport {
        var_name: String,
        module_var: String,
        import_name: String,
    },

    /// Export assignment: `exports.name = value;` or `module.exports = value;`
    ExportAssignment { name: String },

    /// Re-export property: `exports.export_name = module_var.import_name;`
    ReExportProperty {
        export_name: String,
        module_var: String,
        import_name: String,
    },

    // =========================================================================
    // Namespace Transform Specific (IIFE)
    // =========================================================================
    /// Namespace IIFE: `(function (Name1) { ... })(Name1 || (Name1 = {}));`
    NamespaceIIFE {
        name_parts: Vec<String>,
        body: Vec<IRNode>,
        is_exported: bool,
        attach_to_exports: bool,
    },

    /// Namespace export: `Namespace.name = value;`
    NamespaceExport {
        namespace: String,
        name: String,
    },

    // =========================================================================
    // Enum Transform Specific
    // =========================================================================
    /// Enum IIFE: `var EnumName; (function (EnumName) { ... })(EnumName || (EnumName = {}));`
    EnumIIFE {
        name: String,
        members: Vec<EnumMember>,
    },
}

/// Enum member for EnumIIFE transform
#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub value: EnumMemberValue,
}

/// Enum member value
#[derive(Debug, Clone)]
pub enum EnumMemberValue {
    /// Auto-incremented numeric value
    Auto(i64),
    /// Explicit numeric value
    Numeric(i64),
    /// String value
    String(String),
    /// Computed expression
    Computed(IRNode),
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
        IRNode::Identifier(name.into())
    }

    /// Create a string literal
    pub fn string(s: impl Into<String>) -> Self {
        IRNode::StringLiteral(s.into())
    }

    /// Create a numeric literal
    pub fn number(n: impl Into<String>) -> Self {
        IRNode::NumericLiteral(n.into())
    }

    /// Create a call expression
    pub fn call(callee: IRNode, args: Vec<IRNode>) -> Self {
        IRNode::CallExpr {
            callee: Box::new(callee),
            arguments: args,
        }
    }

    /// Create a property access
    pub fn prop(object: IRNode, property: impl Into<String>) -> Self {
        IRNode::PropertyAccess {
            object: Box::new(object),
            property: property.into(),
        }
    }

    /// Create an element access
    pub fn elem(object: IRNode, index: IRNode) -> Self {
        IRNode::ElementAccess {
            object: Box::new(object),
            index: Box::new(index),
        }
    }

    /// Create a binary expression
    pub fn binary(left: IRNode, op: impl Into<String>, right: IRNode) -> Self {
        IRNode::BinaryExpr {
            left: Box::new(left),
            operator: op.into(),
            right: Box::new(right),
        }
    }

    /// Create an assignment expression
    pub fn assign(target: IRNode, value: IRNode) -> Self {
        IRNode::BinaryExpr {
            left: Box::new(target),
            operator: "=".to_string(),
            right: Box::new(value),
        }
    }

    /// Create a var declaration
    pub fn var_decl(name: impl Into<String>, init: Option<IRNode>) -> Self {
        IRNode::VarDecl {
            name: name.into(),
            initializer: init.map(Box::new),
        }
    }

    /// Create a return statement
    pub fn ret(expr: Option<IRNode>) -> Self {
        IRNode::ReturnStatement(expr.map(Box::new))
    }

    /// Create a function expression
    pub fn func_expr(name: Option<String>, params: Vec<IRParam>, body: Vec<IRNode>) -> Self {
        IRNode::FunctionExpr {
            name,
            parameters: params,
            body,
            is_expression_body: false,
        }
    }

    /// Create a function declaration
    pub fn func_decl(name: impl Into<String>, params: Vec<IRParam>, body: Vec<IRNode>) -> Self {
        IRNode::FunctionDecl {
            name: name.into(),
            parameters: params,
            body,
        }
    }

    /// Create `this` reference
    pub fn this() -> Self {
        IRNode::This { captured: false }
    }

    /// Create `_this` reference (captured)
    pub fn this_captured() -> Self {
        IRNode::This { captured: true }
    }

    /// Create `void 0`
    pub fn void_0() -> Self {
        IRNode::Undefined
    }

    /// Wrap in parentheses
    pub fn paren(self) -> Self {
        IRNode::Parenthesized(Box::new(self))
    }

    /// Create a block
    pub fn block(stmts: Vec<IRNode>) -> Self {
        IRNode::Block(stmts)
    }

    /// Create an expression statement
    pub fn expr_stmt(expr: IRNode) -> Self {
        IRNode::ExpressionStatement(Box::new(expr))
    }

    /// Create an object literal
    pub fn object(props: Vec<IRProperty>) -> Self {
        IRNode::ObjectLiteral(props)
    }

    /// Create an empty object literal
    pub fn empty_object() -> Self {
        IRNode::ObjectLiteral(Vec::new())
    }

    /// Create an array literal
    pub fn array(elements: Vec<IRNode>) -> Self {
        IRNode::ArrayLiteral(elements)
    }

    /// Create an empty array literal
    pub fn empty_array() -> Self {
        IRNode::ArrayLiteral(Vec::new())
    }

    /// Create a logical OR expression: `left || right`
    pub fn logical_or(left: IRNode, right: IRNode) -> Self {
        IRNode::LogicalOr {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a logical AND expression: `left && right`
    pub fn logical_and(left: IRNode, right: IRNode) -> Self {
        IRNode::LogicalAnd {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a sequence of statements
    pub fn sequence(nodes: Vec<IRNode>) -> Self {
        IRNode::Sequence(nodes)
    }

    /// Create a new expression: `new Constructor(args)`
    pub fn new_expr(callee: IRNode, args: Vec<IRNode>) -> Self {
        IRNode::NewExpr {
            callee: Box::new(callee),
            arguments: args,
        }
    }
}

impl IRParam {
    pub fn new(name: impl Into<String>) -> Self {
        IRParam {
            name: name.into(),
            rest: false,
            default_value: None,
        }
    }

    pub fn rest(name: impl Into<String>) -> Self {
        IRParam {
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
        IRProperty {
            key: IRPropertyKey::Identifier(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }

    /// Create a property with string literal key: `{ "key": value }`
    pub fn init_string(key: impl Into<String>, value: IRNode) -> Self {
        IRProperty {
            key: IRPropertyKey::StringLiteral(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }

    /// Create a getter property
    pub fn getter(key: impl Into<String>, get: IRNode) -> Self {
        IRProperty {
            key: IRPropertyKey::Identifier(key.into()),
            value: get,
            kind: IRPropertyKind::Get,
        }
    }

    /// Create a setter property
    pub fn setter(key: impl Into<String>, set: IRNode) -> Self {
        IRProperty {
            key: IRPropertyKey::Identifier(key.into()),
            value: set,
            kind: IRPropertyKind::Set,
        }
    }
}
