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

    /// Multiline comma expression whose continuation lines reuse the current
    /// indentation level. Used when a comma expression is nested inside another
    /// multiline comma expression.
    CommaExprMultilineFlat(Vec<Self>),

    /// Array literal: `[a, b, c]`
    ArrayLiteral(Vec<Self>),

    /// Spread element: `...expr`
    SpreadElement(Box<Self>),

    /// Object literal: `{ key: value, ... }`
    ObjectLiteral {
        properties: Vec<IRProperty>,
        /// Source range (pos, end) for single-line vs multiline detection
        source_range: Option<(u32, u32)>,
        /// Extra continuation indentation for object literals that TypeScript
        /// emits as part of downlevel computed-property temporaries.
        extra_indent: u8,
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

    /// Expression with a leading preserved source comment, such as an erased
    /// JSDoc type assertion: `/** @type {*} */ expr`.
    LeadingCommentExpr {
        comment: Cow<'static, str>,
        expression: Box<Self>,
    },

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

    /// `var _newTarget = ...;` capture emitted before parameter/body prologues.
    NewTargetCapture { initializer: Box<Self> },

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
        multiline_body: bool,
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
        /// Optional outer binding name when block-scoped class lowering must
        /// avoid colliding with an outer declaration while preserving the
        /// class's own lexical name inside the IIFE.
        binding_name: Option<Cow<'static, str>>,
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
        /// Statements emitted after the private helper initialization line.
        post_weakmap_statements: Vec<String>,
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
        /// When set, deferred static blocks are folded into the assignment as
        /// `C = (_t = classExpr, staticBlock(), _t)` so the assignment remains
        /// one expression with the class value as its result.
        deferred_static_result_temp: Option<Cow<'static, str>>,
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
        /// Whether the awaiter callback body must declare `var _this = this;`
        /// for generated `IRNode::This { captured: true }` references.
        needs_lexical_this_capture: bool,
        /// Var declaration groups hoisted out of the generator body to the awaiter wrapper scope.
        hoisted_var_groups: Vec<Vec<String>>,
        /// Custom promise constructor for the third `__awaiter` arg.
        promise_constructor: Option<String>,
        /// Force the awaiter callback body onto multiple lines even when no
        /// generator-local vars were hoisted. `tsc` does this when the async
        /// function captures `arguments` in the wrapper scope.
        multiline_callback: bool,
        /// Directive prologues (e.g. `"use strict"`) extracted from the start of
        /// the generator body. `tsc` places these inside the `__awaiter` callback
        /// before the `var` declarations and before `__generator`.
        directives: Vec<String>,
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

    /// `_a.trys.push([start, catch, , end])`
    GeneratorTryPushCatch {
        start_label: u32,
        catch_label: u32,
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

    /// `with (expression) { ... }`
    WithStatement {
        expression: Box<Self>,
        body: Box<Self>,
    },

    /// Reference to an original AST node (for passthrough)
    ASTRef(NodeIndex),

    /// Reference to an original async arrow expression whose generated
    /// `__generator` call should use a static class alias as lexical `this`.
    ASTRefWithGeneratorThis {
        node: NodeIndex,
        generator_this: Cow<'static, str>,
    },

    /// Reference to an original ES5 class expression whose heritage expression
    /// should be evaluated in a captured constructor receiver context.
    ASTRefWithCapturedClassHeritageThis(NodeIndex),

    /// Reference to an original class expression whose computed property names
    /// should inherit an enclosing instance `super` home while the nested
    /// printer applies normal ES5 class-expression lowering.
    ASTRefWithInheritedComputedNameSuper {
        node: NodeIndex,
        super_name: Cow<'static, str>,
    },

    /// Reference to an original class expression whose computed property names
    /// should inherit an enclosing static-initializer `this` binding while the
    /// nested printer applies normal ES5 class-expression lowering.
    ASTRefWithInheritedComputedNameThis {
        node: NodeIndex,
        this_alias: Cow<'static, str>,
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
        invalid_namespace_static: bool,
    },

    /// Namespace IIFE: `(function (NS) { ... })(NS || (NS = {}))`
    NamespaceIIFE {
        name: Cow<'static, str>,
        name_parts: Vec<Cow<'static, str>>,
        body: Vec<Self>,
        is_exported: bool,
        attach_to_exports: bool,
        /// CommonJS export properties folded into an exported namespace IIFE.
        /// Names are in source order, so the last name becomes the outermost
        /// assignment: `[N, Alias]` emits `exports.Alias = exports.N = N = {}`.
        commonjs_export_names: Vec<Cow<'static, str>>,
        /// `SystemJS` export names folded into the namespace IIFE tail:
        /// `N || (exports_1("alias", exports_1("name", N = {})))`.
        system_export_names: Vec<Cow<'static, str>>,
        /// Whether to emit the `var name;` declaration for this namespace.
        /// Set to false when merging with a class/function/enum that already declared it.
        should_declare_var: bool,
        /// When true, the hoisted namespace binding is emitted as
        /// `var name = void 0;` instead of `var name;`. Set for instantiated
        /// namespaces downleveled to `var` while nested in a control-flow /
        /// standalone block (not a function body, namespace body, or top level),
        /// matching `tsc`'s reset of block-scoped hoisted bindings.
        hoist_var_void_zero: bool,
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
        /// Preserve an invalid `static` modifier before the generated namespace
        /// binding declaration when recovering namespace-body emit.
        invalid_namespace_static: bool,
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
    /// Render the (single) clause statement on the same line as the `case`
    /// label, e.g. `case x: return [3 /*break*/, 2];`. tsc emits synthesized
    /// single-statement clauses inline; user-authored clauses stay multi-line.
    pub inline: bool,
}

/// Catch clause
#[derive(Debug, Clone)]
pub struct IRCatchClause {
    pub param: Option<Cow<'static, str>>,
    pub body: Vec<IRNode>,
    /// Emit the catch body on a single line (`catch (e) { stmt; }`) instead of
    /// the default multi-line block. Matches `tsc`'s downlevel-iteration
    /// `for-of` error-handling shape.
    pub single_line: bool,
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

    fn contains_captured_this_reference(&self) -> bool {
        self.key.contains_captured_this_reference() || self.value.contains_captured_this_reference()
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

    fn contains_captured_this_reference(&self) -> bool {
        match self {
            Self::Computed(expr) => expr.contains_captured_this_reference(),
            Self::Identifier(_) | Self::StringLiteral(_) | Self::NumericLiteral(_) => false,
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

    fn contains_captured_this_reference(&self) -> bool {
        self.default_value
            .as_ref()
            .is_some_and(|value| value.contains_captured_this_reference())
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

    fn contains_captured_this_reference(&self) -> bool {
        self.statements
            .iter()
            .any(IRNode::contains_captured_this_reference)
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

fn function_body_declares_var(body: &[IRNode], name: &str) -> bool {
    body.iter().any(|node| match node {
        IRNode::VarDecl { name: var_name, .. } => var_name.as_ref() == name,
        IRNode::VarDeclList(decls) => decls.iter().any(|decl| match decl {
            IRNode::VarDecl { name: var_name, .. } => var_name.as_ref() == name,
            _ => false,
        }),
        _ => false,
    })
}
