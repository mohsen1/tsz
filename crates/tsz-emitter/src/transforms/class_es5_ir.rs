#[path = "class_es5_ast_to_ir.rs"]
pub mod ast_to_ir;

pub use ast_to_ir::AstToIr;

#[path = "class_es5_ir_auto_accessor.rs"]
mod auto_accessor;

#[path = "class_es5_ir_constructor.rs"]
mod constructor;

#[path = "class_es5_ir_decorators.rs"]
mod decorators;

#[path = "class_es5_ir_helpers.rs"]
mod helpers;

#[path = "class_es5_ir_members.rs"]
mod members;

use helpers::*;

use crate::context::transform::TransformContext;

use crate::transforms::async_es5_ir::AsyncES5Transformer;

use crate::transforms::ir::{
    IRCatchClause, IRNode, IRParam, IRProperty, IRPropertyKey, IRPropertyKind, IRSwitchCase,
};

use crate::transforms::ir_printer::IRPrinter;

use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, PrivateMethodInfo,
    collect_enclosing_source_binding_names, collect_private_accessors_with_reserved,
    collect_private_fields_with_reserved, collect_private_methods_with_reserved,
    make_unique_private_name, private_helper_base,
};

use rustc_hash::{FxHashMap, FxHashSet};

use std::cell::{Cell, RefCell};

use tsz_common::common::ModuleKind;

use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_parser::syntax::transform_utils::contains_this_reference;

use tsz_scanner::SyntaxKind;

#[derive(Clone)]
pub(super) struct Tc39Es5MemberDecorator {
    member_idx: NodeIndex,
    decorators_var: String,
    decorator_exprs: Vec<String>,
    kind: &'static str,
    name: Tc39Es5MemberName,
    is_static: bool,
    initializers_var: Option<String>,
    extra_initializers_var: Option<String>,
}

impl Tc39Es5MemberDecorator {
    const fn is_field(&self) -> bool {
        self.initializers_var.is_some()
    }
}

#[derive(Clone)]
enum Tc39Es5MemberName {
    Identifier(String),
    StringLiteral(String),
    Computed { expr_text: String, key_var: String },
}

struct Tc39Es5ComputedMemberInjection {
    kind: &'static str,
    is_static: bool,
    expr_text: String,
    assignments: Vec<String>,
    decorator_vars: Vec<String>,
}

fn tc39_es5_propkey_temp_name(offset: u32) -> String {
    let idx = offset + 1;
    if idx < 26 {
        format!("_{}", (b'a' + idx as u8) as char)
    } else {
        format!("_{idx}")
    }
}

/// Context for ES5 class transformation
pub struct ES5ClassTransformer<'a> {
    arena: &'a NodeArena,
    class_name: String,
    has_extends: bool,
    extends_null: bool,
    super_name: String,
    private_fields: Vec<PrivateFieldInfo>,
    private_accessors: Vec<PrivateAccessorInfo>,
    private_methods: Vec<PrivateMethodInfo>,
    private_instances_weakset_name: Option<String>,
    auto_accessors: Vec<AutoAccessorFieldInfo>,
    /// Transform directives from `LoweringPass`
    transforms: Option<TransformContext>,
    /// Source text for extracting comments
    source_text: Option<&'a str>,
    /// Class-level decorator `NodeIndex` list (for legacy decorator lowering)
    class_decorators: Vec<NodeIndex>,
    /// Whether to emit member decorator __decorate calls inside the IIFE
    legacy_decorators: bool,
    /// Whether to emit `__metadata` calls in `__decorate` arrays
    emit_decorator_metadata: bool,
    /// Whether to emit TC39 decorator helper calls for ES5 output.
    tc39_decorators: bool,
    /// Whether the current TC39-decorated class needs instance extra initializers.
    tc39_has_instance_member_decorators: bool,
    /// TC39 member decorator metadata for the class currently being transformed.
    tc39_es5_member_decorators: Vec<Tc39Es5MemberDecorator>,
    /// Base indent level for raw IR strings (0 for top-level, 1+ for nested contexts)
    indent_base: u32,
    /// Counter for generating unique temp variable names (_a, _b, _c, ...)
    temp_var_counter: Cell<u32>,
    /// Mapping from computed property name expression `NodeIndex` to temp variable name.
    computed_prop_temp_map: std::collections::HashMap<NodeIndex, String>,
    /// Alias used for `this` in static property initializers/static blocks for the current class.
    current_static_class_alias: Option<String>,
    /// Alias used for class-name self references when class decorators can replace the binding.
    class_self_reference_alias: Option<String>,
    /// Whether a nested class heritage expression is evaluated in a pre-super
    /// constructor receiver capture context.
    extends_this_captured: bool,
    /// Whether static field initializer assignments are emitted by the surrounding expression emitter.
    skip_static_field_initializers: bool,
    use_define_for_class_fields: bool,
    /// When true, prefix helper names like `__decorate` with the tslib import binding.
    tslib_prefix: bool,
    /// The tslib import binding name (e.g. `tslib_1`) used when `tslib_prefix` is true.
    tslib_import_binding: String,
    commonjs_import_substitutions: FxHashMap<String, String>,
    module_kind: ModuleKind,
    target_es5: bool,
    downlevel_iteration: bool,
    dynamic_import_promise_counter: Cell<u32>,
    async_generator_inner_name_counts: RefCell<FxHashMap<String, u32>>,
    disposable_env_counter: Cell<u32>,
    blocked_disposable_env_names: RefCell<FxHashSet<String>>,
    generated_disposable_env_names: RefCell<Vec<String>>,
    /// Additional hoisted temp variable names collected from expression conversions
    /// (e.g., from computed property lowering inside object literals)
    extra_hoisted_temps: RefCell<Vec<String>>,
    /// When true, computed-prop-name temps are placed in the `ES5ClassIIFE`
    /// `computed_prop_temp_decls` / `computed_prop_temp_inits` fields instead
    /// of the IIFE body.  Set for class-expression contexts where the caller
    /// owns the hoisting and needs the comma-expression pattern.
    emit_computed_props_outside: Cell<bool>,
    /// Outer block-scope rename map passed from the enclosing printer so that
    /// identifier references in class property initializers use the renamed form
    /// when an outer `let`/`const` was renamed during ES5 lowering.
    outer_rename_map: FxHashMap<String, String>,
    /// Super name of an enclosing *instance* member when this class is lowered
    /// inside that member's body. A computed property name in such a nested
    /// class is evaluated inside the enclosing instance method, so a `super`
    /// reference in the name binds to the outer class's prototype home and must
    /// lower to `<super>.prototype.m.call(this)` rather than the default
    /// static-context `<super>.m`. `None` for a top-level/static definition
    /// site, where computed names keep static-like super access.
    inherited_computed_name_super: Option<String>,
    /// Raw expression used for `this` in computed property names when this
    /// class expression is evaluated inside an enclosing static initializer.
    inherited_computed_name_this: Option<String>,
}

include!("class_es5_ir_parts/part1.rs");
include!("class_es5_ir_parts/part2.rs");

fn es5_temp_name(index: u32) -> String {
    if index < 26 {
        format!("_{}", (b'a' + index as u8) as char)
    } else {
        format!("_{}", index - 26)
    }
}

#[cfg(test)]
#[path = "../../tests/class_es5_ir.rs"]
mod tests;
