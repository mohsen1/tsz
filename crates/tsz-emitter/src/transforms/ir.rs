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

use std::borrow::Cow;

use tsz_parser::parser::NodeIndex;

/// Intermediate Representation node for transformed JavaScript
#[derive(Debug, Clone)]
pub enum IRNode {
    // =========================================================================
    // Literals
    // =========================================================================
    /// Numeric literal: `42`, `3.14`
    NumericLiteral(Cow<'static, str>),

    /// String literal: `"hello"`, `'world'`
    StringLiteral(Cow<'static, str>),

    /// Raw string literal: writes `"<content>"` without escape processing.
    /// Used when the content already contains the intended escape sequences
    /// (e.g., `\u2730`) that must be preserved verbatim in the output.
    RawStringLiteral(Cow<'static, str>),

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
    Identifier(Cow<'static, str>),

    /// Runtime helper reference: `__helper` or `tslib_1.__helper`.
    RuntimeHelper(Cow<'static, str>),

    /// This keyword: `this` or `_this` (for captures)
    This { captured: bool },

    /// Super keyword
    Super,

    /// `import.meta`, with module-wrapper-specific printing handled by `IRPrinter`.
    ImportMeta,

    // =========================================================================
    // Expressions
    // =========================================================================
    /// Binary expression: `left op right`
    BinaryExpr {
        left: Box<Self>,
        operator: Cow<'static, str>,
        right: Box<Self>,
    },

    /// Unary prefix expression: `!x`, `-x`, `++x`
    PrefixUnaryExpr {
        operator: Cow<'static, str>,
        operand: Box<Self>,
    },

    /// Unary postfix expression: `x++`, `x--`
    PostfixUnaryExpr {
        operand: Box<Self>,
        operator: Cow<'static, str>,
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
    PropertyAccess {
        object: Box<Self>,
        property: Cow<'static, str>,
    },

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

    /// Multiline comma expression (used for ES5 computed property lowering):
    /// ```text
    /// (_a = {},
    ///     _a[key] = value,
    ///     _a)
    /// ```
    CommaExprMultiline(Vec<Self>),

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
        name: Option<Cow<'static, str>>,
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
        name: Cow<'static, str>,
        initializer: Option<Box<Self>>,
    },

    /// Multiple variable declarations: `var a = 1, b = 2;`
    VarDeclList(Vec<Self>),

    /// Internal async-transform marker: start a new hoisted `var` statement group.
    HoistedVarGroupBreak,

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

    /// For-in / for-of statement: `for (init <kind> expr) <body>`. Used by
    /// the ES5 class transform to recurse the body through the
    /// derived-constructor `_this` substitution (issue #3539). `kind` is
    /// `"in"`, `"of"`, or `"await of"`.
    ForInOfStatement {
        kind: Cow<'static, str>,
        initializer: Box<Self>,
        expression: Box<Self>,
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
    BreakStatement(Option<Cow<'static, str>>),

    /// Continue statement: `continue;` or `continue label;`
    ContinueStatement(Option<Cow<'static, str>>),

    /// Labeled statement: `label: stmt`
    LabeledStatement {
        label: Cow<'static, str>,
        statement: Box<Self>,
    },

    // =========================================================================
    // Declarations
    // =========================================================================
    /// Function declaration: `function name(params) { body }`
    FunctionDecl {
        name: Cow<'static, str>,
        parameters: Vec<IRParam>,
        body: Vec<Self>,
        /// Source range of the body block (for preserving single-line formatting)
        body_source_range: Option<(u32, u32)>,
        /// Optional leading JSDoc/block comment from the original constructor declaration
        leading_comment: Option<String>,
    },

    // =========================================================================
    // ES5 Class Transform Specific
    // =========================================================================
    /// IIFE pattern for ES5 class:
    /// `var ClassName = /** @class */ (function (_super) { ... }(BaseClass));`
    ES5ClassIIFE {
        name: Cow<'static, str>,
        base_class: Option<Box<Self>>,
        super_param: Option<Cow<'static, str>>,
        body: Vec<Self>,
        /// `WeakMap` declarations for private fields (before the IIFE)
        weakmap_decls: Vec<String>,
        /// Computed property-name temp declarations for class fields (before the IIFE).
        computed_prop_temp_decls: Vec<String>,
        /// Computed property-name temp assignments for class fields (after the IIFE).
        computed_prop_temp_inits: Vec<Self>,
        /// `WeakMap` instantiations (after the IIFE)
        weakmap_inits: Vec<String>,
        /// Optional comment emitted between weakmap declarations and class var declaration.
        leading_comment: Option<String>,
        /// Static block IIFEs deferred to after the class IIFE
        /// (used when the class has no non-block static members)
        deferred_static_blocks: Vec<Self>,
        /// Class alias name to emit outside the IIFE for use by the deferred
        /// static block IIFEs. When set, the printer emits
        /// `var <alias>;` before the class declaration and
        /// `<alias> = <name>;` after the class IIFE and before the deferred
        /// blocks, so blocks that reference `this` (rewritten to the alias)
        /// can resolve it. Issue #3967.
        deferred_block_class_alias: Option<String>,
    },

    /// Assignment form for an ES5 class expression:
    /// `ClassName = /** @class */ (function (_super) { ... }(BaseClass));`
    ///
    /// This is used when a class declaration appears in a scope that already
    /// owns hoist scheduling, such as an async/generator body. The caller
    /// schedules the declaration vars separately, then emits this structured
    /// assignment where the class executes.
    ES5ClassAssignment {
        name: Cow<'static, str>,
        base_class: Option<Box<Self>>,
        super_param: Option<Cow<'static, str>>,
        body: Vec<Self>,
        /// Computed property-name temp assignments for class fields (after the assignment).
        computed_prop_temp_inits: Vec<Self>,
        /// `WeakMap` instantiations (after the assignment)
        weakmap_inits: Vec<String>,
        /// Optional comment emitted before the class assignment.
        leading_comment: Option<String>,
        /// Static block IIFEs deferred to after the class assignment.
        deferred_static_blocks: Vec<Self>,
        /// Class alias name assigned after the class value exists and before
        /// deferred static blocks that reference the alias.
        deferred_block_class_alias: Option<String>,
    },

    /// Static block IIFE: `(function () { ...statements... })();`
    StaticBlockIIFE { statements: Vec<Self> },

    /// __extends helper call: `__extends(ClassName, _super);`
    ExtendsHelper {
        class_name: Cow<'static, str>,
        super_name: Cow<'static, str>,
    },

    /// ES5 class expression application:
    /// `/** @class */ (_a.apply(void 0, [(Base)]))`
    ES5ClassApply {
        factory: Box<Self>,
        base_class: Box<Self>,
    },

    /// Prototype method assignment: `ClassName.prototype.method = function() {...};`
    PrototypeMethod {
        class_name: Cow<'static, str>,
        method_name: IRMethodName,
        function: Box<Self>,
        /// Leading `JSDoc` or block comment from the original method declaration
        leading_comment: Option<String>,
        /// Trailing comment from the original method declaration line
        trailing_comment: Option<String>,
    },

    /// Static method assignment: `ClassName.method = function() {...};`
    StaticMethod {
        class_name: Cow<'static, str>,
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
        /// Leading comment from the original accessor declaration
        leading_comment: Option<String>,
    },

    // =========================================================================
    // Async Transform Specific
    // =========================================================================
    /// __awaiter helper call
    AwaiterCall {
        this_arg: Box<Self>,
        generator_body: Box<Self>,
        /// Var declaration groups hoisted out of the generator body to the awaiter wrapper scope.
        hoisted_var_groups: Vec<Vec<String>>,
        /// Custom promise constructor for the third `__awaiter` arg.
        promise_constructor: Option<String>,
        /// Force the awaiter callback body onto multiple lines even when no
        /// generator-local vars were hoisted. `tsc` does this when the async
        /// function captures `arguments` in the wrapper scope.
        multiline_callback: bool,
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
        comment: Option<Cow<'static, str>>,
    },

    /// _`a.sent()` - get the sent value in generator
    GeneratorSent,

    /// _a.label - the label property
    GeneratorLabel,

    /// `_a.trys.push([start, catch, finally, end])`
    GeneratorTryPush {
        start_label: u32,
        catch_label: u32,
        finally_label: u32,
        end_label: u32,
    },

    /// `_a.trys.push([start, , finally, end])`
    GeneratorTryPushFinally {
        start_label: u32,
        finally_label: u32,
        end_label: u32,
    },

    /// `if (condition) return [3 /*break*/, target_label];`
    /// Used in async state machines for conditional branching.
    IfBreak {
        condition: Box<Self>,
        target_label: u32,
    },

    // =========================================================================
    // Private Field Helpers
    // =========================================================================
    /// __classPrivateFieldGet(receiver, weakmap, "f")
    PrivateFieldGet {
        receiver: Box<Self>,
        weakmap_name: Cow<'static, str>,
    },

    /// __classPrivateFieldGet(receiver, state, "f", storage)
    PrivateStaticFieldGet {
        receiver: Box<Self>,
        state: Box<Self>,
        storage_name: Cow<'static, str>,
    },

    /// __classPrivateFieldSet(receiver, weakmap, value, "f")
    PrivateFieldSet {
        receiver: Box<Self>,
        weakmap_name: Cow<'static, str>,
        value: Box<Self>,
    },

    /// __classPrivateFieldSet(receiver, state, value, "f", storage)
    PrivateStaticFieldSet {
        receiver: Box<Self>,
        state: Box<Self>,
        storage_name: Cow<'static, str>,
        value: Box<Self>,
    },

    /// __classPrivateFieldIn(weakmap, obj)
    PrivateFieldIn {
        weakmap_name: Cow<'static, str>,
        obj: Box<Self>,
    },

    /// WeakMap.set for private field init: `_weakmap.set(this, value);`
    WeakMapSet {
        weakmap_name: Cow<'static, str>,
        key: Box<Self>,
        value: Box<Self>,
    },

    // =========================================================================
    // Special
    // =========================================================================
    /// Raw JavaScript string (escape hatch for complex cases)
    Raw(Cow<'static, str>),

    /// Comment: `/* text */` or `// text`
    Comment {
        text: Cow<'static, str>,
        is_block: bool,
    },

    /// Trailing comment that should be emitted on the same line as the previous node.
    /// Used for comments like `M.x = ""; //comment` inside namespace bodies.
    /// The text includes delimiters (e.g., `//comment` or `/* comment */`).
    TrailingComment(Cow<'static, str>),

    /// Sequence of statements/nodes
    Sequence(Vec<Self>),

    /// Reference to an original AST node (for passthrough)
    ASTRef(NodeIndex),

    /// Reference to an original async arrow expression whose generated
    /// `__generator` call should use a static class alias as lexical `this`.
    ASTRefWithGeneratorThis {
        node: NodeIndex,
        generator_this: Cow<'static, str>,
    },

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
    ExportInit { name: Cow<'static, str> },

    /// `var module = require("module");` (require statement)
    RequireStatement {
        var_name: Cow<'static, str>,
        module_spec: Cow<'static, str>,
    },

    /// `import foo from "module";` -> `var foo = module.foo;` (default import)
    DefaultImport {
        var_name: Cow<'static, str>,
        module_var: Cow<'static, str>,
    },

    /// `import * as ns from "module";` -> `var ns = require("module");` (namespace import)
    NamespaceImport {
        var_name: Cow<'static, str>,
        module_var: Cow<'static, str>,
    },

    /// `import { foo } from "module";` -> `var foo = module.foo;` (named import)
    NamedImport {
        var_name: Cow<'static, str>,
        module_var: Cow<'static, str>,
        import_name: Cow<'static, str>,
    },

    /// `export default value;` -> `exports.default = value;`
    ExportAssignment { name: Cow<'static, str> },

    /// `export { foo as bar } from "module";` (re-export)
    ReExportProperty {
        export_name: Cow<'static, str>,
        module_var: Cow<'static, str>,
        import_name: Cow<'static, str>,
    },

    // =========================================================================
    // Enum / Namespace IR Nodes
    // =========================================================================
    /// Enum IIFE: `(function (E) { ... })(E || (E = {}))`
    /// When `namespace_export` is set, emits: `(E = NS.E || (NS.E = {}))`
    EnumIIFE {
        name: Cow<'static, str>,
        members: Vec<EnumMember>,
        namespace_export: Option<Cow<'static, str>>,
    },

    /// Namespace IIFE: `(function (NS) { ... })(NS || (NS = {}))`
    NamespaceIIFE {
        name: Cow<'static, str>,
        name_parts: Vec<Cow<'static, str>>,
        body: Vec<Self>,
        is_exported: bool,
        attach_to_exports: bool,
        /// `SystemJS` export names folded into the namespace IIFE tail:
        /// `N || (exports_1("alias", exports_1("name", N = {})))`.
        system_export_names: Vec<Cow<'static, str>>,
        /// Whether to emit the `var name;` declaration for this namespace.
        /// Set to false when merging with a class/function/enum that already declared it.
        should_declare_var: bool,
        /// When true, namespace merges with default-exported fn in CJS.
        default_export_merge: bool,
        /// Parent namespace name for qualified binding: `NS = Parent.NS || (Parent.NS = {})`
        parent_name: Option<Cow<'static, str>>,
        /// Renamed IIFE parameter name when a member collides with the namespace name.
        /// E.g., namespace A { export class A {} } => `(function (A_1) { ... A_1.A = A; })`
        /// Only the function parameter and namespace exports use this name;
        /// the var declaration and argument still use the original name.
        param_name: Option<Cow<'static, str>>,
        /// Skip automatic indentation when this node is in a Sequence (after the first child).
        /// Used for nested namespace IIFEs that should align with their siblings rather than
        /// being indented as regular statements. This prevents double-indentation when a
        /// namespace IIFE follows a class/enum/function in a parent namespace body.
        skip_sequence_indent: bool,
        /// Same-line comment after the namespace declaration closing brace.
        trailing_comment: Option<Cow<'static, str>>,
    },

    /// Namespace export: `NS.foo = ...;`
    NamespaceExport {
        namespace: Cow<'static, str>,
        name: Cow<'static, str>,
        value: Box<Self>,
    },
}

/// Enum member representation for IR
#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: Cow<'static, str>,
    pub value: EnumMemberValue,
    /// Optional leading JSDoc/block comment from the original enum member
    pub leading_comment: Option<String>,
    /// Optional trailing comment on the same line as the member value
    pub trailing_comment: Option<String>,
}

/// Enum member value representation
#[derive(Debug, Clone)]
pub enum EnumMemberValue {
    /// Auto-incremented numeric value
    Auto(i64),
    /// Explicit numeric value
    Numeric(i64),
    /// String value
    String(Cow<'static, str>),
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
    Identifier(Cow<'static, str>),
    StringLiteral(Cow<'static, str>),
    NumericLiteral(Cow<'static, str>),
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
    Identifier(Cow<'static, str>),
    StringLiteral(Cow<'static, str>),
    NumericLiteral(Cow<'static, str>),
    Computed(Box<IRNode>),
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct IRParam {
    pub name: Cow<'static, str>,
    pub rest: bool,
    pub default_value: Option<Box<IRNode>>,
    pub leading_comment: Option<Cow<'static, str>>,
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
    pub param: Option<Cow<'static, str>>,
    pub body: Vec<IRNode>,
}

/// Property descriptor for Object.defineProperty
#[derive(Debug, Clone)]
pub struct IRPropertyDescriptor {
    pub get: Option<Box<IRNode>>,
    pub set: Option<Box<IRNode>>,
    pub value: Option<Box<IRNode>>,
    pub get_leading_comment: Option<String>,
    pub set_leading_comment: Option<String>,
    pub enumerable: bool,
    pub configurable: bool,
    pub writable: bool,
    /// Optional trailing comment to emit after the getter function in
    /// `Object.defineProperty(..., { get: ..., ... })` shapes.
    pub trailing_comment: Option<String>,
}

/// Generator case (for async state machine)
#[derive(Debug, Clone)]
pub struct IRGeneratorCase {
    pub label: u32,
    pub statements: Vec<IRNode>,
}

impl IRProperty {
    fn contains_identifier(&self, name: &str) -> bool {
        self.key.contains_identifier(name) || self.value.contains_identifier(name)
    }
}

impl IRPropertyKey {
    fn contains_identifier(&self, name: &str) -> bool {
        match self {
            Self::Identifier(ident) => ident.as_ref() == name,
            Self::Computed(expr) => expr.contains_identifier(name),
            Self::StringLiteral(_) | Self::NumericLiteral(_) => false,
        }
    }
}

impl IRMethodName {
    fn contains_identifier(&self, name: &str) -> bool {
        match self {
            Self::Identifier(ident) => ident.as_ref() == name,
            Self::Computed(expr) => expr.contains_identifier(name),
            Self::StringLiteral(_) | Self::NumericLiteral(_) => false,
        }
    }
}

impl IRParam {
    fn contains_identifier(&self, name: &str) -> bool {
        self.name.as_ref() == name
            || self
                .default_value
                .as_ref()
                .is_some_and(|value| value.contains_identifier(name))
    }
}

impl IRSwitchCase {
    fn contains_identifier(&self, name: &str) -> bool {
        self.test
            .as_ref()
            .is_some_and(|test| test.contains_identifier(name))
            || self
                .statements
                .iter()
                .any(|statement| statement.contains_identifier(name))
    }
}

impl IRCatchClause {
    fn contains_identifier(&self, name: &str) -> bool {
        self.param
            .as_ref()
            .is_some_and(|param| param.as_ref() == name)
            || self
                .body
                .iter()
                .any(|statement| statement.contains_identifier(name))
    }
}

impl IRPropertyDescriptor {
    fn contains_identifier(&self, name: &str) -> bool {
        self.get
            .as_ref()
            .is_some_and(|get| get.contains_identifier(name))
            || self
                .set
                .as_ref()
                .is_some_and(|set| set.contains_identifier(name))
            || self
                .value
                .as_ref()
                .is_some_and(|value| value.contains_identifier(name))
    }
}

impl IRGeneratorCase {
    fn contains_identifier(&self, name: &str) -> bool {
        self.statements
            .iter()
            .any(|statement| statement.contains_identifier(name))
    }
}

impl EnumMember {
    fn contains_identifier(&self, name: &str) -> bool {
        match &self.value {
            EnumMemberValue::Computed(expr) => expr.contains_identifier(name),
            EnumMemberValue::Auto(_) | EnumMemberValue::Numeric(_) | EnumMemberValue::String(_) => {
                false
            }
        }
    }
}

// =========================================================================
// Builder helpers for IR construction
// =========================================================================

impl IRNode {
    /// Return whether this `IR` subtree references `name` as an identifier.
    pub fn contains_identifier(&self, name: &str) -> bool {
        match self {
            Self::Identifier(ident) => ident.as_ref() == name,
            Self::BinaryExpr { left, right, .. }
            | Self::LogicalOr { left, right }
            | Self::LogicalAnd { left, right } => {
                left.contains_identifier(name) || right.contains_identifier(name)
            }
            Self::PrefixUnaryExpr { operand, .. }
            | Self::PostfixUnaryExpr { operand, .. }
            | Self::Parenthesized(operand)
            | Self::SpreadElement(operand)
            | Self::ExpressionStatement(operand)
            | Self::ThrowStatement(operand)
            | Self::PrivateFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateStaticFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateFieldIn { obj: operand, .. } => operand.contains_identifier(name),
            Self::CallExpr { callee, arguments }
            | Self::NewExpr {
                callee, arguments, ..
            } => {
                callee.contains_identifier(name)
                    || arguments.iter().any(|arg| arg.contains_identifier(name))
            }
            Self::PropertyAccess { object, .. } => object.contains_identifier(name),
            Self::ElementAccess { object, index } => {
                object.contains_identifier(name) || index.contains_identifier(name)
            }
            Self::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                condition.contains_identifier(name)
                    || when_true.contains_identifier(name)
                    || when_false.contains_identifier(name)
            }
            Self::CommaExpr(nodes)
            | Self::CommaExprMultiline(nodes)
            | Self::ArrayLiteral(nodes)
            | Self::VarDeclList(nodes)
            | Self::Block(nodes)
            | Self::Sequence(nodes)
            | Self::StaticBlockIIFE { statements: nodes } => {
                nodes.iter().any(|node| node.contains_identifier(name))
            }
            Self::ObjectLiteral { properties, .. } => properties
                .iter()
                .any(|property| property.contains_identifier(name)),
            Self::FunctionExpr {
                parameters, body, ..
            }
            | Self::FunctionDecl {
                parameters, body, ..
            } => {
                parameters
                    .iter()
                    .any(|param| param.contains_identifier(name))
                    || body.iter().any(|node| node.contains_identifier(name))
            }
            Self::VarDecl { initializer, .. } => initializer
                .as_ref()
                .is_some_and(|init| init.contains_identifier(name)),
            Self::ReturnStatement(expr) => expr
                .as_ref()
                .is_some_and(|expr| expr.contains_identifier(name)),
            Self::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.contains_identifier(name)
                    || then_branch.contains_identifier(name)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| branch.contains_identifier(name))
            }
            Self::SwitchStatement { expression, cases } => {
                expression.contains_identifier(name)
                    || cases.iter().any(|case| case.contains_identifier(name))
            }
            Self::ForStatement {
                initializer,
                condition,
                incrementor,
                body,
            } => {
                initializer
                    .as_ref()
                    .is_some_and(|init| init.contains_identifier(name))
                    || condition
                        .as_ref()
                        .is_some_and(|condition| condition.contains_identifier(name))
                    || incrementor
                        .as_ref()
                        .is_some_and(|incrementor| incrementor.contains_identifier(name))
                    || body.contains_identifier(name)
            }
            Self::ForInOfStatement {
                initializer,
                expression,
                body,
                ..
            } => {
                initializer.contains_identifier(name)
                    || expression.contains_identifier(name)
                    || body.contains_identifier(name)
            }
            Self::WhileStatement { condition, body }
            | Self::DoWhileStatement { body, condition } => {
                condition.contains_identifier(name) || body.contains_identifier(name)
            }
            Self::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                try_block.contains_identifier(name)
                    || catch_clause
                        .as_ref()
                        .is_some_and(|catch| catch.contains_identifier(name))
                    || finally_block
                        .as_ref()
                        .is_some_and(|finally_block| finally_block.contains_identifier(name))
            }
            Self::LabeledStatement { statement, .. } => statement.contains_identifier(name),
            Self::ES5ClassIIFE {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            }
            | Self::ES5ClassAssignment {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            } => {
                base_class
                    .as_ref()
                    .is_some_and(|base| base.contains_identifier(name))
                    || body.iter().any(|node| node.contains_identifier(name))
                    || computed_prop_temp_inits
                        .iter()
                        .any(|node| node.contains_identifier(name))
                    || deferred_static_blocks
                        .iter()
                        .any(|node| node.contains_identifier(name))
            }
            Self::ExtendsHelper {
                class_name,
                super_name,
            } => class_name.as_ref() == name || super_name.as_ref() == name,
            Self::ES5ClassApply {
                factory,
                base_class,
            } => factory.contains_identifier(name) || base_class.contains_identifier(name),
            Self::PrototypeMethod {
                class_name,
                method_name,
                function,
                ..
            }
            | Self::StaticMethod {
                class_name,
                method_name,
                function,
                ..
            } => {
                class_name.as_ref() == name
                    || method_name.contains_identifier(name)
                    || function.contains_identifier(name)
            }
            Self::DefineProperty {
                target,
                property_name,
                descriptor,
                ..
            } => {
                target.contains_identifier(name)
                    || property_name.contains_identifier(name)
                    || descriptor.contains_identifier(name)
            }
            Self::AwaiterCall {
                this_arg,
                generator_body,
                ..
            } => this_arg.contains_identifier(name) || generator_body.contains_identifier(name),
            Self::GeneratorBody { cases, .. } => {
                cases.iter().any(|case| case.contains_identifier(name))
            }
            Self::GeneratorOp { value, .. } => value
                .as_ref()
                .is_some_and(|value| value.contains_identifier(name)),
            Self::IfBreak { condition, .. } => condition.contains_identifier(name),
            Self::PrivateFieldSet {
                receiver, value, ..
            } => receiver.contains_identifier(name) || value.contains_identifier(name),
            Self::PrivateStaticFieldSet {
                receiver,
                state,
                value,
                ..
            } => {
                receiver.contains_identifier(name)
                    || state.contains_identifier(name)
                    || value.contains_identifier(name)
            }
            Self::WeakMapSet { key, value, .. } => {
                key.contains_identifier(name) || value.contains_identifier(name)
            }
            Self::NamedImport { var_name, .. }
            | Self::NamespaceImport { var_name, .. }
            | Self::DefaultImport { var_name, .. }
            | Self::RequireStatement { var_name, .. }
            | Self::ExportInit { name: var_name }
            | Self::ExportAssignment { name: var_name } => var_name.as_ref() == name,
            Self::ReExportProperty {
                export_name,
                module_var,
                import_name,
            } => {
                export_name.as_ref() == name
                    || module_var.as_ref() == name
                    || import_name.as_ref() == name
            }
            Self::EnumIIFE {
                name: enum_name,
                members,
                namespace_export,
            } => {
                enum_name.as_ref() == name
                    || namespace_export
                        .as_ref()
                        .is_some_and(|ns| ns.as_ref() == name)
                    || members
                        .iter()
                        .any(|member| member.contains_identifier(name))
            }
            Self::NamespaceIIFE {
                name: namespace_name,
                body,
                parent_name,
                param_name,
                ..
            } => {
                namespace_name.as_ref() == name
                    || parent_name
                        .as_ref()
                        .is_some_and(|parent| parent.as_ref() == name)
                    || param_name
                        .as_ref()
                        .is_some_and(|param| param.as_ref() == name)
                    || body.iter().any(|node| node.contains_identifier(name))
            }
            Self::NamespaceExport {
                namespace,
                name: export_name,
                value,
            } => {
                namespace.as_ref() == name
                    || export_name.as_ref() == name
                    || value.contains_identifier(name)
            }
            Self::NumericLiteral(_)
            | Self::StringLiteral(_)
            | Self::RawStringLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Undefined
            | Self::RuntimeHelper(_)
            | Self::This { .. }
            | Self::Super
            | Self::ImportMeta
            | Self::EmptyStatement
            | Self::HoistedVarGroupBreak
            | Self::BreakStatement(_)
            | Self::ContinueStatement(_)
            | Self::GeneratorSent
            | Self::GeneratorLabel
            | Self::GeneratorTryPush { .. }
            | Self::GeneratorTryPushFinally { .. }
            | Self::Raw(_)
            | Self::Comment { .. }
            | Self::TrailingComment(_)
            | Self::ASTRef(_)
            | Self::ASTRefWithGeneratorThis { .. }
            | Self::ASTRefRange(..)
            | Self::UseStrict
            | Self::EsesModuleMarker => false,
        }
    }

    /// Create an identifier node
    pub fn id(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Identifier(name.into())
    }

    /// Create a string literal
    pub fn string(s: impl Into<Cow<'static, str>>) -> Self {
        Self::StringLiteral(s.into())
    }

    /// Create a numeric literal
    pub fn number(n: impl Into<Cow<'static, str>>) -> Self {
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
    pub fn prop(object: Self, property: impl Into<Cow<'static, str>>) -> Self {
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
    pub fn binary(left: Self, op: impl Into<Cow<'static, str>>, right: Self) -> Self {
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
            operator: Cow::Borrowed("="),
            right: Box::new(value),
        }
    }

    /// Create a var declaration
    pub fn var_decl(name: impl Into<Cow<'static, str>>, init: Option<Self>) -> Self {
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
    pub const fn func_expr(
        name: Option<Cow<'static, str>>,
        params: Vec<IRParam>,
        body: Vec<Self>,
    ) -> Self {
        Self::FunctionExpr {
            name,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Create a function declaration
    pub fn func_decl(
        name: impl Into<Cow<'static, str>>,
        params: Vec<IRParam>,
        body: Vec<Self>,
    ) -> Self {
        Self::FunctionDecl {
            name: name.into(),
            parameters: params,
            body,
            body_source_range: None,
            leading_comment: None,
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
}

impl IRParam {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            rest: false,
            default_value: None,
            leading_comment: None,
        }
    }

    pub fn rest(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            rest: true,
            default_value: None,
            leading_comment: None,
        }
    }

    pub fn with_default(mut self, default: IRNode) -> Self {
        self.default_value = Some(Box::new(default));
        self
    }
}

impl IRProperty {
    /// Create a simple property with identifier key: `{ key: value }`
    pub fn init(key: impl Into<Cow<'static, str>>, value: IRNode) -> Self {
        Self {
            key: IRPropertyKey::Identifier(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }
}
