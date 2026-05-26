//! Helper routines for class instance type construction.

use crate::query_boundaries::class_type::{callable_shape_for_type, object_shape_for_type};
use crate::query_boundaries::common::is_plain_object_type;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallableShape, IndexSignature, ObjectShape, TypeId, TypeParamInfo};

/// Bookkeeping record for a single type parameter pushed into
/// `type_parameter_scope`: the parameter name, its previous binding in that
/// scope (so `pop_type_parameters` can restore it), and a flag indicating
/// whether the push shadowed an enclosing class's type parameter (so the pop
/// can restore the class scope entry too).
type ScopeUpdate = (String, Option<TypeId>, bool);

pub(in crate::types_domain) const fn can_skip_base_instantiation(
    base_type_param_count: usize,
    explicit_type_arg_count: usize,
) -> bool {
    base_type_param_count == 0 && explicit_type_arg_count == 0
}

pub(super) const fn exceeds_class_inheritance_depth_limit(depth: usize) -> bool {
    // Keep well above realistic inheritance chains while bounding pathological recursion.
    depth > 256
}

pub(super) fn in_progress_class_instance_result(
    in_resolution_set: bool,
    cached: Option<TypeId>,
) -> Option<TypeId> {
    if in_resolution_set {
        Some(cached.unwrap_or(TypeId::ERROR))
    } else {
        None
    }
}

pub(super) fn declaration_is_module_augmentation(
    arena: &tsz_parser::parser::NodeArena,
    decl_idx: NodeIndex,
) -> bool {
    let mut current = Some(decl_idx);
    while let Some(node_idx) = current {
        let Some(ext) = arena.get_extended(node_idx) else {
            break;
        };
        if ext.parent.is_none() {
            break;
        }
        let parent_idx = ext.parent;
        let Some(parent_node) = arena.get(parent_idx) else {
            break;
        };
        if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module_decl) = arena.get_module(parent_node)
            && let Some(name_node) = arena.get(module_decl.name)
        {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
            if name_node.kind == SyntaxKind::GlobalKeyword as u16 {
                return false;
            }
            if let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == "global"
            {
                return false;
            }
        }
        current = Some(parent_idx);
    }
    false
}

impl<'a> CheckerState<'a> {
    pub(super) fn class_member_name_is_symbol_named(&mut self, name_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(name_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            && self.is_symbol_property_name(name_idx)
    }

    pub(super) fn class_declaration_symbol(
        &self,
        class_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let arena_ptr = self.ctx.arena as *const _ as usize;
        self.ctx
            .cross_file_node_symbols_for_arena(self.ctx.binder, arena_ptr)
            .and_then(|node_symbols| node_symbols.get(&class_idx.0).copied())
            .or_else(|| self.ctx.binder.get_node_symbol(class_idx))
    }

    /// Check if a method body syntactically returns only `this`.
    /// Returns true if every return statement in the body has `this` as
    /// its expression (or the body is an expression-bodied arrow returning `this`).
    pub(super) fn method_body_returns_only_this(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if body_node.kind != syntax_kind_ext::BLOCK {
            return false;
        }
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return false;
        };
        let mut found_return = false;
        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT
                && let Some(return_data) = self.ctx.arena.get_return_statement(stmt_node)
            {
                if return_data.expression.is_none() {
                    continue;
                }
                let Some(expr_node) = self.ctx.arena.get(return_data.expression) else {
                    return false;
                };
                if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
                    return false;
                }
                found_return = true;
            }
        }
        found_return
    }

    pub(super) fn merge_class_instance_with_interface(
        &mut self,
        instance_type: TypeId,
        interface_type: TypeId,
    ) -> TypeId {
        let factory = self.ctx.types.factory();

        let mut properties = FxHashMap::default();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;
        let mut symbol = None;
        let mut result_is_callable = false;

        let mut merge_shape = |type_id: TypeId, is_derived_class: bool| {
            if let Some(shape) = callable_shape_for_type(self.ctx.types, type_id) {
                result_is_callable = true;
                if is_derived_class {
                    symbol = shape.symbol;
                    string_index = shape.string_index;
                    number_index = shape.number_index;
                } else {
                    if string_index.is_none() {
                        string_index = shape.string_index;
                    }
                    if number_index.is_none() {
                        number_index = shape.number_index;
                    }
                }
                call_signatures.extend(shape.call_signatures.iter().cloned());
                construct_signatures.extend(shape.construct_signatures.iter().cloned());
                for prop in &shape.properties {
                    properties.entry(prop.name).or_insert_with(|| prop.clone());
                }
                return;
            }

            if let Some(shape) = object_shape_for_type(self.ctx.types, type_id) {
                if is_derived_class {
                    symbol = shape.symbol;
                    string_index = shape.string_index;
                    number_index = shape.number_index;
                } else {
                    if string_index.is_none() {
                        string_index = shape.string_index;
                    }
                    if number_index.is_none() {
                        number_index = shape.number_index;
                    }
                }
                for prop in &shape.properties {
                    properties.entry(prop.name).or_insert_with(|| prop.clone());
                }
            }
        };

        merge_shape(instance_type, true);
        merge_shape(interface_type, false);

        if result_is_callable {
            return factory.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties: properties.into_values().collect(),
                string_index,
                number_index,
                symbol,
                is_abstract: false,
            });
        }

        let shape = ObjectShape {
            properties: properties.into_values().collect(),
            string_index,
            number_index,
            symbol,
            ..ObjectShape::default()
        };

        if is_plain_object_type(self.ctx.types, instance_type)
            && string_index.is_none()
            && number_index.is_none()
        {
            factory.object(shape.properties)
        } else {
            factory.object_with_index(shape)
        }
    }

    pub(super) fn merge_union_index_signature(
        &self,
        target: &mut Option<IndexSignature>,
        incoming: IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type {
                existing.value_type = self
                    .ctx
                    .types
                    .factory()
                    .union2(existing.value_type, incoming.value_type);
            }
            existing.readonly &= incoming.readonly;
        } else {
            *target = Some(incoming);
        }
    }

    pub(super) fn merge_index_signature_from_unresolved_computed_name(
        &mut self,
        name_idx: NodeIndex,
        value_type: TypeId,
        string_index: &mut Option<IndexSignature>,
        number_index: &mut Option<IndexSignature>,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let key_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev;

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(key_type) {
            if wants_string {
                self.merge_union_index_signature(
                    string_index,
                    IndexSignature {
                        key_type: TypeId::STRING,
                        value_type,
                        readonly: false,
                        param_name: None,
                    },
                );
            }
            if wants_number {
                self.merge_union_index_signature(
                    number_index,
                    IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                        param_name: None,
                    },
                );
            }
        }
    }

    /// For JS classes without syntax-level type parameters, check the leading
    /// JSDoc for `@template` tags and create type parameters from them.
    ///
    /// Returns `(type_params, scope_updates)` with the same shape as `push_type_parameters`.
    /// The caller must pass `scope_updates` to `pop_type_parameters` when done.
    pub(in crate::types_domain) fn push_jsdoc_class_template_type_params(
        &mut self,
        class_idx: NodeIndex,
    ) -> (Vec<TypeParamInfo>, Vec<ScopeUpdate>) {
        if !self.is_js_file() {
            return (Vec::new(), Vec::new());
        }

        let jsdoc = {
            let sf = match self.ctx.arena.source_files.first() {
                Some(sf) => sf,
                None => return (Vec::new(), Vec::new()),
            };
            let source_text: &str = &sf.text;
            let comments = &sf.comments;
            match self.try_leading_jsdoc(
                comments,
                self.ctx.arena.get(class_idx).map_or(0, |n| n.pos),
                source_text,
            ) {
                Some(j) => j,
                None => return (Vec::new(), Vec::new()),
            }
        };

        self.validate_jsdoc_template_tag_syntax_at_decl(class_idx);

        let template_names = Self::jsdoc_template_type_params(&jsdoc);
        if template_names.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut type_params = Vec::with_capacity(template_names.len());
        let mut scope_updates = Vec::with_capacity(template_names.len());
        let factory = self.ctx.types.factory();
        let constraint_strs = Self::jsdoc_template_constraint_strings(&jsdoc);
        for (name, is_const, default_str) in template_names {
            let atom = self.ctx.types.intern_string(&name);
            let default = default_str
                .as_deref()
                .and_then(|s| self.resolve_jsdoc_reference(s));
            let constraint = constraint_strs
                .get(&name)
                .and_then(|s| self.resolve_jsdoc_reference(s));
            let info = TypeParamInfo {
                name: atom,
                constraint,
                default,
                is_const,
            };
            let ty = factory.type_param(info);
            type_params.push(info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
            scope_updates.push((name, previous, false));
        }
        (type_params, scope_updates)
    }

    pub(super) fn register_final_class_instance_type(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        instance_type: TypeId,
        class_type_params: &[TypeParamInfo],
    ) {
        let is_class_symbol = self
            .get_symbol_globally(sym_id)
            .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
        if !is_class_symbol {
            return;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        self.ctx
            .definition_store
            .register_type_to_def(instance_type, def_id);
        self.ctx
            .register_class_instance_in_envs(def_id, instance_type);
        self.ctx
            .register_resolved_type(sym_id, instance_type, class_type_params.to_vec());
        if !class_type_params.is_empty() {
            self.get_type_params_for_symbol(sym_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_skip_base_instantiation, exceeds_class_inheritance_depth_limit,
        in_progress_class_instance_result,
    };
    use tsz_solver::TypeId;

    #[test]
    fn skip_base_instantiation_only_without_generics() {
        assert!(can_skip_base_instantiation(0, 0));
        assert!(!can_skip_base_instantiation(1, 0));
        assert!(!can_skip_base_instantiation(0, 1));
        assert!(!can_skip_base_instantiation(2, 3));
    }

    #[test]
    fn class_inheritance_depth_guard_is_conservative() {
        assert!(!exceeds_class_inheritance_depth_limit(1));
        assert!(!exceeds_class_inheritance_depth_limit(100));
        assert!(!exceeds_class_inheritance_depth_limit(256));
        assert!(exceeds_class_inheritance_depth_limit(257));
    }

    #[test]
    fn in_progress_class_instance_uses_cached_or_error() {
        assert_eq!(
            in_progress_class_instance_result(true, Some(TypeId(42))),
            Some(TypeId(42))
        );
        assert_eq!(
            in_progress_class_instance_result(true, None),
            Some(TypeId::ERROR)
        );
        assert_eq!(in_progress_class_instance_result(false, None), None);
    }
}
