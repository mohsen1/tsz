//! Helpers for TS2344 cases involving utility mapped type constraints.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

const MAX_CONSTRAINT_BASE_RECURSION: usize = 128;

impl<'a> CheckerState<'a> {
    pub(super) fn type_node_is_generic_ref_with_scoped_type_param_arg(
        &self,
        arg_idx: NodeIndex,
    ) -> bool {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(arg_node) else {
            return false;
        };
        let Some(type_args) = &type_ref.type_arguments else {
            return false;
        };
        type_args
            .nodes
            .iter()
            .copied()
            .any(|node_idx| self.type_node_contains_scoped_type_parameter(node_idx))
    }

    fn type_node_contains_scoped_type_parameter(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .ctx
                .type_parameter_scope
                .contains_key(&identifier.escaped_text)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_scoped_type_parameter(child_idx))
    }

    /// Substitute every in-scope type parameter in `type_id` with its declared
    /// constraint (or `unknown` when the parameter is unconstrained). The
    /// result is used to give a concrete instantiation of a generic-reference
    /// type argument so that `is_assignable_to(concrete, target_constraint)`
    /// can be evaluated without ambiguity. (#3063)
    pub(super) fn scoped_type_param_substituted_form(&self, type_id: TypeId) -> TypeId {
        if self.ctx.type_parameter_scope.is_empty() {
            return type_id;
        }
        let db = self.ctx.types.as_type_database();
        let mut subst = tsz_solver::TypeSubstitution::new();
        for (name, &scope_type_id) in &self.ctx.type_parameter_scope {
            let bound =
                crate::query_boundaries::common::type_parameter_constraint(db, scope_type_id)
                    .unwrap_or(TypeId::UNKNOWN);
            let bound = if bound == scope_type_id {
                TypeId::UNKNOWN
            } else {
                bound
            };
            let atom = self.ctx.types.intern_string(name);
            subst.insert(atom, bound);
        }
        if subst.is_empty() {
            return type_id;
        }
        crate::query_boundaries::common::instantiate_type(self.ctx.types, type_id, &subst)
    }

    pub(super) fn instantiate_constraint_for_type_args(
        &self,
        constraint: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args: &[TypeId],
    ) -> TypeId {
        let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
        for (param, &arg) in type_params.iter().zip(type_args.iter()) {
            subst.insert(param.name, arg);
        }
        if subst.is_empty() {
            constraint
        } else {
            crate::query_boundaries::common::instantiate_type(self.ctx.types, constraint, &subst)
        }
    }

    pub(super) fn conditional_true_type_parameter_base_satisfies_constraint(
        &mut self,
        cond_check: TypeId,
        cond_true: TypeId,
        constraint: TypeId,
    ) -> bool {
        if cond_true != cond_check
            || !query::is_bare_type_parameter(self.ctx.types.as_type_database(), cond_true)
        {
            return false;
        }

        let true_base = self.constraint_check_base_type(cond_true);
        if matches!(true_base, TypeId::UNKNOWN | TypeId::ERROR) || true_base == cond_true {
            return false;
        }
        if true_base == constraint {
            return true;
        }

        let true_base_resolved = self.resolve_lazy_type(true_base);
        let constraint_resolved = self.resolve_lazy_type(constraint);
        if true_base_resolved == constraint_resolved {
            return true;
        }

        let true_base_evaluated = self.evaluate_type_for_assignability(true_base_resolved);
        let constraint_evaluated = self.evaluate_type_for_assignability(constraint_resolved);
        true_base_evaluated == constraint_evaluated
            || self.is_assignable_to(true_base_evaluated, constraint_evaluated)
            || self.is_assignable_to(true_base_resolved, constraint_resolved)
    }

    pub(super) fn constraint_check_base_type(&mut self, type_id: TypeId) -> TypeId {
        let mut seen = FxHashSet::default();
        self.constraint_check_base_type_inner(type_id, &mut seen)
    }

    fn constraint_check_base_type_inner(
        &mut self,
        type_id: TypeId,
        seen: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if seen.len() >= MAX_CONSTRAINT_BASE_RECURSION {
            return TypeId::UNKNOWN;
        }
        if !seen.insert(type_id) {
            return TypeId::UNKNOWN;
        }

        let evaluated = self.evaluate_type_for_assignability(type_id);
        let result = if evaluated != type_id {
            self.constraint_check_base_type_inner(evaluated, seen)
        } else {
            let db = self.ctx.types.as_type_database();
            // For TypeParameter: returns constraint or UNKNOWN; for non-TypeParameter: returns type_id
            let base = query::base_constraint_of_type(db, type_id);
            if base == TypeId::UNKNOWN
                && query::is_bare_type_parameter(db, type_id)
                && let Some(name_atom) = query::type_parameter_name(db, type_id)
            {
                let name = self.ctx.types.resolve_atom(name_atom);
                if let Some(&scoped_type_id) = self.ctx.type_parameter_scope.get(&name)
                    && scoped_type_id != type_id
                {
                    let scoped_base = query::base_constraint_of_type(db, scoped_type_id);
                    if scoped_base != TypeId::UNKNOWN && scoped_base != scoped_type_id {
                        let result = self.constraint_check_base_type_inner(scoped_base, seen);
                        seen.remove(&type_id);
                        return result;
                    }
                }
            }
            if base != type_id {
                let base = self.evaluate_type_for_assignability(base);
                if let Some(keyof_operand) = query::keyof_operand(db, base) {
                    // Only normalize `keyof X` when X is a fully concrete type. When
                    // X is itself a (free) type parameter, `get_keyof_type` would
                    // resolve through X's constraint and return a concrete union of
                    // the constraint's keys (e.g., `keyof T` for `T extends unknown[]`
                    // becomes `number | "length" | "concat" | ...`). That breaks the
                    // upstream `contains_free_type_parameters(base)` deferral, causing
                    // false TS2344 on patterns like `{ [K in keyof T]: F<K> }`.
                    // Keeping `keyof X` deferred lets the caller defer the constraint
                    // check to instantiation time, matching tsc.
                    if !query::contains_free_type_parameters(self.ctx.types, keyof_operand) {
                        let normalized = self.get_keyof_type(keyof_operand);
                        if normalized != self.ctx.types.keyof(keyof_operand) {
                            seen.remove(&type_id);
                            return normalized;
                        }
                    }
                }
                base
            } else if let Some((object_type, index_type)) =
                query::index_access_components(db, type_id)
            {
                let constrained_object_type = if query::is_bare_type_parameter(
                    self.ctx.types.as_type_database(),
                    object_type,
                ) {
                    self.constraint_check_base_type_inner(object_type, seen)
                } else {
                    object_type
                };
                let constrained_index_type =
                    self.constraint_check_base_type_inner(index_type, seen);
                let resolved_object_type = if constrained_object_type == TypeId::UNKNOWN {
                    object_type
                } else {
                    constrained_object_type
                };
                let resolved_index_type = if constrained_index_type == TypeId::UNKNOWN {
                    index_type
                } else {
                    constrained_index_type
                };
                if let Some(indexed_value_type) = self.constraint_check_indexed_access_value_type(
                    resolved_object_type,
                    resolved_index_type,
                ) {
                    self.evaluate_type_for_assignability(indexed_value_type)
                } else if resolved_object_type == object_type && resolved_index_type == index_type {
                    type_id
                } else {
                    let constrained_access = self
                        .ctx
                        .types
                        .index_access(resolved_object_type, resolved_index_type);
                    self.evaluate_type_for_assignability(constrained_access)
                }
            } else {
                type_id
            }
        };

        seen.remove(&type_id);
        result
    }

    pub(super) fn required_mapped_constraint_source_is_required_and_arg_satisfies(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        substitutions: &[(tsz_common::Atom, TypeId)],
    ) -> bool {
        let Some(source) = self.required_mapped_constraint_source(constraint) else {
            return false;
        };
        let source = self.substitute_required_mapped_source(source, substitutions);

        let source = self.resolve_lazy_type(source);
        self.ensure_relation_input_ready(source);
        let source = self.evaluate_type_with_resolution(source);
        let tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } =
            tsz_solver::objects::collect_properties(source, self.ctx.types, &self.ctx)
        else {
            return false;
        };
        if properties.is_empty() || properties.iter().any(|prop| prop.optional) {
            return false;
        }

        let type_arg_resolved = self.resolve_lazy_type(type_arg);
        self.ensure_relation_input_ready(type_arg_resolved);
        let type_arg_evaluated = self.evaluate_type_with_resolution(type_arg_resolved);
        type_arg_evaluated == source
            || self.is_assignable_to(type_arg_evaluated, source)
            || self.type_satisfies_required_source_properties(type_arg_resolved, &properties)
            || (type_arg_evaluated != type_arg_resolved
                && self.type_satisfies_required_source_properties(type_arg_evaluated, &properties))
            || self.type_literal_alias_satisfies_required_source(type_arg_resolved, source)
    }

    fn required_mapped_constraint_source(&self, constraint: TypeId) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        if let Some(mapped) = crate::query_boundaries::common::mapped_type_info(db, constraint)
            && mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove)
        {
            return crate::query_boundaries::common::homomorphic_mapped_source(db, constraint);
        }

        let (Some(base_def), args) = query::application_base_def_and_args(db, constraint)? else {
            return None;
        };
        if args.len() != 1 {
            return None;
        }
        let sym_id = self.ctx.def_to_symbol_id(base_def)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        // The shortcut treats `Required<Source>` as the lib's mapped utility
        // and skips the constraint check by comparing the type argument
        // against the source itself. A *user-defined* `type Required<T> = …`
        // with a different shape must NOT trigger the shortcut, otherwise
        // the constraint check is silently skipped (#3061). Gate on the
        // symbol coming from a lib file so user redeclarations fall through
        // to the regular constraint check.
        if symbol.escaped_name != "Required" || !self.ctx.symbol_is_from_lib(sym_id) {
            return None;
        }
        Some(args[0])
    }

    fn substitute_required_mapped_source(
        &self,
        source: TypeId,
        substitutions: &[(tsz_common::Atom, TypeId)],
    ) -> TypeId {
        let db = self.ctx.types.as_type_database();
        let Some(name) = query::type_parameter_name(db, source) else {
            return source;
        };
        substitutions
            .iter()
            .find_map(|&(param_name, arg)| (param_name == name).then_some(arg))
            .unwrap_or(source)
    }

    fn type_satisfies_required_source_properties(
        &mut self,
        type_arg: TypeId,
        source_properties: &[tsz_solver::PropertyInfo],
    ) -> bool {
        let tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } =
            tsz_solver::objects::collect_properties(type_arg, self.ctx.types, &self.ctx)
        else {
            return false;
        };
        for source_prop in source_properties {
            let Some(arg_prop) = properties.iter().find(|prop| prop.name == source_prop.name)
            else {
                return false;
            };
            if arg_prop.optional {
                return false;
            }
            if arg_prop.type_id != source_prop.type_id {
                let arg_type = self.evaluate_type_for_assignability(arg_prop.type_id);
                let source_type = self.evaluate_type_for_assignability(source_prop.type_id);
                if !self.is_assignable_to(arg_type, source_type) {
                    return false;
                }
            }
        }
        true
    }

    fn type_literal_alias_satisfies_required_source(
        &mut self,
        type_arg: TypeId,
        source: TypeId,
    ) -> bool {
        let Some(source_props) = self.type_literal_alias_property_nodes(source) else {
            return false;
        };
        if source_props.is_empty() || source_props.iter().any(|(_, _, optional)| *optional) {
            return false;
        }
        let Some(arg_props) = self.type_literal_alias_property_nodes(type_arg) else {
            return false;
        };

        for (source_name, source_type_node, _) in source_props {
            let Some((_, arg_type_node, arg_optional)) = arg_props
                .iter()
                .find(|(arg_name, _, _)| arg_name == &source_name)
                .cloned()
            else {
                return false;
            };
            if arg_optional {
                return false;
            }

            let arg_type = self.get_type_from_type_node(arg_type_node);
            let source_type = self.get_type_from_type_node(source_type_node);
            if arg_type != source_type && !self.is_assignable_to(arg_type, source_type) {
                return false;
            }
        }

        true
    }

    fn type_literal_alias_property_nodes(
        &self,
        type_id: TypeId,
    ) -> Option<Vec<(String, NodeIndex, bool)>> {
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(alias.type_node) else {
                continue;
            };
            let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                continue;
            };

            let mut props = Vec::new();
            for &member_idx in &type_lit.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                if signature.type_annotation == NodeIndex::NONE {
                    continue;
                }
                let Some(name) = self.ctx.arena.identifier_text_owned(signature.name) else {
                    continue;
                };
                props.push((name, signature.type_annotation, signature.question_token));
            }
            return Some(props);
        }
        None
    }
}
