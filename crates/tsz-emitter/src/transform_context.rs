//! Transform Context - Projection Layer for AST Transforms
//!
//! This module implements the "Projection Layer" approach to separate Transform logic
//! from Print logic. Since our Node AST is read-only (Data-Oriented Design), we
//! cannot mutate nodes. Instead, we create lightweight "transform directives" that tell
//! the printer how to emit a node differently than its literal AST representation.
//!
//! # Architecture
//!
//! ## Phase 1: Lowering Pass
//! The `LoweringPass` walks the AST and produces `TransformDirective`s for nodes that
//! need transformation (ES5 classes, module exports, etc.). These directives are stored
//! in a `TransformContext`.
//!
//! ## Phase 2: Print Pass
//! The `Printer` checks the `TransformContext` before emitting each node. If a
//! directive exists, it uses that to guide emission; otherwise it emits the node directly.
//!
//! # Benefits
//!
//! - ✅ AST remains read-only (DOD compliance)
//! - ✅ Transforms are testable independently from printing
//! - ✅ No intermediate allocations (just a HashMap of directives)
//! - ✅ Composable transforms via `Chain` directive
//! - ✅ Clear separation of concerns

use crate::transforms::helpers::HelpersNeeded;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;

pub type IdentifierId = u32;

/// Transform directives tell the printer how to emit a node differently
/// than its literal AST representation.
#[derive(Debug, Clone)]
pub enum TransformDirective {
    /// Emit node as-is (identity transform)
    Identity,

    /// ES5 Class: Transform class to IIFE pattern
    ///
    /// ```typescript
    /// class Foo { constructor(x) { this.x = x; } }
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var Foo = /** @class */ (function () {
    ///     function Foo(x) { this.x = x; }
    ///     return Foo;
    /// }());
    /// ```
    ES5Class {
        /// Original class node index
        class_node: NodeIndex,
        /// Heritage clause (extends)
        heritage: Option<NodeIndex>,
    },

    /// ES5 Class Expression: Transform class expression to IIFE expression.
    /// Uses a synthetic name for anonymous classes to preserve semantics.
    ES5ClassExpression {
        /// Original class expression node index
        class_node: NodeIndex,
    },

    /// ES5 Namespace: Transform namespace to IIFE pattern
    ES5Namespace {
        /// Original namespace node index
        namespace_node: NodeIndex,
        /// Whether to emit a 'var' declaration for the namespace (false when merging with class/enum/function)
        should_declare_var: bool,
    },

    /// ES5 Enum: Transform enum to IIFE pattern
    ES5Enum {
        /// Original enum node index
        enum_node: NodeIndex,
    },

    /// CommonJS Export: Wrap declaration with exports assignment
    ///
    /// ```typescript
    /// export class Foo {}
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// class Foo {}
    /// exports.Foo = Foo;
    /// ```
    CommonJSExport {
        /// Identifier ids to export
        names: Arc<[IdentifierId]>,
        /// Whether this is a default export
        is_default: bool,
        /// The inner directive to apply first
        inner: Box<TransformDirective>,
    },

    /// CommonJS default export for anonymous class/function declarations.
    ///
    /// ```typescript
    /// export default function () {}
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// exports.default = function () {};
    /// ```
    CommonJSExportDefaultExpr,

    /// CommonJS default export for anonymous class declarations in ES5.
    ///
    /// ```typescript
    /// export default class { method() {} }
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var _a = /** @class */ (function () { ... }());
    /// exports.default = _a;
    /// ```
    CommonJSExportDefaultClassES5 {
        /// Original class node index
        class_node: NodeIndex,
    },

    /// ES5 Arrow Function: Transform arrow to regular function
    ///
    /// ```typescript
    /// const f = (x) => x + 1;
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var f = function (x) { return x + 1; };
    /// ```
    ES5ArrowFunction {
        /// Original arrow function node
        arrow_node: NodeIndex,
        /// Whether this captures 'this' (needs _this = this)
        captures_this: bool,
        /// Whether this captures 'arguments' (needs _arguments = arguments)
        captures_arguments: bool,
        /// Class alias for static members (e.g., "_a" for static class foo)
        /// When set, 'this' in the arrow function refers to this class alias
        class_alias: Option<Arc<str>>,
    },

    /// ES5 Async/Await: Transform to __awaiter helper
    ES5AsyncFunction {
        /// Original async function node
        function_node: NodeIndex,
    },

    /// ES5 For-Of: Transform to iterator loop with __values helper
    ES5ForOf {
        /// Original for-of statement node
        for_of_node: NodeIndex,
    },

    /// ES5 Object Literal: Transform computed properties and spread to assignments
    ///
    /// ```typescript
    /// const obj = { a: 1, [k]: 2, ...rest };
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var obj = (_a = { a: 1 }, _a[k] = 2, Object.assign(_a, rest), _a);
    /// ```
    ES5ObjectLiteral {
        /// Original object literal node
        object_literal: NodeIndex,
    },

    /// ES5 Array Literal: Transform spread elements to ES5 compatible code.
    ///
    /// ```typescript
    /// const arr = [1, ...rest, 2];
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var arr = [1].concat(rest, [2]);
    /// ```
    ES5ArrayLiteral {
        /// Original array literal node
        array_literal: NodeIndex,
    },

    /// ES5 Call Expression with Spread: Transform spread arguments to .apply() with __spreadArray.
    ///
    /// ```typescript
    /// foo(...arr, 1, 2);
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [1, 2], false));
    /// ```
    ES5CallSpread {
        /// Original call expression node
        call_expr: NodeIndex,
    },

    /// ES5 Variable Declaration List: Transform destructuring declarations to assignments.
    ES5VariableDeclarationList {
        /// Original variable declaration list node
        decl_list: NodeIndex,
    },

    /// ES5 Function Parameters: Transform default/rest/destructuring params.
    ES5FunctionParameters {
        /// Original function declaration/expression node
        function_node: NodeIndex,
    },

    /// ES5 Template Literal: Transform template literals/tagged templates to ES5 output.
    ES5TemplateLiteral {
        /// Original template node (template expression, tagged template, or no-sub literal)
        template_node: NodeIndex,
    },

    /// Substitute this with _this (lexical capture for arrow functions)
    ///
    /// When an arrow function captures `this`, all `this` references inside it
    /// should be substituted with `_this` (the capture variable).
    ///
    /// ```typescript
    /// class Foo {
    ///   method() {
    ///     // this -> _this
    ///     () => this.x
    ///   }
    /// }
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var _this = this;
    /// function () {
    ///   // All this references become _this
    ///   (function (_this) { return _this.x; })
    /// }
    /// ```
    SubstituteThis {
        /// The capture variable name to substitute with (e.g., "_this" or "_this_1")
        capture_name: Arc<str>,
    },

    /// Substitute arguments with _arguments (lexical capture for arrow functions)
    ///
    /// When an arrow function captures `arguments`, all `arguments` references
    /// inside it should be substituted with `_arguments` (the capture variable).
    ///
    /// ```typescript
    /// function outer() {
    ///   return () => arguments.length;
    /// }
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// function outer() {
    ///   return function (_arguments) { return _arguments.length; }(arguments);
    /// }
    /// ```
    SubstituteArguments,

    /// ES5 Super Call: Transform super(...args) to _super.call(this, ...args)
    ///
    /// ```typescript
    /// class Foo extends Bar {
    ///   constructor(x) {
    ///     super(x);
    ///   }
    /// }
    /// ```
    ///
    /// Becomes:
    ///
    /// ```javascript
    /// var Foo = /** @class */ (function (_super) {
    ///   __extends(Foo, _super);
    ///   function Foo(x) {
    ///     _super.call(this, x);
    ///   }
    ///   return Foo;
    /// }(Bar));
    /// ```
    ES5SuperCall,

    /// Module Wrapper: Wrap entire file for AMD/System/UMD
    ModuleWrapper {
        /// Module format (AMD, System, UMD)
        format: ModuleFormat,
        /// Dependencies
        dependencies: Arc<[String]>,
    },

    /// Chain multiple transforms (composition)
    /// Transforms are applied in order.
    Chain(Vec<TransformDirective>),
}

/// Module formats that require wrapping transforms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleFormat {
    CommonJS,
    AMD,
    System,
    UMD,
    ES6,
}

/// Transform context maps node indices to their transform directives
#[derive(Clone)]
pub struct TransformContext {
    /// Map of NodeIndex -> TransformDirective
    /// Only contains entries for nodes that need transformation
    directives: FxHashMap<NodeIndex, TransformDirective>,
    /// Helper usage derived during lowering (optional).
    helpers: HelpersNeeded,
    helpers_populated: bool,
    /// Function body block nodes that need `var _this = this;` at the start.
    /// When an arrow function captures `this`, the enclosing non-arrow function's
    /// body block is added here so the emitter knows to inject the capture statement.
    /// The value is the capture variable name (e.g., "_this" or "_this_1" on collision).
    this_capture_scopes: FxHashMap<NodeIndex, Arc<str>>,
}

impl TransformContext {
    /// Create a new empty transform context
    pub fn new() -> Self {
        TransformContext {
            directives: FxHashMap::default(),
            helpers: HelpersNeeded::default(),
            helpers_populated: false,
            this_capture_scopes: FxHashMap::default(),
        }
    }

    /// Register a transform directive for a node
    pub fn insert(&mut self, node: NodeIndex, directive: TransformDirective) {
        self.directives.insert(node, directive);
    }

    /// Get the transform directive for a node, if any
    pub fn get(&self, node: NodeIndex) -> Option<&TransformDirective> {
        self.directives.get(&node)
    }

    /// Access helper usage recorded during lowering.
    pub fn helpers(&self) -> &HelpersNeeded {
        &self.helpers
    }

    /// Mutate helper usage, marking it as populated.
    pub fn helpers_mut(&mut self) -> &mut HelpersNeeded {
        self.helpers_populated = true;
        &mut self.helpers
    }

    /// Check if helper usage has been populated by a lowering pass.
    pub fn helpers_populated(&self) -> bool {
        self.helpers_populated
    }

    /// Mark helper usage as populated without changing flags.
    pub fn mark_helpers_populated(&mut self) {
        self.helpers_populated = true;
    }

    /// Iterate over all registered directives.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeIndex, &TransformDirective)> {
        self.directives.iter()
    }

    /// Mark a function body block as needing `var _this = this;` at the start.
    /// `capture_name` is the variable name to use (e.g., "_this" or "_this_1" on collision).
    pub fn mark_this_capture_scope(&mut self, body_idx: NodeIndex, capture_name: Arc<str>) {
        self.this_capture_scopes.insert(body_idx, capture_name);
    }

    /// Check if a function body block needs `var _this = this;` at the start.
    /// Returns the capture variable name if so.
    pub fn this_capture_name(&self, body_idx: NodeIndex) -> Option<&str> {
        self.this_capture_scopes.get(&body_idx).map(|s| &**s)
    }

    /// Check if a node has a transform directive
    pub fn has_transform(&self, node: NodeIndex) -> bool {
        self.directives.contains_key(&node)
    }

    /// Clear all directives (for reuse)
    pub fn clear(&mut self) {
        self.directives.clear();
        self.helpers = HelpersNeeded::default();
        self.helpers_populated = false;
    }

    /// Get the number of registered transforms
    pub fn len(&self) -> usize {
        self.directives.len()
    }

    /// Check if the context is empty
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }
}

impl Default for TransformContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/transform_context.rs"]
mod tests;
