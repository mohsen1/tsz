//! Property-inspection probes used by diagnostic anchoring, split out of
//! `core_formatting.rs`.
//!
//! These two helpers answer *structural* questions about a type — which
//! constructor parameter property is non-public, and whether exactly one
//! required property is missing — rather than formatting anything. They live
//! beside the formatters because the anchor/message code is their only caller,
//! but they are not display logic, and moving them keeps the
//! `core_formatting.rs` shard under the 2000-line file-size cap (§19).
//!
//! Moved verbatim; `use super::*`-style imports are spelled explicitly so the
//! relocation is behavior-preserving.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    fn is_function_like_type(&mut self, ty: TypeId) -> bool {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        [ty, resolved, evaluated].into_iter().any(|candidate| {
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, candidate)
                .is_some()
                || crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    candidate,
                )
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
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, candidate)
        }) {
            symbol_candidates.push(sym);
        }
        let ty_name = self.format_type_for_assignability_message(ty);
        let bare = ty_name.split('<').next().unwrap_or(&ty_name);
        let simple = bare.rsplit('.').next().unwrap_or(bare).trim();
        if !simple.is_empty() && !simple.starts_with('{') && !simple.contains(' ') {
            for &sym in self.ctx.binder.get_symbols().find_all_by_name(simple) {
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
        if crate::query_boundaries::common::is_primitive_type(self.ctx.types, source) {
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

        for target_candidate in target_candidates {
            if let Some(target_callable) = crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                target_candidate,
            ) {
                let required_props: Vec<_> = target_callable
                    .properties
                    .iter()
                    .filter(|p| !p.optional)
                    .collect();
                if required_props.len() == 1 {
                    let prop = required_props[0];
                    let source_has_prop = if source_is_function_like {
                        true
                    } else {
                        source_candidates.iter().any(|candidate| {
                            if let Some(source_callable) =
                                crate::query_boundaries::common::callable_shape_for_type(
                                    self.ctx.types,
                                    *candidate,
                                )
                            {
                                crate::query_boundaries::common::find_matching_property(
                                    &source_callable.properties,
                                    prop.name,
                                )
                                .is_some()
                            } else if let Some(source_shape) =
                                crate::query_boundaries::common::object_shape_for_type(
                                    self.ctx.types,
                                    *candidate,
                                )
                            {
                                crate::query_boundaries::common::find_matching_property(
                                    &source_shape.properties,
                                    prop.name,
                                )
                                .is_some()
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

        // Reuse the already-resolved candidate arrays (`[direct, resolved,
        // evaluated]`) rather than recomputing the resolve/evaluate pipeline.
        let source_with_shape = source_candidates.into_iter().find(|candidate| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                .is_some()
        })?;
        let target_with_shape = target_candidates.into_iter().find(|candidate| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                .is_some()
        })?;

        let source_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            source_with_shape,
        )?;
        let target_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            target_with_shape,
        )?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let missing_required_props: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|p| !p.optional)
            .filter(|prop| !source_shape.properties.iter().any(|p| p.name == prop.name))
            .collect();
        if missing_required_props.len() != 1 {
            return None;
        }

        Some(missing_required_props[0].name)
    }
}
