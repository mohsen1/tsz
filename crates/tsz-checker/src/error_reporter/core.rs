//! Core error emission helpers and type formatting utilities.

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn unresolved_unused_renaming_property_in_type_query(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut saw_type_query = false;
        let mut current = idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                saw_type_query = true;
            }

            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::CALL_SIGNATURE
                    | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    | syntax_kind_ext::METHOD_SIGNATURE
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                if !saw_type_query {
                    return None;
                }
                return self.find_renamed_binding_property_for_name(current, name);
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn find_renamed_binding_property_for_name(
        &self,
        root: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(node)
                && binding.property_name.is_some()
                && binding.name.is_some()
                && self.ctx.arena.get_identifier_text(binding.name) == Some(name)
            {
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_text(binding.property_name)
                    .map(str::to_string)?;
                return Some(prop_name);
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }
        None
    }

    pub(super) fn has_more_specific_diagnostic_at_span(&self, start: u32, length: u32) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
    }

    pub(super) fn format_type_for_assignability_message(&mut self, ty: TypeId) -> String {
        let mut formatted = self.format_type(ty);

        // Preserve generic instantiations for nominal class instance names when possible.
        if !formatted.contains('<')
            && let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let symbol_name = symbol.escaped_name.as_str();
            if formatted == symbol_name {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                let type_param_count =
                    if let Some(type_params) = self.ctx.get_def_type_params(def_id) {
                        type_params.len()
                    } else {
                        symbol
                            .declarations
                            .iter()
                            .find_map(|decl| {
                                let node = self.ctx.arena.get(*decl)?;
                                let class = self.ctx.arena.get_class(node)?;
                                Some(class.type_parameters.as_ref().map_or(0, |p| p.nodes.len()))
                            })
                            .unwrap_or(0)
                    };
                if type_param_count > 0 && shape.properties.len() >= type_param_count {
                    let args: Vec<String> = shape
                        .properties
                        .iter()
                        .filter(|prop| {
                            !self
                                .ctx
                                .types
                                .resolve_atom_ref(prop.name)
                                .starts_with("__private_brand_")
                        })
                        .take(type_param_count)
                        .map(|prop| self.format_type(prop.type_id))
                        .collect();
                    if args.len() == type_param_count {
                        formatted = format!("{}<{}>", symbol_name, args.join(", "));
                    }
                }
            }
        }

        // tsc commonly formats object type literals with a trailing semicolon before `}`.
        if formatted.starts_with("{ ")
            && formatted.ends_with(" }")
            && formatted.contains(':')
            && !formatted.ends_with("; }")
        {
            return format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        formatted
    }

    fn format_type_param_for_signature(&mut self, tp: &tsz_solver::TypeParamInfo) -> String {
        let mut part = String::new();
        if tp.is_const {
            part.push_str("const ");
        }
        part.push_str(self.ctx.types.resolve_atom_ref(tp.name).as_ref());
        if let Some(constraint) = tp.constraint {
            part.push_str(" extends ");
            part.push_str(&self.format_type_for_assignability_message(constraint));
        }
        if let Some(default) = tp.default {
            part.push_str(" = ");
            part.push_str(&self.format_type_for_assignability_message(default));
        }
        part
    }

    fn format_signature_text(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        params: &[tsz_solver::ParamInfo],
        return_type: TypeId,
        is_construct: bool,
        arrow: bool,
    ) -> String {
        let mut type_params_text = String::new();
        if !type_params.is_empty() {
            let parts: Vec<String> = type_params
                .iter()
                .map(|tp| self.format_type_param_for_signature(tp))
                .collect();
            type_params_text = format!("<{}>", parts.join(", "));
        }

        let params_text: Vec<String> = params
            .iter()
            .map(|p| {
                let name = p.name.map_or_else(
                    || "_".to_string(),
                    |atom| self.ctx.types.resolve_atom_ref(atom).to_string(),
                );
                let rest = if p.rest { "..." } else { "" };
                let optional = if p.optional { "?" } else { "" };
                let ty = self.format_type_for_assignability_message(p.type_id);
                format!("{rest}{name}{optional}: {ty}")
            })
            .collect();

        let return_text = if is_construct && return_type == TypeId::UNKNOWN {
            "any".to_string()
        } else {
            self.format_type_for_assignability_message(return_type)
        };
        let prefix = if is_construct { "new " } else { "" };

        if arrow {
            format!(
                "{}{}({}) => {}",
                prefix,
                type_params_text,
                params_text.join(", "),
                return_text
            )
        } else {
            format!(
                "{}{}({}): {}",
                prefix,
                type_params_text,
                params_text.join(", "),
                return_text
            )
        }
    }

    fn first_signature_parts(
        &self,
        ty: TypeId,
        wants_construct: bool,
    ) -> Option<(
        Vec<tsz_solver::TypeParamInfo>,
        Vec<tsz_solver::ParamInfo>,
        TypeId,
    )> {
        if let Some(shape) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty) {
            if wants_construct {
                if let Some(sig) = shape.construct_signatures.first() {
                    return Some((sig.type_params.clone(), sig.params.clone(), sig.return_type));
                }
            } else if let Some(sig) = shape.call_signatures.first() {
                return Some((sig.type_params.clone(), sig.params.clone(), sig.return_type));
            }
        }

        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, ty)
            && shape.is_constructor == wants_construct
        {
            return Some((
                shape.type_params.clone(),
                shape.params.clone(),
                shape.return_type,
            ));
        }

        None
    }

    fn is_abstract_constructor_target(&self, ty: TypeId) -> bool {
        let Some(callable) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty)
        else {
            return false;
        };
        if callable.construct_signatures.is_empty() {
            return false;
        }
        let Some(sym_id) = callable.symbol else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        (symbol.flags & tsz_binder::symbol_flags::ABSTRACT) != 0
    }

    pub(super) fn property_visibility_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(tsz_solver::Visibility, tsz_solver::Visibility)> {
        let source_with_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let source_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, source_with_shape)?;
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;
        let source_prop = source_shape
            .properties
            .iter()
            .find(|p| p.name == property_name)?;
        let target_prop = target_shape
            .properties
            .iter()
            .find(|p| p.name == property_name)?;
        Some((source_prop.visibility, target_prop.visibility))
    }

    fn is_function_like_type(&mut self, ty: TypeId) -> bool {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        [ty, resolved, evaluated].into_iter().any(|candidate| {
            tsz_solver::type_queries::get_function_shape(self.ctx.types, candidate).is_some()
                || tsz_solver::type_queries::get_callable_shape(self.ctx.types, candidate)
                    .is_some_and(|s| !s.call_signatures.is_empty())
                || candidate == TypeId::FUNCTION
        })
    }

    pub(super) fn first_nonpublic_constructor_param_property(
        &mut self,
        ty: TypeId,
    ) -> Option<(String, MemberAccessLevel)> {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        let candidates = [ty, resolved, evaluated];

        let mut symbol_candidates: Vec<tsz_binder::SymbolId> = Vec::new();
        if let Some(sym) = candidates.into_iter().find_map(|candidate| {
            tsz_solver::type_queries::get_type_shape_symbol(self.ctx.types, candidate)
        }) {
            symbol_candidates.push(sym);
        }
        let ty_name = self.format_type_for_assignability_message(ty);
        let bare = ty_name.split('<').next().unwrap_or(&ty_name);
        let simple = bare.rsplit('.').next().unwrap_or(bare).trim();
        if !simple.is_empty() && !simple.starts_with('{') && !simple.contains(' ') {
            for sym in self.ctx.binder.get_symbols().find_all_by_name(simple) {
                if !symbol_candidates.contains(&sym) {
                    symbol_candidates.push(sym);
                }
            }
        }
        if symbol_candidates.is_empty() {
            return None;
        }

        for symbol_id in symbol_candidates {
            let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
                continue;
            };
            for &decl_idx in &symbol.declarations {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                let Some(class) = self.ctx.arena.get_class(decl_node) else {
                    continue;
                };
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        let Some(level) = self.member_access_level_from_modifiers(&param.modifiers)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        return Some((name, level));
                    }
                }
            }
        }

        None
    }

    pub(super) fn missing_single_required_property(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_common::interner::Atom> {
        if tsz_solver::is_primitive_type(self.ctx.types, source) {
            return None;
        }

        let source_candidates = {
            let resolved = self.resolve_type_for_property_access(source);
            let evaluated = self.judge_evaluate(resolved);
            [source, resolved, evaluated]
        };
        let target_candidates = {
            let resolved = self.resolve_type_for_property_access(target);
            let evaluated = self.judge_evaluate(resolved);
            [target, resolved, evaluated]
        };

        let source_is_function_like = self.is_function_like_type(source);

        let target_name = self.format_type_for_assignability_message(target);
        if target_name == "Callable" || target_name == "Applicable" {
            let required_name = if target_name == "Callable" {
                "call"
            } else {
                "apply"
            };
            let required_atom = self.ctx.types.intern_string(required_name);
            let source_has_prop = if source_is_function_like {
                true
            } else {
                source_candidates.iter().any(|candidate| {
                    if let Some(source_callable) =
                        tsz_solver::type_queries::get_callable_shape(self.ctx.types, *candidate)
                    {
                        source_callable
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else if let Some(source_shape) =
                        tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate)
                    {
                        source_shape
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else {
                        false
                    }
                })
            };
            if !source_has_prop {
                return Some(required_atom);
            }
        }

        if !source_is_function_like {
            for target_candidate in target_candidates {
                let Some(target_callable) =
                    tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
                else {
                    continue;
                };
                let Some(sym_id) = target_callable.symbol else {
                    continue;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    continue;
                };
                if symbol.escaped_name == "Callable" {
                    return Some(self.ctx.types.intern_string("call"));
                }
                if symbol.escaped_name == "Applicable" {
                    return Some(self.ctx.types.intern_string("apply"));
                }
            }
        }

        for target_candidate in target_candidates {
            if let Some(target_callable) =
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
            {
                let required_props: Vec<_> = target_callable
                    .properties
                    .iter()
                    .filter(|p| !p.optional)
                    .collect();
                if required_props.len() == 1 {
                    let prop = required_props[0];
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "call" || prop_name.as_ref() == "apply" {
                        let source_has_prop = if source_is_function_like {
                            true
                        } else {
                            source_candidates.iter().any(|candidate| {
                                if let Some(source_callable) =
                                    tsz_solver::type_queries::get_callable_shape(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_callable
                                        .properties
                                        .iter()
                                        .any(|p| p.name == prop.name)
                                } else if let Some(source_shape) =
                                    tsz_solver::type_queries::get_object_shape(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_shape.properties.iter().any(|p| p.name == prop.name)
                                } else {
                                    false
                                }
                            })
                        };
                        if !source_has_prop {
                            return Some(prop.name);
                        }
                    }
                }
            }
        }

        let source_with_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };

        let source_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, source_with_shape)?;
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let required_props: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|p| !p.optional)
            .collect();
        if required_props.len() != 1 {
            return None;
        }

        let prop = required_props[0];
        let source_has_prop = source_shape.properties.iter().any(|p| p.name == prop.name);
        if source_has_prop {
            return None;
        }

        let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
        if prop_name.as_ref() == "call" || prop_name.as_ref() == "apply" {
            return Some(prop.name);
        }

        None
    }

    pub(super) fn elaborate_type_mismatch_detail(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if let Some((target_tparams, target_params, target_return)) =
            self.first_signature_parts(target, false)
            && self.first_signature_parts(source, false).is_none()
        {
            let source_str = self.format_type_for_assignability_message(source);
            let target_sig = self.format_signature_text(
                &target_tparams,
                &target_params,
                target_return,
                false,
                false,
            );
            return Some(format_message(
                diagnostic_messages::TYPE_PROVIDES_NO_MATCH_FOR_THE_SIGNATURE,
                &[&source_str, &target_sig],
            ));
        }

        if let Some((target_tparams, target_params, target_return)) =
            self.first_signature_parts(target, true)
        {
            if let Some((source_tparams, source_params, source_return)) =
                self.first_signature_parts(source, true)
            {
                let source_required = source_params
                    .iter()
                    .filter(|p| !p.optional && !p.rest)
                    .count();
                let target_arity = target_params.len();
                if source_required > target_arity {
                    let source_sig = self.format_signature_text(
                        &source_tparams,
                        &source_params,
                        source_return,
                        true,
                        true,
                    );
                    let mut target_sig = self.format_signature_text(
                        &target_tparams,
                        &target_params,
                        target_return,
                        true,
                        true,
                    );
                    if self.is_abstract_constructor_target(target) {
                        target_sig = format!("abstract {target_sig}");
                    }
                    let assignable = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_sig, &target_sig],
                    );
                    let arity = format_message(
                        diagnostic_messages::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT,
                        &[&source_required.to_string(), &target_arity.to_string()],
                    );
                    return Some(format!(
                        "{} {} {}",
                        diagnostic_messages::TYPES_OF_CONSTRUCT_SIGNATURES_ARE_INCOMPATIBLE,
                        assignable,
                        arity
                    ));
                }
            } else {
                let source_str = self.format_type_for_assignability_message(source);
                let mut target_sig = self.format_signature_text(
                    &target_tparams,
                    &target_params,
                    target_return,
                    true,
                    false,
                );
                if self.is_abstract_constructor_target(target) {
                    target_sig = format!("abstract {target_sig}");
                }
                return Some(format_message(
                    diagnostic_messages::TYPE_PROVIDES_NO_MATCH_FOR_THE_SIGNATURE,
                    &[&source_str, &target_sig],
                ));
            }
        }

        let source_with_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let source_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, source_with_shape)?;
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;
        let source_props = source_shape.properties.as_slice();
        let target_props = target_shape.properties.as_slice();

        if target_shape.number_index.is_some() && source_shape.number_index.is_none() {
            let source_str = self.format_type_for_assignability_message(source);
            return Some(format_message(
                diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                &["number", &source_str],
            ));
        }
        if target_shape.string_index.is_some() && source_shape.string_index.is_none() {
            let source_str = self.format_type_for_assignability_message(source);
            return Some(format_message(
                diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                &["string", &source_str],
            ));
        }

        for target_prop in target_props {
            let Some(source_prop) = source_props.iter().find(|p| p.name == target_prop.name) else {
                continue;
            };

            let effective_target_type = if target_prop.optional {
                self.ctx
                    .types
                    .union(vec![target_prop.type_id, TypeId::UNDEFINED])
            } else {
                target_prop.type_id
            };

            if source_prop.visibility != target_prop.visibility {
                let prop_name = self.ctx.types.resolve_atom_ref(target_prop.name);
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
                let detail = match (source_prop.visibility, target_prop.visibility) {
                    (
                        tsz_solver::Visibility::Public | tsz_solver::Visibility::Protected,
                        tsz_solver::Visibility::Private,
                    ) => format_message(
                        diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                        &[&prop_name, &target_str, &source_str],
                    ),
                    (
                        tsz_solver::Visibility::Private,
                        tsz_solver::Visibility::Public | tsz_solver::Visibility::Protected,
                    ) => format_message(
                        diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                        &[&prop_name, &source_str, &target_str],
                    ),
                    (tsz_solver::Visibility::Public, tsz_solver::Visibility::Protected) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &target_str, &source_str],
                        )
                    }
                    (tsz_solver::Visibility::Protected, tsz_solver::Visibility::Public) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &source_str, &target_str],
                        )
                    }
                    _ => continue,
                };
                return Some(detail);
            }

            // Prefer property type incompatibility details when both optionality and type differ.
            // tsc reports the type incompatibility first in this situation.
            if !self.is_assignable_to(source_prop.type_id, target_prop.type_id) {
                let prop_name = self.ctx.types.resolve_atom_ref(target_prop.name);
                let prop_message = format_message(
                    diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    &[&prop_name],
                );
                let source_prop_str =
                    self.format_type_for_assignability_message(source_prop.type_id);
                let target_prop_str =
                    self.format_type_for_assignability_message(target_prop.type_id);
                let nested = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_prop_str, &target_prop_str],
                );
                return Some(format!("{prop_message} {nested}"));
            }

            if source_prop.optional && !target_prop.optional {
                let prop_name = self.ctx.types.resolve_atom_ref(target_prop.name);
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
                return Some(format_message(
                    diagnostic_messages::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop_name, &source_str, &target_str],
                ));
            }

            // Fallback assignability check for optional target properties.
            if !self.is_assignable_to(source_prop.type_id, effective_target_type) {
                let prop_name = self.ctx.types.resolve_atom_ref(target_prop.name);
                let prop_message = format_message(
                    diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    &[&prop_name],
                );
                let source_prop_str =
                    self.format_type_for_assignability_message(source_prop.type_id);
                let target_prop_str =
                    self.format_type_for_assignability_message(effective_target_type);
                let nested = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_prop_str, &target_prop_str],
                );
                return Some(format!("{prop_message} {nested}"));
            }
        }

        None
    }

    /// Prefer statement-level anchors for assignment diagnostics so TS2322 spans
    /// line up with tsc in assignment/variable-declaration contexts.
    pub(super) fn assignment_diagnostic_anchor_idx(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        let mut saw_assignment_binary = false;
        let mut var_decl: Option<NodeIndex> = None;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }

            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                && self.is_assignment_operator(binary.operator_token)
            {
                saw_assignment_binary = true;
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                var_decl = Some(parent);
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT && var_decl.is_some() {
                return parent;
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT && saw_assignment_binary {
                return parent;
            }

            current = parent;
        }

        var_decl.unwrap_or(idx)
    }

    // =========================================================================
    // Fundamental Error Emitters
    // =========================================================================

    /// Report an error at a specific node.
    pub(crate) fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            // Use the error() function which has deduplication by (start, code)
            self.error(start, length, message.to_string(), code);
        }
    }

    /// Emit a templated diagnostic error at a node.
    ///
    /// Looks up the message template for `code` via `get_message_template`,
    /// formats it with `args`, and emits the error at `node_idx`.
    /// Panics in debug mode if the code has no registered template.
    pub(crate) fn error_at_node_msg(&mut self, node_idx: NodeIndex, code: u32, args: &[&str]) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        self.error_at_node(node_idx, &message, code);
    }

    /// Report an error at a specific position.
    pub(crate) fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic {
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: message.to_string(),
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        });
    }

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    pub(crate) fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }
}
