use crate::context::emit::EmitContext;
use crate::context::transform::{TransformContext, TransformDirective};
use crate::enums::evaluator::EnumValue;
use crate::output::source_writer::{
    LineMap, SourcePosition, SourceWriter, source_position_from_offset,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::warn;
use tsz_common::common::{ModuleKind, NewLineKind, ScriptTarget};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// A class field initializer entry: (`field_name`, `initializer_node`, `init_end`, `leading_comments`, `trailing_comments`).
pub(crate) type FieldInit = (String, NodeIndex, u32, Vec<String>, Vec<String>);

/// A const enum entry scoped to a specific region of the source.
/// File-level const enums use `(0, u32::MAX)` so they match any position.
/// Function-scoped const enums use the enclosing function's `(pos, end)`.
#[derive(Debug, Clone)]
pub(crate) struct ScopedConstEnum {
    pub scope_start: u32,
    pub scope_end: u32,
    pub values: FxHashMap<String, EnumValue>,
}

/// Info about a private class member for lowering.
/// Determines the kind argument for `__classPrivateFieldGet`/`__classPrivateFieldSet`.
#[derive(Debug, Clone)]
pub(crate) struct PrivateMemberInfo {
    /// The kind: "f" for field, "m" for method, "a" for accessor.
    pub kind: &'static str,
    /// For static fields: the function ref variable name (e.g., `_C_field`).
    /// For methods: the function variable name (e.g., `_C_method`).
    /// For accessors: the getter variable name (e.g., `_C_prop_get`).
    pub fn_ref: Option<String>,
    /// For accessors: the setter variable name (e.g., `_C_prop_set`).
    pub setter_ref: Option<String>,
    /// Whether this is a static member.
    /// Currently set but not read — reserved for future static private member emission.
    #[allow(dead_code)]
    pub is_static: bool,
    /// The WeakSet/class-alias variable used as the `state` argument.
    /// For instance methods/accessors: `_ClassName_instances`.
    /// For static members: the class alias variable.
    pub state_var: Option<String>,
}

/// Info about a private accessor function to emit after the class body.
#[derive(Debug, Clone)]
pub(crate) struct PrivateAccessorDef {
    /// The variable name (e.g., `_C_prop_get`).
    pub var_name: String,
    /// The body node index.
    pub body: NodeIndex,
    /// Optional setter parameter node index.
    pub param: Option<NodeIndex>,
    /// Whether this is an async accessor.
    #[allow(dead_code)]
    pub is_async: bool,
}

/// How a class property name should be emitted in `ClassName.name = ...` assignments.
#[derive(Clone)]
pub(crate) enum PropertyNameEmit {
    /// Identifier: `ClassName.foo = ...`
    Dot(String),
    /// String literal: `ClassName["foo"] = ...`
    Bracket(String),
    /// Numeric literal: `ClassName[0] = ...`
    BracketNumeric(String),
}

// =============================================================================
// Emitter Options
// =============================================================================

/// JSX emit mode — controls how JSX is transformed in the output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JsxEmit {
    /// Keep the JSX as part of the output (default).
    #[default]
    Preserve = 0,
    /// Classic transform: `React.createElement(tag, props, ...children)`.
    React = 1,
    /// Automatic transform: `_jsx(tag, { children, ...props })`.
    ReactJsx = 2,
    /// Automatic dev transform: `_jsxDEV(tag, { children, ...props }, ...)`.
    ReactJsxDev = 3,
    /// Keep the JSX but emit `.js` files (React Native).
    ReactNative = 4,
}

/// Printer configuration options.
#[derive(Clone, Debug)]
pub struct PrinterOptions {
    /// Remove comments from output
    pub remove_comments: bool,
    /// Target ECMAScript version
    pub target: ScriptTarget,
    /// Use single quotes for strings
    pub single_quote: bool,
    /// Omit trailing semicolons
    pub omit_trailing_semicolon: bool,
    /// Don't emit helpers
    pub no_emit_helpers: bool,
    /// Module kind
    pub module: ModuleKind,
    /// New line character
    pub new_line: NewLineKind,
    /// Downlevel iteration (for-of with full iterator protocol)
    pub downlevel_iteration: bool,
    /// Set of import specifier nodes that should be elided (type-only imports)
    pub type_only_nodes: Arc<FxHashSet<NodeIndex>>,
    /// Emit "use strict" for every source file
    pub always_strict: bool,
    /// Emit class fields using Object.defineProperty semantics when downleveling
    pub use_define_for_class_fields: bool,
    /// Enable legacy (experimental) decorator lowering (`__decorate` style)
    pub legacy_decorators: bool,
    /// Emit design-type metadata for decorated declarations (`__metadata` style)
    pub emit_decorator_metadata: bool,
    /// Emit interop helpers (`__importStar`, `__importDefault`) for CJS/ESM interop
    pub es_module_interop: bool,
    /// When true, treat all non-declaration files as modules (moduleDetection=force)
    pub module_detection_force: bool,
    /// When true, this file was resolved from Node16/NodeNext to ESM based on
    /// file extension (.mts) or package.json "type":"module". Such files are
    /// definitively ES modules regardless of import/export content.
    pub resolved_node_module_to_esm: bool,
    /// When true, this file was resolved from a Node module (node16/nodenext) to CJS format.
    /// In this context, dynamic `import()` should be kept as native `import()` (Node CJS supports it)
    /// rather than being transformed to `require()`.
    pub resolved_node_module_to_cjs: bool,
    /// When true, preserve const enum declarations instead of erasing them
    pub preserve_const_enums: bool,
    /// When true, disable const enum value inlining at usage sites.
    /// Set by `--isolatedModules` and `--verbatimModuleSyntax` which prevent
    /// cross-file const enum inlining. Note: `--preserveConstEnums` alone
    /// preserves declarations but still inlines values.
    pub no_const_enum_inlining: bool,
    /// Import helpers from tslib instead of inlining them
    pub import_helpers: bool,
    /// JSX emit mode
    pub jsx: JsxEmit,
    /// Custom JSX factory function (e.g. "React.createElement", "h")
    pub jsx_factory: Option<String>,
    /// Custom JSX fragment factory (e.g. "React.Fragment", "Fragment")
    pub jsx_fragment_factory: Option<String>,
    /// Module specifier for automatic JSX runtime (e.g. "react")
    pub jsx_import_source: Option<String>,
    /// When true, suppress "use strict" emission even if module kind is CJS.
    /// Set when module was overridden from ESM/preserve to CJS for .cts/.cjs files.
    pub suppress_use_strict: bool,
    /// When true, null and undefined are meaningful types in unions for metadata serialization.
    /// Matches tsc's strictNullChecks behavior in decorator metadata emission.
    pub strict_null_checks: bool,
    /// When true, do not elide any imports or exports not explicitly marked as type-only.
    /// Corresponds to `--verbatimModuleSyntax`.
    pub verbatim_module_syntax: bool,
    /// When true, rewrite `.ts`/`.tsx`/`.mts`/`.cts` extensions to `.js`/`.jsx`/`.mjs`/`.cjs`
    /// in relative import/export specifiers during emit.
    pub rewrite_relative_import_extensions: bool,
}

impl Default for PrinterOptions {
    fn default() -> Self {
        Self {
            remove_comments: false,
            // Default to ES2024 to match tsgo/tsc 7.x behavior.
            // ESNext loads 12 additional esnext-specific lib files (87 vs 75)
            // that add startup overhead without benefit for most users.
            target: ScriptTarget::ES2024,
            single_quote: false,
            omit_trailing_semicolon: false,
            no_emit_helpers: false,
            module: ModuleKind::None,
            new_line: NewLineKind::LineFeed,
            downlevel_iteration: false,
            type_only_nodes: Arc::new(FxHashSet::default()),
            always_strict: false,
            use_define_for_class_fields: false,
            legacy_decorators: false,
            emit_decorator_metadata: false,
            es_module_interop: false,
            module_detection_force: false,
            resolved_node_module_to_esm: false,
            resolved_node_module_to_cjs: false,
            preserve_const_enums: false,
            no_const_enum_inlining: false,
            import_helpers: false,
            jsx: JsxEmit::Preserve,
            jsx_factory: None,
            jsx_fragment_factory: None,
            jsx_import_source: None,
            suppress_use_strict: false,
            strict_null_checks: false,
            verbatim_module_syntax: false,
            rewrite_relative_import_extensions: false,
        }
    }
}

#[derive(Default)]
pub(crate) struct ParamTransformPlan {
    pub(crate) params: Vec<ParamTransform>,
    pub(crate) rest: Option<RestParamTransform>,
}

#[derive(Default)]
pub(crate) struct TempScopeState {
    pub(crate) temp_var_counter: u32,
    pub(crate) generated_temp_names: FxHashSet<String>,
    pub(crate) first_for_of_emitted: bool,
    pub(crate) preallocated_temp_names: VecDeque<String>,
    pub(crate) preallocated_assignment_temps: VecDeque<String>,
    pub(crate) preallocated_logical_assignment_value_temps: VecDeque<String>,
    pub(crate) hoisted_assignment_value_temps: Vec<String>,
    pub(crate) hoisted_assignment_temps: Vec<String>,
}

impl ParamTransformPlan {
    pub(crate) const fn has_transforms(&self) -> bool {
        !self.params.is_empty() || self.rest.is_some()
    }
}

pub(crate) struct ParamTransform {
    pub(crate) name: String,
    pub(crate) pattern: Option<NodeIndex>,
    pub(crate) initializer: Option<NodeIndex>,
}

pub(crate) struct RestParamTransform {
    pub(crate) name: String,
    pub(crate) pattern: Option<NodeIndex>,
    pub(crate) index: usize,
}

pub(crate) struct TemplateParts {
    pub(crate) cooked: Vec<String>,
    pub(crate) raw: Vec<String>,
    pub(crate) expressions: Vec<NodeIndex>,
}

// =============================================================================
// Printer
// =============================================================================

/// Maximum recursion depth for emit to prevent infinite loops
const MAX_EMIT_RECURSION_DEPTH: u32 = 1000;

/// Printer that works with `NodeArena`.
///
/// Uses `SourceWriter` for output generation (enables source map support).
/// Uses `EmitContext` for transform-specific state management.
/// Uses `TransformContext` for directive-based transforms (Phase 2 architecture).
pub struct Printer<'a> {
    /// The `NodeArena` containing the AST.
    pub(crate) arena: &'a NodeArena,

    /// Source writer for output generation and source map tracking
    pub(crate) writer: SourceWriter,

    /// Emit context holding options and transform state
    pub(crate) ctx: EmitContext,

    /// Transform directives from lowering pass (optional, defaults to empty)
    pub(crate) transforms: TransformContext,

    /// Emit `void 0` for missing initializers during recovery.
    pub(crate) emit_missing_initializer_as_void_0: bool,

    /// Source text for detecting single-line constructs
    pub(crate) source_text: Option<&'a str>,

    /// Source text for source map generation (kept separate from comment emission).
    pub(crate) source_map_text: Option<&'a str>,

    /// Precomputed line map for O(log n) line/column lookups from byte offsets.
    /// Built once when source text is set; avoids O(n^2) scanning during emission.
    pub(crate) line_map: Option<LineMap>,

    /// Pending source position for mapping the next write.
    pub(crate) pending_source_pos: Option<SourcePosition>,

    /// Recursion depth counter to prevent infinite loops
    emit_recursion_depth: u32,

    /// All comments in the source file, collected once during `emit_source_file`.
    /// Used for distributing comments to blocks and other nested constructs.
    pub(crate) all_comments: Vec<tsz_common::comments::CommentRange>,

    /// Shared index into `all_comments`, monotonically advancing as comments are emitted.
    /// Used across `emit_source_file` and `emit_block` to prevent double-emission.
    pub(crate) comment_emit_idx: usize,

    /// All identifier texts in the source file.
    /// Collected once at `emit_source_file` start for temp name collision detection.
    /// Mirrors TypeScript's `sourceFile.identifiers` used by `makeUniqueName`.
    pub(crate) file_identifiers: FxHashSet<String>,

    /// Set of generated temp names (_a, _b, etc.) to avoid collisions.
    /// Tracks ALL generated temp names across destructuring and for-of lowering.
    pub(crate) generated_temp_names: FxHashSet<String>,

    /// Stack for saving/restoring temp naming state when entering function scopes.
    pub(crate) temp_scope_stack: Vec<TempScopeState>,

    /// Whether the first for-of loop has been emitted (uses special `_i` index name).
    pub(crate) first_for_of_emitted: bool,

    /// Whether we're inside a namespace IIFE (strip export/default modifiers from classes).
    pub(crate) in_namespace_iife: bool,

    /// End position of the current namespace body in source text.
    /// Used to scope reference searches for namespace-scoped import aliases.
    pub(crate) namespace_scope_end: u32,

    /// When set, the next enum emit should fold the namespace export into the IIFE closing.
    /// E.g., `(Color = A.Color || (A.Color = {}))` instead of `(Color || (Color = {}))`.
    pub(crate) enum_namespace_export: Option<String>,

    /// Set to true when the next `MODULE_DECLARATION` emit should use parent namespace
    /// assignment in its IIFE closing. This is set by `emit_namespace_body_statements`
    /// when the module is wrapped in an `EXPORT_DECLARATION`.
    pub(crate) namespace_export_inner: bool,

    /// Marker that the next block emission is a function body.
    pub(crate) emitting_function_body_block: bool,

    /// The name of the current namespace we're emitting inside (if any).
    /// Used for nested exported namespaces to emit proper IIFE parameters.
    pub(crate) current_namespace_name: Option<String>,
    /// Parent namespace name for scope-qualified `namespace_prior_exports` keys.
    /// Used to distinguish same-named nested namespaces (e.g., `m1.m2` vs `m4.m2`).
    pub(crate) parent_namespace_name: Option<String>,

    /// Override name for anonymous default exports (e.g., "`default_1`").
    /// When set, class/function emitters use this instead of leaving the name blank.
    pub(crate) anonymous_default_export_name: Option<String>,

    /// Counter used for disposable resource environment names (`env_1`, `env_2`, ...).
    pub(crate) next_disposable_env_id: u32,

    /// When set, a block-level using-lowering try/catch is active. `using` variable
    /// statements should emit `const x = __addDisposableResource(env, expr, async)`
    /// instead of their own try/catch wrapper. The tuple is (`env_name`, `is_async`).
    pub(crate) block_using_env: Option<(String, bool)>,

    /// True while emitting statements inside a wrapped top-level using region.
    /// This distinguishes post-`using` lowered statements from pre-`using` ones.
    pub(crate) in_top_level_using_scope: bool,

    /// Type parameter names of the class currently being decorated (for metadata serialization).
    /// Set during `emit_legacy_member_decorator_calls` so `serialize_type_for_metadata` can
    /// resolve generic type parameters to "Object".
    pub(crate) metadata_class_type_params: Option<Vec<String>>,

    /// When true, the next namespace IIFE tail should fold `exports.Name` into
    /// the closing: `(N || (exports.N = N = {}))` instead of `(N || (N = {}))`.
    pub(crate) pending_cjs_namespace_export_fold: bool,

    /// For CommonJS class exports, emit `exports.X = X;` immediately after class
    /// declaration and before post-class lowered statements (static fields/blocks).
    pub(crate) pending_commonjs_class_export_name: Option<String>,

    /// Names of namespaces already declared with `var name;` to avoid duplicates.
    pub(crate) declared_namespace_names: FxHashSet<String>,

    /// Incrementing counter per namespace name for IIFE parameter conflict renaming.
    /// When a namespace body has a declaration conflicting with the namespace name,
    /// tsc renames the IIFE parameter with incrementing suffixes: `M_1`, `M_2`, `M_3`, etc.
    pub(crate) namespace_iife_param_counter: FxHashMap<String, u32>,

    /// Accumulated exported variable names per namespace name, used for cross-block
    /// export substitution in namespace IIFEs. When a second `namespace M { ... }` block
    /// references `x` exported by the first block, this map provides the prior exports
    /// so the transformer can rewrite `x` → `M.x`.
    pub(crate) namespace_prior_exports: FxHashMap<String, std::collections::HashSet<String>>,

    /// Exported variable/function/class names in the current namespace IIFE.
    /// Used to qualify identifier references: `foo` → `ns.foo`.
    pub(crate) namespace_exported_names: FxHashSet<String>,

    /// Names of variables exported from the current CJS module.
    /// Used to qualify identifier reads: `x` → `exports.x` in expression positions.
    pub(crate) commonjs_exported_var_names: FxHashSet<String>,

    /// Deferred local export bindings active for the current wrapped region.
    /// Maps local variable names to their exported names so nested variable
    /// statements can append the right export binding after initialization.
    pub(crate) deferred_local_export_bindings: Option<FxHashMap<String, String>>,

    /// When true, an inline block comment (`/* ... */`) was just emitted without a trailing
    /// newline. The next `write()` call should insert a space before non-whitespace text.
    /// This avoids double-spacing with expression emitters that handle their own comment spacing.
    pub(crate) pending_block_comment_space: bool,

    /// When true, suppress namespace identifier qualification (emitting a declaration name).
    pub(crate) suppress_ns_qualification: bool,

    /// When true, do not substitute CommonJS named imports while emitting identifiers.
    /// Used for property-name positions like `obj.name`.
    pub(crate) suppress_commonjs_named_import_substitution: bool,

    /// Pending class field initializers to inject into constructor body.
    /// Each entry is (`field_name`, `initializer_node_index`, `init_end`, `trailing_comments`).
    /// `init_end` is used for trailing comment emission in synthesized constructors.
    /// `leading_comments` are pre-collected for comments before the property declaration.
    /// `trailing_comments` are pre-collected during class body iteration for existing constructors.
    pub(crate) pending_class_field_inits: Vec<FieldInit>,

    /// Pending auto-accessor field initializers to emit in constructor body.
    /// Each tuple is (`weakmap_storage_name`, `initializer_expression`).
    /// `initializer_expression` is `None` when the accessor field has no
    /// initializer and should default to `void 0`.
    pub(crate) pending_auto_accessor_inits: Vec<(String, Option<NodeIndex>)>,

    /// Temp names for assignment target values that need to be hoisted as `var _a, _b, ...;`.
    /// These are emitted on a separate declaration list before reference temps.
    pub(crate) hoisted_assignment_value_temps: Vec<String>,

    /// Temp names for assignment target values that must be reserved before references.
    /// These are used by `make_unique_name_hoisted_value`.
    pub(crate) preallocated_logical_assignment_value_temps: VecDeque<String>,

    /// Temp names for assignment target values that must be reserved before references.
    /// These are used by `make_unique_name_hoisted_assignment`.
    pub(crate) preallocated_assignment_temps: VecDeque<String>,

    /// Temp variable names that need to be hoisted to the top of the current scope
    /// as `var _a, _b, ...;`. Used for assignment targets in helper expressions.
    pub(crate) hoisted_assignment_temps: Vec<String>,

    /// Temp variable names for CJS/AMD exported destructuring patterns.
    /// These are emitted as `var _a, _b;` BEFORE the `__esModule` marker,
    /// matching tsc's placement (between "use strict" and Object.defineProperty).
    pub(crate) cjs_destructuring_export_temps: Vec<String>,

    /// Byte offset where CJS destructuring export temps should be inserted.
    pub(crate) cjs_destr_hoist_byte_offset: usize,
    /// Line number where CJS destructuring export temps should be inserted.
    pub(crate) cjs_destr_hoist_line: u32,

    /// Temp names reserved ahead-of-time and consumed before generating new names.
    pub(crate) preallocated_temp_names: VecDeque<String>,

    /// Temp names for ES5 iterator-based for-of lowering that must be emitted
    /// as top-level `var` declarations (e.g., `e_1, _a, e_2, _b`).
    pub(crate) hoisted_for_of_temps: Vec<String>,

    /// CommonJS named import substitutions (e.g. `f` -> `demoModule_1.f`).
    /// Used to match tsc emit where named imports are referenced via module temps.
    pub(crate) commonjs_named_import_substitutions: FxHashMap<String, String>,

    /// Pre-allocated return-temp names for iterator for-of nodes.
    /// This lets nested loops reserve their return temp before outer loop
    /// iterator/result temps, matching tsc temp ordering.
    pub(crate) reserved_iterator_return_temps: FxHashMap<NodeIndex, String>,

    /// Pending object rest parameter replacements for ES2018 lowering.
    /// When a function parameter has `{ a, ...rest }`, the parameter is replaced with a temp
    /// and this stores `(temp_name, pattern_idx)` for body preamble emission.
    pub(crate) pending_object_rest_params: Vec<(String, NodeIndex)>,

    /// Current nesting depth of function/method/constructor scopes.
    /// Used to determine if we're inside a function scope (depth > 0) or at top level (0).
    pub(crate) function_scope_depth: u32,

    /// Current nesting depth for iterator for-of emission.
    pub(crate) iterator_for_of_depth: usize,

    /// Current nesting depth for destructuring emission that should wrap spread inputs with `__read`.
    pub(crate) destructuring_read_depth: u32,

    /// When true, the current parenthesized expression is being emitted as the
    /// base of a property/element access. This prevents stripping parens around
    /// `new` expressions where removal would change semantics: `(new a).b` vs `new a.b`.
    pub(crate) paren_in_access_position: bool,

    /// True when emitting inside a System.register `execute` function body.
    /// Used to substitute `import.meta` with `context_1.meta`.
    pub(crate) in_system_execute_body: bool,

    /// When true, the current parenthesized expression is being emitted as the
    /// callee of a `new` expression. This prevents stripping parens around
    /// call expressions where removal would change semantics:
    /// `new (x() as T)` → `new (x())` (not `new x()`).
    pub(crate) paren_in_new_callee: bool,

    /// Depth counter for accessor members emitted from object literal syntax.
    object_literal_accessor_depth: u32,

    /// Whether the current root source file has a JavaScript-like extension.
    pub(crate) is_current_root_js_source: bool,

    /// Const enum member values for inlining at usage sites.
    /// Maps `enum_name -> Vec<ScopedConstEnum>`.  Each entry carries the
    /// position range of the scope it was declared in so that at inline time
    /// we pick the right entry (the tightest scope that contains the access).
    /// File-level const enums use `(0, u32::MAX)`.
    pub(crate) const_enum_values: FxHashMap<String, Vec<ScopedConstEnum>>,

    /// Accumulated enum member values across all processed enum declarations.
    /// Used by `EnumES5Transformer` to resolve cross-enum references like
    /// `Foo.a` in `enum Bar { B = Foo.a }`.
    /// Keyed by `enum_name` → `member_name` → value.
    pub(crate) prior_enum_member_values: FxHashMap<String, FxHashMap<String, i64>>,

    /// String enum member names from previously-evaluated enums.
    /// Used to detect cross-enum string member references in `is_syntactically_string`.
    pub(crate) prior_enum_string_members: FxHashMap<String, FxHashSet<String>>,

    /// Private field `WeakMap` mapping for ES2015-ES2021 class private field lowering.
    /// Maps `field_name` (without `#`) → `_ClassName_fieldName` (`WeakMap` variable name).
    /// When non-empty, property accesses with private identifiers are lowered to
    /// `__classPrivateFieldGet`/`__classPrivateFieldSet` helper calls.
    pub(crate) private_field_weakmaps: FxHashMap<String, String>,

    /// Private member kind info for ES2015-ES2021 lowering.
    /// Maps `field_name` (without `#`) → `PrivateMemberKind`.
    /// Used to determine the correct kind argument ("f", "m", "a") and
    /// additional function ref for `__classPrivateFieldGet`/`__classPrivateFieldSet`.
    pub(crate) private_member_info: FxHashMap<String, PrivateMemberInfo>,

    /// Pending `WeakMap` initializations to emit after the class body.
    /// Each entry is `_ClassName_fieldName = new WeakMap()`.
    pub(crate) pending_weakmap_inits: Vec<String>,

    /// Pending static private field value initializations to emit after the class body.
    /// Each entry is `(var_name, initializer_idx)` producing `_ClassName_field = { value: <init> };`
    pub(crate) pending_static_private_inits: Vec<(String, NodeIndex)>,

    /// Class alias variable name for static private member access (e.g., `_a`).
    /// Emitted as `_a = ClassName;` after the class body.
    pub(crate) pending_private_class_alias: Option<(String, String)>,

    /// Private field constructor inits: (`weakmap_name`, `has_initializer`, `initializer_idx`).
    /// Emitted as `_C_field.set(this, <init>)` at the start of the constructor.
    pub(crate) pending_private_field_constructor_inits: Vec<(String, bool, NodeIndex)>,

    /// `WeakSet` instance name for `_X_instances.add(this)` in the constructor.
    /// Set when the class has private instance methods or accessors.
    pub(crate) pending_instances_weakset_add: Option<String>,

    /// Private method/accessor function definitions to emit after the class body.
    /// Each entry is (`var_name`, `body_idx`, params) for `_C_method = function _C_method(params) { ... }`.
    /// These are joined with the WeakMap/WeakSet inits using comma separation.
    pub(crate) pending_private_method_defs: Vec<(String, NodeIndex, Vec<NodeIndex>)>,

    /// Private accessor function definitions to emit after the class body.
    /// Each entry is (`var_name`, `body_idx`) for `_C_prop_get = function _C_prop_get() { ... }`.
    pub(crate) pending_private_accessor_defs: Vec<PrivateAccessorDef>,

    /// Set of private method/accessor names (without #) that should be skipped
    /// from the class body because they're extracted as standalone functions.
    pub(crate) private_members_to_skip: FxHashSet<String>,

    /// When true, class emitter defers static block IIFEs.
    pub(crate) defer_class_static_blocks: bool,

    /// Deferred static block IIFEs.
    pub(crate) deferred_class_static_blocks: Vec<(NodeIndex, usize)>,

    /// Source file name for jsx=react-jsxdev mode (e.g., "file.tsx").
    /// Used to emit `const _jsxFileName = "file.tsx";` and source location args.
    pub(crate) jsx_dev_file_name: Option<String>,

    /// When true, the current source file is a JavaScript file (.js/.jsx/.cjs/.mjs).
    /// JS files do not undergo import elision since all imports are value imports.
    pub(crate) source_is_js_file: bool,

    /// Mapping from computed property name expression `NodeIndex` to its hoisted temp
    /// variable name (e.g., `_a`). When target < ES2022 and a class member has a
    /// computed property name with a non-constant expression, the expression is hoisted
    /// to a temp variable and the class body uses the temp instead of the expression.
    pub(crate) computed_prop_temp_map: FxHashMap<NodeIndex, String>,

    /// Temporary alias for outer static `this` while emitting a static field initializer.
    /// This must not flow into nested non-arrow function or class scopes.
    pub(crate) scoped_static_this_alias: Option<Arc<str>>,

    /// When true, scoped static `super` lowering should emit direct property access/calls
    /// on the scoped base expression instead of `Reflect.get`.
    pub(crate) scoped_static_super_direct_access: bool,

    /// Temporary base-class alias for outer static `super` while emitting a static field
    /// initializer. This is cleared at the same nested scope boundaries as static `this`.
    pub(crate) scoped_static_super_base_alias: Option<Arc<str>>,
}

impl<'a> Printer<'a> {
    const DEFAULT_OUTPUT_CAPACITY: usize = 1024;

    pub(crate) fn estimate_output_capacity(source_len: usize) -> usize {
        // Emit output can be slightly smaller (type erasure) or significantly larger
        // (downlevel transforms/helpers). Bias toward ~1.5x while keeping a sane floor.
        source_len
            .saturating_mul(3)
            .saturating_div(2)
            .max(Self::DEFAULT_OUTPUT_CAPACITY)
    }

    /// Create a new Printer.
    pub fn new(arena: &'a NodeArena) -> Self {
        Self::with_options(arena, PrinterOptions::default())
    }

    /// Create a new Printer with options and source-length-informed preallocation.
    pub fn with_source_text_len_and_options(
        arena: &'a NodeArena,
        source_text_len: usize,
        options: PrinterOptions,
    ) -> Self {
        let capacity = Self::estimate_output_capacity(source_text_len);
        Self::with_capacity_and_options(arena, capacity, options)
    }

    /// Create a new Printer with source-length-informed preallocation.
    pub fn with_source_text_len(arena: &'a NodeArena, source_text_len: usize) -> Self {
        Self::with_source_text_len_and_options(arena, source_text_len, PrinterOptions::default())
    }

    /// Create a new Printer with options and root-node-informed preallocation.
    pub fn with_root_and_options(
        arena: &'a NodeArena,
        root: NodeIndex,
        options: PrinterOptions,
    ) -> Self {
        let source_text_len = arena
            .get(root)
            .and_then(|node| arena.get_source_file(node))
            .map_or(0, |source| source.text.len());
        Self::with_source_text_len_and_options(arena, source_text_len, options)
    }

    /// Create a new Printer with root-node-informed preallocation.
    pub fn with_root(arena: &'a NodeArena, root: NodeIndex) -> Self {
        Self::with_root_and_options(arena, root, PrinterOptions::default())
    }

    /// Create a new Printer with pre-allocated output capacity
    /// This reduces allocations when the expected output size is known (e.g., ~1.5x source size)
    pub fn with_capacity(arena: &'a NodeArena, capacity: usize) -> Self {
        Self::with_capacity_and_options(arena, capacity, PrinterOptions::default())
    }

    /// Create a new Printer with options.
    pub fn with_options(arena: &'a NodeArena, options: PrinterOptions) -> Self {
        Self::with_capacity_and_options(arena, Self::DEFAULT_OUTPUT_CAPACITY, options)
    }

    /// Create a new Printer with pre-allocated capacity and options.
    pub fn with_capacity_and_options(
        arena: &'a NodeArena,
        capacity: usize,
        options: PrinterOptions,
    ) -> Self {
        let mut writer = SourceWriter::with_capacity(capacity);
        writer.set_new_line_kind(options.new_line);

        // Create EmitContext from options (target controls ES5 vs ESNext)
        let ctx = EmitContext::with_options(options);

        Printer {
            arena,
            writer,
            ctx,
            transforms: TransformContext::new(), // Empty by default, can be set later
            emit_missing_initializer_as_void_0: false,
            source_text: None,
            source_map_text: None,
            line_map: None,
            pending_source_pos: None,
            emit_recursion_depth: 0,
            all_comments: Vec::new(),
            comment_emit_idx: 0,
            file_identifiers: FxHashSet::default(),
            generated_temp_names: FxHashSet::default(),
            temp_scope_stack: Vec::new(),
            pending_object_rest_params: Vec::new(),
            function_scope_depth: 0,
            first_for_of_emitted: false,
            in_namespace_iife: false,
            namespace_scope_end: u32::MAX,
            enum_namespace_export: None,
            namespace_export_inner: false,
            emitting_function_body_block: false,
            current_namespace_name: None,
            parent_namespace_name: None,
            anonymous_default_export_name: None,
            next_disposable_env_id: 1,
            block_using_env: None,
            in_top_level_using_scope: false,
            metadata_class_type_params: None,
            pending_block_comment_space: false,
            pending_cjs_namespace_export_fold: false,
            pending_commonjs_class_export_name: None,
            declared_namespace_names: FxHashSet::default(),
            namespace_iife_param_counter: FxHashMap::default(),
            namespace_prior_exports: FxHashMap::default(),
            namespace_exported_names: FxHashSet::default(),
            commonjs_exported_var_names: FxHashSet::default(),
            deferred_local_export_bindings: None,
            suppress_ns_qualification: false,
            suppress_commonjs_named_import_substitution: false,
            pending_class_field_inits: Vec::new(),
            pending_auto_accessor_inits: Vec::new(),
            hoisted_assignment_value_temps: Vec::new(),
            preallocated_logical_assignment_value_temps: VecDeque::new(),
            preallocated_assignment_temps: VecDeque::new(),
            hoisted_assignment_temps: Vec::new(),
            cjs_destructuring_export_temps: Vec::new(),
            cjs_destr_hoist_byte_offset: 0,
            cjs_destr_hoist_line: 0_u32,
            preallocated_temp_names: VecDeque::new(),
            hoisted_for_of_temps: Vec::new(),
            commonjs_named_import_substitutions: FxHashMap::default(),
            reserved_iterator_return_temps: FxHashMap::default(),
            iterator_for_of_depth: 0,
            destructuring_read_depth: 0,
            paren_in_access_position: false,
            in_system_execute_body: false,
            paren_in_new_callee: false,
            object_literal_accessor_depth: 0,
            is_current_root_js_source: false,
            const_enum_values: FxHashMap::default(),
            prior_enum_member_values: FxHashMap::default(),
            prior_enum_string_members: FxHashMap::default(),
            private_field_weakmaps: FxHashMap::default(),
            private_member_info: FxHashMap::default(),
            pending_weakmap_inits: Vec::new(),
            pending_static_private_inits: Vec::new(),
            pending_private_class_alias: None,
            pending_private_field_constructor_inits: Vec::new(),
            pending_instances_weakset_add: None,
            pending_private_method_defs: Vec::new(),
            pending_private_accessor_defs: Vec::new(),
            private_members_to_skip: FxHashSet::default(),
            defer_class_static_blocks: false,
            deferred_class_static_blocks: Vec::new(),
            jsx_dev_file_name: None,
            source_is_js_file: false,
            computed_prop_temp_map: FxHashMap::default(),
            scoped_static_this_alias: None,
            scoped_static_super_direct_access: false,
            scoped_static_super_base_alias: None,
        }
    }

    /// Set whether the current root source file has a JavaScript-like extension.
    pub(crate) const fn set_current_root_js_source(&mut self, is_js_source: bool) {
        self.is_current_root_js_source = is_js_source;
    }

    /// Whether an accessor node is currently being emitted from object-literal syntax.
    pub(crate) const fn is_emitting_object_literal_accessor(&self) -> bool {
        self.object_literal_accessor_depth > 0
    }

    /// Emit an object-literal property node, marking accessor members to enable
    /// JS pass-through formatting rules (e.g., empty accessor-body spacing).
    pub(crate) fn emit_object_property(&mut self, property_idx: NodeIndex) {
        let is_accessor = self.arena.get(property_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::GET_ACCESSOR || node.kind == syntax_kind_ext::SET_ACCESSOR
        });

        if is_accessor {
            self.object_literal_accessor_depth =
                self.object_literal_accessor_depth.saturating_add(1);
        }

        self.emit(property_idx);

        if is_accessor {
            self.object_literal_accessor_depth =
                self.object_literal_accessor_depth.saturating_sub(1);
        }
    }

    /// Create a new Printer with transform directives.
    /// This is the Phase 2 constructor that accepts pre-computed transforms.
    pub fn with_transforms(arena: &'a NodeArena, transforms: TransformContext) -> Self {
        let mut printer = Self::new(arena);
        printer.transforms = transforms;
        printer
    }

    /// Create a new Printer with transforms and options.
    pub fn with_transforms_and_options(
        arena: &'a NodeArena,
        transforms: TransformContext,
        options: PrinterOptions,
    ) -> Self {
        let mut printer = Self::with_options(arena, options);
        printer.transforms = transforms;
        printer
    }

    /// Set whether to target ES5 behavior.
    ///
    /// This updates both the legacy `target_es5` bool and all derived
    /// per-version lowering gates in the shared context.
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.ctx.set_target_es5(es5);
    }

    /// Set the full script target.
    ///
    /// This keeps all derived feature gates synchronized, including `target_es5`.
    pub const fn set_target(&mut self, target: ScriptTarget) {
        self.ctx.set_target(target);
    }

    /// Set the module kind (`CommonJS`, ESM, etc.).
    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.ctx.options.module = kind;
    }

    /// Set auto-detect module mode. When enabled, the emitter will detect if
    /// the source file contains import/export statements and apply `CommonJS`
    /// transforms automatically.
    pub const fn set_auto_detect_module(&mut self, enabled: bool) {
        self.ctx.auto_detect_module = enabled;
    }

    /// Set the source text (for detecting single-line constructs).
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        self.line_map = Some(LineMap::new(text));
        let estimated = Self::estimate_output_capacity(text.len());
        self.writer.ensure_output_capacity(estimated);
    }

    /// Enable declaration emit mode for `.d.ts` output.
    ///
    /// Declaration mode changes emission behavior in multiple nodes, such as:
    /// - Skipping JS-only constructs
    /// - Emitting `declare` signatures instead of values
    /// - Keeping type-only information
    pub const fn set_declaration_emit(&mut self, enabled: bool) {
        self.ctx.flags.in_declaration_emit = enabled;
    }

    /// Set source text for source map generation without enabling comment emission.
    pub const fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    /// Enable source map generation and register the current source file.
    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.writer.enable_source_map(output_name.to_string());
        let content = self
            .source_text_for_map()
            .map(std::string::ToString::to_string);
        self.writer.add_source(source_name.to_string(), content);
    }

    /// Generate source map JSON (if enabled).
    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.writer.generate_source_map_json()
    }

    pub(crate) fn source_text_for_map(&self) -> Option<&'a str> {
        self.source_map_text.or(self.source_text)
    }

    /// Compute a `SourcePosition` from a byte offset, using the precomputed
    /// line map for O(log n) lookup when available, falling back to the O(n)
    /// linear scan otherwise.
    pub(crate) fn fast_source_position(&self, pos: u32) -> Option<SourcePosition> {
        if let Some(ref lm) = self.line_map {
            Some(lm.source_position(pos))
        } else if let Some(text) = self.source_text_for_map() {
            Some(source_position_from_offset(text, pos))
        } else {
            None
        }
    }

    fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        self.pending_source_pos = self.fast_source_position(node.pos);
    }

    /// Check if a node spans a single line in the source.
    /// Finds the first `{` and last `}` within the node's source span and checks
    /// if there's a newline between them. Uses depth counting to handle nested braces correctly.
    pub(crate) fn is_single_line(&self, node: &Node) -> bool {
        if let Some(text) = self.source_text {
            let actual_start = self.skip_trivia_forward(node.pos, node.end) as usize;
            // Use actual token end, not node.end which includes trailing trivia.
            // For example, `{ return x; }\n` has trailing newline in node.end,
            // but we want to check only `{ return x; }`.
            let token_end = self.find_token_end_before_trivia(node.pos, node.end);
            let end = std::cmp::min(token_end as usize, text.len());
            if actual_start < end {
                let slice = &text[actual_start..end];
                // Find the first `{` and its matching `}` using depth counting
                // to handle nested braces (e.g., `{ return new Line({ x: 0 }, p); }`)
                if let Some(open) = slice.find('{') {
                    let mut depth = 1;
                    let mut close = None;
                    for (i, ch) in slice[open + 1..].char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    close = Some(open + 1 + i);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(close) = close {
                        let inner = &slice[open..close + 1];
                        return !inner.contains('\n');
                    }
                }
                return !slice.contains('\n');
            }
        }
        false
    }

    /// Check if two nodes are on the same line in the source.
    pub(crate) fn are_on_same_line_in_source(
        &self,
        node1: tsz_parser::parser::NodeIndex,
        node2: tsz_parser::parser::NodeIndex,
    ) -> bool {
        if let Some(text) = self.source_text
            && let (Some(n1), Some(n2)) = (self.arena.get(node1), self.arena.get(node2))
        {
            let start = std::cmp::min(n1.end as usize, text.len());
            let end = std::cmp::min(n2.pos as usize, text.len());
            if start < end {
                // Check if there's a newline between the two nodes
                return !text[start..end].contains('\n');
            }
        }
        false
    }

    /// Get the output.
    pub fn get_output(&self) -> &str {
        self.writer.get_output()
    }

    /// Take the output.
    pub fn take_output(self) -> String {
        self.writer.take_output()
    }
    // =========================================================================
    // Main Emit Method
    // =========================================================================

    /// Emit a node by index.
    pub fn emit(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(source) = self.arena.get_source_file(node) {
            let file_name = source.file_name.to_ascii_lowercase();
            let is_js_source = file_name.ends_with(".js")
                || file_name.ends_with(".jsx")
                || file_name.ends_with(".mjs")
                || file_name.ends_with(".cjs");
            self.set_current_root_js_source(is_js_source);
        }

        if let Some(source) = self.arena.get_source_file(node)
            && self.transforms.is_empty()
        {
            let format = match self.ctx.options.module {
                ModuleKind::AMD => Some(crate::context::transform::ModuleFormat::AMD),
                ModuleKind::UMD => Some(crate::context::transform::ModuleFormat::UMD),
                ModuleKind::System => Some(crate::context::transform::ModuleFormat::System),
                _ => None,
            };
            if let Some(format) = format
                && self.file_is_module(&source.statements)
            {
                let dependencies = self.collect_module_dependencies(&source.statements.nodes);
                self.emit_module_wrapper(format, &dependencies, node, source, idx);
                return;
            }
        }

        self.emit_node(node, idx);
    }

    /// Emit a node in an expression context.
    /// If the node is missing or an error/unknown node, emits nothing (matching tsc behavior
    /// for parse error recovery — e.g. `const x = ;` rather than `const x = void 0;`).
    pub fn emit_expression(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Check if this is an error/unknown node
        use tsz_scanner::SyntaxKind;
        if node.kind == SyntaxKind::Unknown as u16 {
            return;
        }

        // Otherwise, emit normally
        self.emit_node(node, idx);
    }

    #[allow(dead_code)]
    pub(crate) fn emit_expression_with_scoped_static_initializer(
        &mut self,
        idx: NodeIndex,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
    ) {
        self.emit_expression_with_scoped_static_initializer_mode(
            idx,
            this_alias,
            super_base_alias,
            false,
        );
    }

    pub(crate) fn emit_expression_with_scoped_static_initializer_mode(
        &mut self,
        idx: NodeIndex,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
        super_direct_access: bool,
    ) {
        let prev_this_alias = self.scoped_static_this_alias.clone();
        let prev_super_direct_access = self.scoped_static_super_direct_access;
        let prev_super_alias = self.scoped_static_super_base_alias.clone();

        self.scoped_static_this_alias = this_alias.map(Arc::from);
        self.scoped_static_super_direct_access = super_direct_access;
        self.scoped_static_super_base_alias = super_base_alias.map(Arc::from);

        self.emit_expression(idx);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_direct_access = prev_super_direct_access;
        self.scoped_static_super_base_alias = prev_super_alias;
    }

    pub(in crate::emitter) fn with_scoped_static_initializer_context_cleared<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev_this_alias = self.scoped_static_this_alias.take();
        let prev_super_direct_access = self.scoped_static_super_direct_access;
        let prev_super_alias = self.scoped_static_super_base_alias.take();
        self.scoped_static_super_direct_access = false;
        let result = f(self);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_direct_access = prev_super_direct_access;
        self.scoped_static_super_base_alias = prev_super_alias;
        result
    }

    /// Emit a node.
    fn emit_node(&mut self, node: &Node, idx: NodeIndex) {
        // Recursion depth check to prevent infinite loops
        self.emit_recursion_depth += 1;
        if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
            // Log a warning about the recursion limit being exceeded.
            // This helps developers identify problematic deeply nested ASTs.
            warn!(
                depth = MAX_EMIT_RECURSION_DEPTH,
                node_kind = node.kind,
                node_pos = node.pos,
                "Emit recursion limit exceeded"
            );
            self.write("/* emit recursion limit exceeded */");
            self.emit_recursion_depth -= 1;
            return;
        }

        // Check transform directives first
        let has_transform = !self.transforms.is_empty()
            && Self::kind_may_have_transform(node.kind)
            && self.transforms.has_transform(idx);
        let previous_pending = self.pending_source_pos;

        self.queue_source_mapping(node);
        if has_transform {
            self.apply_transform(node, idx);
        } else {
            let kind = node.kind;
            self.emit_node_by_kind(node, idx, kind);
        }

        self.pending_source_pos = previous_pending;
        self.emit_recursion_depth -= 1;
    }

    const fn kind_may_have_transform(kind: u16) -> bool {
        matches!(
            kind,
            k if k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::MODULE_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Emit a node by kind using default logic (no transforms).
    /// This is the main dispatch method for emission.
    pub(crate) fn emit_node_by_kind(&mut self, node: &Node, idx: NodeIndex, kind: u16) {
        match kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => {
                // Check for substitution directives on identifier nodes.
                if self.transforms.has_transform(idx) {
                    if let Some(directive) = self.transforms.get(idx) {
                        match directive {
                            TransformDirective::SubstituteArguments => self.write("arguments"),
                            TransformDirective::SubstituteThis { capture_name } => {
                                let name = std::sync::Arc::clone(capture_name);
                                self.write(&name);
                            }
                            _ => self.emit_identifier(node),
                        }
                    } else {
                        self.emit_identifier(node);
                    }
                } else {
                    self.emit_identifier(node);
                }
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                // Private identifiers (#name) are emitted as-is for ES2022+ targets.
                // For < ES2022 targets, private identifiers in class members are
                // lowered by the class transform (WeakMap/WeakSet pattern).
                // Any remaining PrivateIdentifier nodes (e.g. in object literals or
                // enums, which are parse errors) are replaced with empty text,
                // matching tsc's classFields transformer behavior.
                if self.ctx.needs_es2022_lowering {
                    // Emit nothing — tsc replaces these with factory.createIdentifier("")
                } else if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                self.emit_type_parameter(node);
            }

            // Qualified name: A.B.C (used in type references, import types)
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(qn) = self.arena.get_qualified_name(node) {
                    self.emit(qn.left);
                    self.write(".");
                    self.emit(qn.right);
                }
            }

            // Literals
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.emit_numeric_literal(node);
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                self.emit_bigint_literal(node);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.emit_string_literal(node);
            }
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => {
                self.emit_regex_literal(node);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.write("false");
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.write("null");
            }

            // Binary expression
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.emit_binary_expression(node);
            }

            // Unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.emit_prefix_unary(node);
            }
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                self.emit_postfix_unary(node);
            }

            // Call expression
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.emit_call_expression(node);
            }

            // New expression
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.emit_new_expression(node);
            }

            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.emit_property_access(node);
            }

            // Meta property (new.target, import.meta)
            k if k == syntax_kind_ext::META_PROPERTY => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    // The expression is the keyword token (new/import)
                    if let Some(kw_node) = self.arena.get(access.expression) {
                        if kw_node.kind == SyntaxKind::NewKeyword as u16 {
                            self.write("new");
                        } else if kw_node.kind == SyntaxKind::ImportKeyword as u16 {
                            self.write("import");
                        }
                    }
                    self.write(".");
                    let name = self.get_identifier_text_idx(access.name_or_argument);
                    self.write(&name);
                }
            }

            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.emit_element_access(node);
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.emit_parenthesized(node);
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.emit_type_assertion_expression(node);
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.emit_non_null_expression(node);
            }

            // Conditional expression
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.emit_conditional(node);
            }

            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_array_literal(node);
            }

            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_object_literal(node);
            }

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                self.emit_arrow_function(node, idx);
            }

            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_function_expression(node, idx);
                });
            }

            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_function_declaration(node, idx);
                });
            }

            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.emit_variable_declaration(node);
            }

            // Variable declaration list
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                self.emit_variable_declaration_list(node);
            }

            // Variable statement
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_statement(node);
            }

            // Expression statement
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.emit_expression_statement(node);
            }

            // Block
            k if k == syntax_kind_ext::BLOCK => {
                self.emit_block(node, idx);
            }

            // Class static block: `static { ... }`
            // Treated like a function body for single-line formatting purposes.
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                self.write("static ");
                let prev = self.emitting_function_body_block;
                self.emitting_function_body_block = true;
                self.emit_block(node, idx);
                self.emitting_function_body_block = prev;
            }

            // If statement
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.emit_if_statement(node);
            }

            // While statement
            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.emit_while_statement(node);
            }

            // For statement
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.emit_for_statement(node);
            }

            // For-in statement
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                self.emit_for_in_statement(node);
            }

            // For-of statement
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                self.emit_for_of_statement(node);
            }

            // Return statement
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                self.emit_return_statement(node);
            }

            // Class declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_class_declaration(node, idx);
                });
            }

            // Class expression (e.g., `return class extends Base { ... }`)
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                self.emit_class_expression_with_captured_computed_names(node, idx);
            }

            // Property assignment
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.emit_property_assignment(node);
            }

            // Shorthand property assignment
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                self.emit_shorthand_property(node);
            }

            // Spread assignment in object literal: `{ ...expr }` (ES2018+ native spread)
            // For pre-ES2018 targets this is handled by emit_object_literal_with_object_assign.
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("...");
                    self.emit_expression(spread.expression);
                }
            }

            // Parameter declaration
            k if k == syntax_kind_ext::PARAMETER => {
                self.emit_parameter(node);
            }

            // Type keywords (for type annotations)
            k if k == SyntaxKind::NumberKeyword as u16 => self.write("number"),
            k if k == SyntaxKind::StringKeyword as u16 => self.write("string"),
            k if k == SyntaxKind::BooleanKeyword as u16 => self.write("boolean"),
            k if k == SyntaxKind::VoidKeyword as u16 => self.write("void"),
            k if k == SyntaxKind::AnyKeyword as u16 => self.write("any"),
            k if k == SyntaxKind::NeverKeyword as u16 => self.write("never"),
            k if k == SyntaxKind::UnknownKeyword as u16 => self.write("unknown"),
            k if k == SyntaxKind::UndefinedKeyword as u16 => self.write("undefined"),
            k if k == SyntaxKind::ObjectKeyword as u16 => self.write("object"),
            k if k == SyntaxKind::SymbolKeyword as u16 => self.write("symbol"),
            k if k == SyntaxKind::BigIntKeyword as u16 => self.write("bigint"),

            // Type reference
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                self.emit_type_reference(node);
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                self.emit_array_type(node);
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                self.emit_union_type(node);
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                self.emit_intersection_type(node);
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                self.emit_tuple_type(node);
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                self.emit_function_type(node);
            }

            // Constructor type: `new (...) => T`
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.emit_constructor_type(node);
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                self.emit_type_literal(node);
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                self.emit_parenthesized_type(node);
            }

            // Conditional type: T extends U ? X : Y
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                self.emit_conditional_type(node);
            }

            // Indexed access type: T[K]
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.emit_indexed_access_type(node);
            }

            // Infer type: infer U
            k if k == syntax_kind_ext::INFER_TYPE => {
                self.emit_infer_type(node);
            }

            // Literal type wrapper (string/number/boolean/bigint literals in type position)
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                self.emit_literal_type(node);
            }

            // Mapped type: { [P in keyof T]: T[P] }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                self.emit_mapped_type(node);
            }

            // Named tuple member: [name: Type]
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                self.emit_named_tuple_member(node);
            }

            // Optional type: T? (in tuple elements)
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                self.emit_optional_type(node);
            }

            // Rest type: ...T (in tuple elements)
            k if k == syntax_kind_ext::REST_TYPE => {
                self.emit_rest_type(node);
            }

            // Template literal type: `prefix${T}suffix`
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                self.emit_template_literal_type(node);
            }

            // this type in type position
            k if k == syntax_kind_ext::THIS_TYPE => {
                self.write("this");
            }

            // Type operator: keyof T, readonly T, unique symbol
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                self.emit_type_operator(node);
            }

            // Type predicate: x is T, asserts x is T
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                self.emit_type_predicate(node);
            }

            // Type query: typeof x
            k if k == syntax_kind_ext::TYPE_QUERY => {
                self.emit_type_query(node);
            }

            // Empty statement
            k if k == syntax_kind_ext::EMPTY_STATEMENT => {
                self.write_semicolon();
            }

            // JSX
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                self.emit_jsx_element(node);
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.emit_jsx_self_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_ELEMENT => {
                self.emit_jsx_opening_element(node);
            }
            k if k == syntax_kind_ext::JSX_CLOSING_ELEMENT => {
                self.emit_jsx_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                self.emit_jsx_fragment(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_FRAGMENT => {
                self.write("<>");
            }
            k if k == syntax_kind_ext::JSX_CLOSING_FRAGMENT => {
                self.write("</>");
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                self.emit_jsx_attributes(node);
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                self.emit_jsx_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                self.emit_jsx_spread_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                self.emit_jsx_expression(node);
            }
            k if k == SyntaxKind::JsxText as u16 => {
                self.emit_jsx_text(node);
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                self.emit_jsx_namespaced_name(node);
            }

            // Imports/Exports
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.emit_import_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.emit_import_equals_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                self.emit_import_clause(node);
            }
            k if k == syntax_kind_ext::NAMED_IMPORTS || k == syntax_kind_ext::NAMESPACE_IMPORT => {
                self.emit_named_imports(node);
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                self.emit_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(node);
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT => {
                // `* as name` in `export * as name from "..."`
                if let Some(data) = self.arena.get_named_imports(node) {
                    self.write("* as ");
                    self.emit(data.name);
                }
            }
            k if k == syntax_kind_ext::NAMED_EXPORTS => {
                self.emit_named_exports(node);
            }
            k if k == syntax_kind_ext::EXPORT_SPECIFIER => {
                self.emit_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(node);
            }

            // Additional statements
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.emit_throw_statement(node);
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.emit_try_statement(node);
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                self.emit_catch_clause(node);
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.emit_switch_statement(node);
            }
            k if k == syntax_kind_ext::CASE_CLAUSE => {
                self.emit_case_clause(node);
            }
            k if k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.emit_default_clause(node);
            }
            k if k == syntax_kind_ext::CASE_BLOCK => {
                self.emit_case_block(node);
            }
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                self.emit_break_statement(node);
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.emit_continue_statement(node);
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                self.emit_labeled_statement(node);
            }
            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.emit_do_statement(node);
            }
            k if k == syntax_kind_ext::DEBUGGER_STATEMENT => {
                self.emit_debugger_statement(node);
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                self.emit_with_statement(node);
            }

            // Declarations
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(node, idx);
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                self.emit_enum_member(node);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                // Interface declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_interface_declaration(node);
                } else {
                    // Skip comments belonging to erased declarations so they don't
                    // get emitted later by gap/before-pos comment handling.
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // Type alias declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_type_alias_declaration(node);
                } else {
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                // `export as namespace X` is TypeScript-only (UMD global declaration) -
                // erased in JS output, preserved only in .d.ts declaration emit.
                if self.ctx.flags.in_declaration_emit {
                    self.emit_namespace_export_declaration(node);
                } else {
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(node, idx);
            }

            // Computed property name: [expr]
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.write("[");
                    // If this expression has been hoisted to a temp variable, emit the
                    // temp name instead of the original expression.
                    if let Some(temp_name) = self.computed_prop_temp_map.get(&computed.expression) {
                        self.write(&temp_name.clone());
                    } else {
                        self.emit(computed.expression);
                    }
                    // Map closing `]` to its source position.
                    // The expression's end points past the expression, so `]`
                    // is at the expression's end position (where the expression
                    // text ends and `]` begins).
                    if self.source_text_for_map().is_some() {
                        let expr_end = self
                            .arena
                            .get(computed.expression)
                            .map_or(node.pos + 1, |e| e.end);
                        self.pending_source_pos = self.fast_source_position(expr_end);
                    }
                    self.write("]");
                }
            }

            // Class members
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_method_declaration(node);
                });
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(node);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_constructor_declaration(node);
                });
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_get_accessor(node, idx);
                });
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_set_accessor(node, idx);
                });
            }
            k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => {
                self.write(";");
            }
            k if k == syntax_kind_ext::DECORATOR => {
                self.emit_decorator(node);
            }

            // Interface/type members (signatures)
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                self.emit_property_signature(node);
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                self.emit_method_signature(node);
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE => {
                // Call signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_call_signature(node);
                }
            }
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                // Construct signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_construct_signature(node);
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_index_signature(node);
                }
            }

            // Template literals
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.emit_tagged_template_expression(node, idx);
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.emit_template_expression(node);
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                self.emit_no_substitution_template(node);
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                self.emit_template_span(node);
            }
            k if k == SyntaxKind::TemplateHead as u16 => {
                self.emit_template_head(node);
            }
            k if k == SyntaxKind::TemplateMiddle as u16 => {
                self.emit_template_middle(node);
            }
            k if k == SyntaxKind::TemplateTail as u16 => {
                self.emit_template_tail(node);
            }

            // Yield/Await/Spread
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                self.emit_yield_expression(node);
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                self.emit_await_expression(node);
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                self.emit_spread_element(node);
            }

            // Source file
            k if k == syntax_kind_ext::SOURCE_FILE => {
                self.emit_source_file(node, idx);
            }

            // Other tokens and keywords - emit their text
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // Check for SubstituteThis directive from lowering pass (Phase C)
                // Directive approach is now the only path (fallback removed)
                if let Some(TransformDirective::SubstituteThis { capture_name }) =
                    self.transforms.get(idx)
                {
                    let name = std::sync::Arc::clone(capture_name);
                    self.write(&name);
                } else if let Some(alias) = self.scoped_static_this_alias.as_ref().cloned() {
                    self.write(&alias);
                } else {
                    self.write("this");
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == SyntaxKind::ImportKeyword as u16 => self.write("import"),

            // Binding patterns (for destructuring)
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                // When emitting as-is (non-ES5 or for parameters), just emit the pattern
                self.emit_object_binding_pattern(node);
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_binding_pattern(node);
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                self.emit_binding_element(node);
            }

            // ExpressionWithTypeArguments / instantiation expression:
            // Strip type arguments and wrap the expression in parentheses.
            // tsc wraps the result in parens when erasing type arguments,
            // e.g. `f<string>` becomes `(f)`.
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.arena.get_expr_type_args(node) {
                    let expression = data.expression;
                    let type_arg_nodes: Vec<NodeIndex> = data
                        .type_arguments
                        .as_ref()
                        .map_or_else(Vec::new, |ta| ta.nodes.clone());
                    self.write("(");
                    self.emit(expression);
                    self.write(")");
                    // Skip comments inside the erased type arguments so they
                    // don't leak into subsequent output.
                    if !self.ctx.options.remove_comments {
                        for ta_idx in &type_arg_nodes {
                            if let Some(ta_node) = self.arena.get(*ta_idx) {
                                self.skip_comments_in_range(ta_node.pos, ta_node.end);
                            }
                        }
                    }
                }
            }

            // Default: do nothing (or handle other cases as needed)
            _ => {}
        }
    }
}

// =============================================================================
// Operator Text Helper
// =============================================================================

pub(crate) use crate::transforms::emit_utils::is_valid_identifier_name;

pub(crate) const fn get_operator_text(op: u16) -> &'static str {
    crate::transforms::emit_utils::operator_to_str(op)
}
