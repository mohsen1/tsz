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

    /// Recover one actual type argument from a property whose declared type is
    /// a bare type parameter (or an array/`Array<T>` wrapper around one).
    ///
    /// Returns the declared index of the matched type parameter together with
    /// the instantiated type that occupies it: when `value: T` is declared and
    /// the instance's `value` property has type `number`, the application's
    /// actual argument for `T` IS `number`. This is a sound projection of the
    /// actual arguments — unlike harvesting arbitrary member types, which
    /// fabricates an argument list unrelated to the instantiation.
    pub(super) fn declared_property_type_arg_candidate_for_display(
        &self,
        symbol: &tsz_binder::Symbol,
        property_name: Atom,
        actual_type: TypeId,
        type_param_names: &[Atom],
    ) -> Option<(usize, TypeId)> {
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

    /// Recover actual type arguments from declared call/construct signatures
    /// whose parameter (or return) annotations are bare type parameters.
    ///
    /// Only fills slots when the declared signature count matches the shape's
    /// instantiated signature count (so positions correspond); each candidate
    /// is placed at the matched parameter's declared index. Like the property
    /// pass, this is a sound projection of the actual arguments, never a
    /// harvest of arbitrary signature types.
    pub(super) fn fill_signature_type_arg_slots_for_display(
        &self,
        symbol: &tsz_binder::Symbol,
        ty: TypeId,
        type_param_names: &[Atom],
        slots: &mut [Option<TypeId>],
        conflict: &mut bool,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{CALL_SIGNATURE, CONSTRUCT_SIGNATURE};

        let Some(callable) =
            crate::query_boundaries::diagnostics::callable_shape_for_type(self.ctx.types, ty)
        else {
            return;
        };
        let arena = self.symbol_declaration_arena(symbol);

        let mut declared_calls: Vec<&tsz_parser::parser::node::SignatureData> = Vec::new();
        let mut declared_constructs: Vec<&tsz_parser::parser::node::SignatureData> = Vec::new();
        for decl in &symbol.declarations {
            let Some(node) = arena.get(*decl) else {
                continue;
            };
            let Some(members) = arena
                .get_class(node)
                .map(|class| &class.members)
                .or_else(|| {
                    arena
                        .get_interface(node)
                        .map(|interface| &interface.members)
                })
            else {
                continue;
            };
            for member_idx in &members.nodes {
                let Some(member_node) = arena.get(*member_idx) else {
                    continue;
                };
                let Some(sig) = arena.get_signature(member_node) else {
                    continue;
                };
                match member_node.kind {
                    CALL_SIGNATURE => declared_calls.push(sig),
                    CONSTRUCT_SIGNATURE => declared_constructs.push(sig),
                    _ => {}
                }
            }
        }

        let mut record = |index: usize, candidate: TypeId, conflict: &mut bool| match slots[index] {
            None => slots[index] = Some(candidate),
            Some(existing) if existing == candidate => {}
            Some(_) => *conflict = true,
        };

        let mut fill_from_pairs = |declared: &[&tsz_parser::parser::node::SignatureData],
                                   instantiated: &[tsz_solver::CallSignature],
                                   conflict: &mut bool| {
            if declared.is_empty() || declared.len() != instantiated.len() {
                return;
            }
            for (decl_sig, inst_sig) in declared.iter().zip(instantiated.iter()) {
                // Signature-local type parameters shadow the owner's; skip
                // generic signatures so a same-named inner parameter never
                // masquerades as the owner's argument.
                if decl_sig.type_parameters.is_some() {
                    continue;
                }
                if let Some(params) = &decl_sig.parameters {
                    for (param_pos, param_idx) in params.nodes.iter().enumerate() {
                        let Some(param_node) = arena.get(*param_idx) else {
                            continue;
                        };
                        let Some(param) = arena.get_parameter(param_node) else {
                            continue;
                        };
                        let Some(inst_param) = inst_sig.params.get(param_pos) else {
                            continue;
                        };
                        if let Some((index, candidate)) = self
                            .declared_type_arg_candidate_for_display(
                                arena,
                                param.type_annotation,
                                inst_param.type_id,
                                type_param_names,
                            )
                        {
                            record(index, candidate, conflict);
                        }
                    }
                }
                if let Some((index, candidate)) = self.declared_type_arg_candidate_for_display(
                    arena,
                    decl_sig.type_annotation,
                    inst_sig.return_type,
                    type_param_names,
                ) {
                    record(index, candidate, conflict);
                }
            }
        };

        fill_from_pairs(&declared_calls, &callable.call_signatures, conflict);
        fill_from_pairs(
            &declared_constructs,
            &callable.construct_signatures,
            conflict,
        );
    }

    fn symbol_declaration_arena<'b>(&'b self, symbol: &tsz_binder::Symbol) -> &'b NodeArena {
        if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            self.ctx.arena
        }
    }

    /// When `type_node` is a bare reference to one of `type_param_names`,
    /// return that parameter's declared index.
    fn display_type_param_index(
        &self,
        arena: &NodeArena,
        type_node: tsz_parser::parser::NodeIndex,
        type_param_names: &[Atom],
    ) -> Option<usize> {
        let name = arena
            .get_type_ref_at(type_node)
            .filter(|type_ref| type_ref.type_arguments.is_none())
            .and_then(|type_ref| arena.get_identifier_at(type_ref.type_name))
            .map(|ident| self.ctx.types.intern_string(&ident.escaped_text))?;
        type_param_names.iter().position(|param| *param == name)
    }

    fn declared_type_arg_candidate_for_display(
        &self,
        arena: &NodeArena,
        declared_type: tsz_parser::parser::NodeIndex,
        actual_type: TypeId,
        type_param_names: &[Atom],
    ) -> Option<(usize, TypeId)> {
        if declared_type.is_none() {
            return None;
        }
        if let Some(index) = self.display_type_param_index(arena, declared_type, type_param_names) {
            return Some((index, actual_type));
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
