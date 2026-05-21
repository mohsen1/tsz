//! Helpers for recovering generic type arguments in diagnostic display.

use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn symbol_type_param_names_for_display(
        &self,
        symbol: &tsz_binder::Symbol,
    ) -> Vec<Atom> {
        let arena = self.symbol_declaration_arena(symbol);
        symbol
            .declarations
            .iter()
            .find_map(|decl| {
                let node = arena.get(*decl)?;
                let params = arena
                    .get_class(node)
                    .and_then(|class| class.type_parameters.as_ref())
                    .or_else(|| {
                        arena
                            .get_interface(node)
                            .and_then(|interface| interface.type_parameters.as_ref())
                    })?;
                Some(
                    params
                        .nodes
                        .iter()
                        .filter_map(|param_idx| {
                            let param = arena.get_type_parameter_at(*param_idx)?;
                            let ident = arena.get_identifier_at(param.name)?;
                            Some(self.ctx.types.intern_string(&ident.escaped_text))
                        })
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    pub(super) fn declared_property_type_arg_candidate_for_display(
        &self,
        symbol: &tsz_binder::Symbol,
        property_name: Atom,
        actual_type: TypeId,
        type_param_names: &[Atom],
    ) -> Option<TypeId> {
        let arena = self.symbol_declaration_arena(symbol);
        for decl in &symbol.declarations {
            let Some(node) = arena.get(*decl) else {
                continue;
            };
            let members = arena
                .get_class(node)
                .map(|class| &class.members)
                .or_else(|| {
                    arena
                        .get_interface(node)
                        .map(|interface| &interface.members)
                });
            let Some(members) = members else {
                continue;
            };

            for member_idx in &members.nodes {
                let Some(member_node) = arena.get(*member_idx) else {
                    continue;
                };
                if let Some(prop) = arena.get_property_decl(member_node)
                    && let Some(ident) = arena.get_identifier_at(prop.name)
                    && self.ctx.types.intern_string(&ident.escaped_text) == property_name
                    && let Some(candidate) = self.declared_type_arg_candidate_for_display(
                        arena,
                        prop.type_annotation,
                        actual_type,
                        type_param_names,
                    )
                {
                    return Some(candidate);
                }
                if let Some(sig) = arena.get_signature(member_node)
                    && let Some(ident) = arena.get_identifier_at(sig.name)
                    && self.ctx.types.intern_string(&ident.escaped_text) == property_name
                    && let Some(candidate) = self.declared_type_arg_candidate_for_display(
                        arena,
                        sig.type_annotation,
                        actual_type,
                        type_param_names,
                    )
                {
                    return Some(candidate);
                }
            }
        }

        None
    }

    pub(super) fn signature_type_arg_display_candidates(&self, ty: TypeId) -> Vec<TypeId> {
        let is_substantive = |type_id: TypeId| {
            !matches!(
                type_id,
                TypeId::VOID
                    | TypeId::NEVER
                    | TypeId::ANY
                    | TypeId::UNKNOWN
                    | TypeId::UNDEFINED
                    | TypeId::NULL
            )
        };
        let mut candidates = Vec::new();
        if let Some(callable) =
            crate::query_boundaries::diagnostics::callable_shape_for_type(self.ctx.types, ty)
        {
            for sig in callable
                .call_signatures
                .iter()
                .chain(callable.construct_signatures.iter())
            {
                candidates.extend(
                    sig.params
                        .iter()
                        .map(|param| param.type_id)
                        .filter(|type_id| is_substantive(*type_id)),
                );
                if is_substantive(sig.return_type) {
                    candidates.push(sig.return_type);
                }
            }
        }
        candidates
    }

    fn symbol_declaration_arena<'b>(&'b self, symbol: &tsz_binder::Symbol) -> &'b NodeArena {
        if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            self.ctx.arena
        }
    }

    fn type_node_is_display_type_param(
        &self,
        arena: &NodeArena,
        type_node: tsz_parser::parser::NodeIndex,
        type_param_names: &[Atom],
    ) -> bool {
        arena
            .get_type_ref_at(type_node)
            .filter(|type_ref| type_ref.type_arguments.is_none())
            .and_then(|type_ref| arena.get_identifier_at(type_ref.type_name))
            .map(|ident| self.ctx.types.intern_string(&ident.escaped_text))
            .is_some_and(|name| type_param_names.contains(&name))
    }

    fn declared_type_arg_candidate_for_display(
        &self,
        arena: &NodeArena,
        declared_type: tsz_parser::parser::NodeIndex,
        actual_type: TypeId,
        type_param_names: &[Atom],
    ) -> Option<TypeId> {
        if declared_type.is_none() {
            return None;
        }
        if self.type_node_is_display_type_param(arena, declared_type, type_param_names) {
            return Some(actual_type);
        }

        let node = arena.get(declared_type)?;
        if let Some(array) = arena.get_array_type(node) {
            let element_type = crate::query_boundaries::diagnostics::array_element_type(
                self.ctx.types,
                actual_type,
            )?;
            return self.declared_type_arg_candidate_for_display(
                arena,
                array.element_type,
                element_type,
                type_param_names,
            );
        }

        if let Some(type_ref) = arena.get_type_ref(node)
            && let Some(type_args) = &type_ref.type_arguments
            && type_args.nodes.len() == 1
            && let Some(ident) = arena.get_identifier_at(type_ref.type_name)
            && matches!(ident.escaped_text.as_str(), "Array" | "ReadonlyArray")
        {
            let element_type = crate::query_boundaries::diagnostics::array_element_type(
                self.ctx.types,
                actual_type,
            )?;
            return self.declared_type_arg_candidate_for_display(
                arena,
                type_args.nodes[0],
                element_type,
                type_param_names,
            );
        }

        None
    }
}
