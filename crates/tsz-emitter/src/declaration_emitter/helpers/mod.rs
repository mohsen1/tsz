//! Declaration emitter - expression/node emission, import management, and utility helpers.
//!
//! Type syntax emission (type references, unions, mapped types, etc.) is in `type_emission.rs`.

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::source_map::escape_js_string;
use tsz_parser::parser::NodeIndex;

/// Escape a cooked string value for embedding in a double-quoted string literal.
///
/// Delegates to `tsz_common::source_map::escape_js_string` so there is one
/// canonical escape implementation in the workspace.
pub(crate) fn escape_string_for_double_quote(s: &str) -> String {
    escape_js_string(s, '"')
}

/// Escape a cooked string value for embedding in a single-quoted string literal.
///
/// Delegates to `tsz_common::source_map::escape_js_string` so there is one
/// canonical escape implementation in the workspace.
pub(crate) fn escape_string_for_single_quote(s: &str) -> String {
    escape_js_string(s, '\'')
}

type JsFoldedNamedExports = (
    FxHashSet<String>,
    FxHashMap<NodeIndex, Vec<NodeIndex>>,
    FxHashSet<NodeIndex>,
);
#[derive(Clone)]
pub(crate) struct JsNamespaceExportAlias {
    pub(crate) export_name: String,
    pub(crate) local_name: String,
    pub(crate) use_import_alias: bool,
    pub(crate) source_statements: Vec<NodeIndex>,
    pub(crate) has_non_statement_origin: bool,
}
type JsNamespaceExportAliases = FxHashMap<String, Vec<JsNamespaceExportAlias>>;
type JsCommonjsSyntheticStatements = FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>;
type JsCommonjsNamedExports = (
    FxHashSet<String>,
    JsCommonjsSyntheticStatements,
    JsCommonjsSyntheticStatements,
);

#[derive(Clone, Copy)]
pub(in crate::declaration_emitter) enum JsCommonjsExpandoDeclKind {
    Function,
    Value,
    PrototypeMethod,
}

#[derive(Default)]
pub(crate) struct JsCommonjsExpandoDeclarations {
    pub(crate) function_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) value_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) prototype_methods: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
}

#[derive(Clone)]
pub(crate) struct JsStaticMethodAugmentationGroup {
    pub(crate) class_idx: NodeIndex,
    pub(crate) method_idx: NodeIndex,
    pub(crate) class_is_exported: bool,
    pub(crate) properties: Vec<(NodeIndex, NodeIndex)>,
}

#[derive(Default)]
pub(crate) struct JsStaticMethodAugmentations {
    pub(crate) statements: FxHashMap<NodeIndex, JsStaticMethodAugmentationGroup>,
    pub(crate) skipped_statements: FxHashSet<NodeIndex>,
    pub(crate) augmented_method_nodes: FxHashSet<NodeIndex>,
}

/// Collected prototype member assignments for JS class-like heuristic variables.
/// e.g. `let A; A.prototype.b = {};` → variable `A` becomes `declare class A { ... }`.
#[derive(Default)]
pub(crate) struct JsClassLikePrototypeMembers {
    /// Maps variable name → list of (`member_name_idx`, `initializer_idx`) pairs.
    pub(crate) members: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// Statement indices consumed by the class-like heuristic (to skip during normal emit).
    pub(crate) consumed_stmts: FxHashSet<NodeIndex>,
}

#[derive(Default)]
pub(crate) struct JsClassStaticMembers {
    pub(crate) members: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    pub(crate) consumed_stmts: FxHashSet<NodeIndex>,
}

#[derive(Clone)]
pub(crate) struct JsClassDefinePropertyAccessor {
    pub(crate) property_name: String,
    pub(crate) getter: Option<NodeIndex>,
    pub(crate) setter: Option<JsClassDefinePropertySetter>,
}

#[derive(Clone, Copy)]
pub(crate) struct JsClassDefinePropertySetter {
    pub(crate) initializer: NodeIndex,
    pub(crate) preserve_param_name: bool,
}

type JsStaticMethodKey = (String, String);
type JsStaticMethodInfo = (NodeIndex, NodeIndex, bool);
type JsStaticMethodAugmentationEntry = (
    NodeIndex,
    NodeIndex,
    NodeIndex,
    bool,
    Vec<(NodeIndex, NodeIndex)>,
);

#[derive(Clone)]
pub(in crate::declaration_emitter) struct JsdocTypeAliasDecl {
    pub(in crate::declaration_emitter) name: String,
    pub(in crate::declaration_emitter) type_params: Vec<String>,
    pub(in crate::declaration_emitter) type_text: String,
    pub(in crate::declaration_emitter) description_lines: Vec<String>,
    pub(in crate::declaration_emitter) render_verbatim: bool,
}

pub(in crate::declaration_emitter) struct JsDefinedPropertyDecl {
    pub(in crate::declaration_emitter) name: String,
    pub(in crate::declaration_emitter) type_text: String,
    pub(in crate::declaration_emitter) readonly: bool,
    pub(in crate::declaration_emitter) value: NodeIndex,
}

#[derive(Clone)]
pub(crate) struct LateBoundAssignmentMember {
    pub(crate) property_name_text: String,
    pub(crate) namespace_member_name: Option<String>,
    pub(crate) type_text: String,
}

#[derive(Clone)]
pub(crate) struct JsdocParamDecl {
    pub(crate) name: String,
    pub(crate) type_text: String,
    pub(crate) optional: bool,
    /// True only for the JSDoc optional-type marker form (`{T=}`), which tsc
    /// serializes as `T | undefined` on declaration surfaces. The bracketed
    /// name form (`[name]`) sets `optional` alone and never adds `undefined`
    /// to the printed type.
    pub(crate) optional_type_marker: bool,
    pub(crate) rest: bool,
}

#[derive(Clone)]
pub(crate) struct JsdocOverloadSignature {
    pub(crate) comment: String,
    pub(crate) type_params: Vec<String>,
    pub(crate) params: Vec<JsdocParamDecl>,
    pub(crate) return_type: String,
}

/// Lightweight `TypeResolver` backed by `TypeCacheView` data for DTS emit.
pub(crate) struct DtsCacheResolver<'a> {
    pub(crate) cache: &'a crate::type_cache_view::TypeCacheView,
}

/// Resolver used only when declaration emit needs a structural answer from the
/// solver, not a printable named surface. It resolves every cached `DefId` body,
/// including object/interface lazies, so `keyof` and remapped-key evaluation can
/// reduce concrete aliases to finite literal sets.
pub(crate) struct DtsStructuralResolver<'a> {
    pub(crate) cache: &'a crate::type_cache_view::TypeCacheView,
}

impl tsz_solver::def::resolver::TypeResolver for DtsCacheResolver<'_> {
    fn resolve_ref(
        &self,
        _symbol: tsz_solver::types::SymbolRef,
        _interner: &dyn tsz_solver::construction::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        None
    }

    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        interner: &dyn tsz_solver::construction::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        let &type_id = self.cache.def_types.get(&def_id.0)?;
        tsz_solver::type_queries::lazy_body_resolves_for_declaration_display(interner, type_id)
            .then_some(type_id)
    }

    fn get_lazy_type_params(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<Vec<tsz_solver::types::TypeParamInfo>> {
        self.cache.def_type_params.get(&def_id.0).cloned()
    }

    fn resolve_well_known_symbol_name(&self, name: &str) -> Option<tsz_solver::types::SymbolRef> {
        self.cache.well_known_symbol_names.get(name).copied()
    }
}

impl tsz_solver::def::resolver::TypeResolver for DtsStructuralResolver<'_> {
    fn resolve_ref(
        &self,
        _symbol: tsz_solver::types::SymbolRef,
        _interner: &dyn tsz_solver::construction::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        None
    }

    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        _interner: &dyn tsz_solver::construction::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        self.cache.def_types.get(&def_id.0).copied()
    }

    fn get_lazy_type_params(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<Vec<tsz_solver::types::TypeParamInfo>> {
        self.cache.def_type_params.get(&def_id.0).cloned()
    }

    fn resolve_well_known_symbol_name(&self, name: &str) -> Option<tsz_solver::types::SymbolRef> {
        self.cache.well_known_symbol_names.get(name).copied()
    }
}

mod comments_source;
mod computed_declarations;
mod correlated_union;
mod correlated_union_mapped_arrays;
mod default_import_alias_rewrite;
mod dts_export_text_scan;
mod emit_node;
mod function_analysis;
mod generic_call_literal;
mod generic_call_mapped_inference;
mod generic_call_no_infer;
mod generic_call_variadic_surface;
mod generic_call_variadic_tuple;
mod js_exports;
mod js_exports_local;
mod js_exports_namespace;
mod jsdoc;
mod jsdoc_function_signature;
mod late_bound_function_analysis;
mod literal_initializers;
mod local_asserted_type_alias;
mod portability_check;
#[cfg(test)]
pub(in crate::declaration_emitter) use portability_check::PortabilityVisitState;
mod portability_export_paths;
mod portability_resolve;
mod portability_symbols;
mod returned_function_initializer;
mod returned_function_initializer_return;
mod synthetic_dependencies;
mod synthetic_public_api_dependencies;
mod type_inference;
mod type_inference_accessor_property;
mod type_inference_class_expression;
mod type_inference_const_assertions;
mod type_inference_contextual_callbacks;
mod type_inference_declared_call;
mod type_inference_enum_access;
mod type_inference_expression_literals;
mod type_inference_fallback_types;
mod type_inference_flat_map;
mod type_inference_foreign_names;
mod type_inference_function_text;
mod type_inference_generator_yield;
mod type_inference_imported_calls;
mod type_inference_imported_indexed_access;
mod type_inference_instantiation;
mod type_inference_object_members;
mod type_inference_object_rewrites;
mod type_inference_object_unions;
mod type_inference_package_matching;
mod type_inference_parameter_return;
mod type_inference_portable_mapped_objects;
mod type_inference_public_packages;
mod type_inference_return_async;
mod type_inference_return_guards;
mod type_inference_return_indexed_source;
mod type_inference_return_normalization;
mod type_inference_return_surface;
mod type_inference_return_unions;
mod type_inference_source_call;
mod type_inference_source_callables;
mod type_inference_source_object_args;
mod type_inference_source_text;
mod type_inference_truncation_expansion;
mod type_inference_ts7_union_order;
mod type_inference_type_annotations;
mod type_inference_type_nodes;
mod type_literal_accessor_names;
mod type_param_rewrite;
mod type_predicate_text;
mod type_printing;
mod type_printing_paths;
mod type_printing_undefined;
mod unexported_alias_literal;
mod variable_decl;
mod variable_decl_function_initializers;
mod variable_decl_type_helpers;
mod visibility;
