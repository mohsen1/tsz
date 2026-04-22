//! Helper methods for symbol type resolution: circular constraint detection,
//! type parameter identity checks, provisional function types, and numeric enum registration.

use crate::query_boundaries::common::type_param_info;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_indirect_circular_constraints(
        &mut self,
        params: &[tsz_solver::TypeParamInfo],
        param_indices: &[NodeIndex],
    ) {
        // Build a map: param name (Atom) -> index in params list
        let mut name_to_idx: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let param_names: Vec<String> = params
            .iter()
            .map(|p| self.ctx.types.resolve_atom(p.name))
            .collect();
        for (i, name) in param_names.iter().enumerate() {
            name_to_idx.insert(name.clone(), i);
        }

        // For each param, check if its constraint forms an indirect cycle
        for (i, param) in params.iter().enumerate() {
            let Some(constraint_type) = param.constraint else {
                continue;
            };

            // Get the name of the constraint if it's a type parameter
            let constraint_info = type_param_info(self.ctx.types, constraint_type);
            let Some(constraint_info) = constraint_info else {
                continue;
            };
            let constraint_name = self
                .ctx
                .types
                .resolve_atom(constraint_info.name)
                .to_string();

            // Skip direct self-references (already caught)
            if constraint_name == param_names[i] {
                continue;
            }

            // Only follow if constraint is another param in the same list
            let Some(&next_idx) = name_to_idx.get(&constraint_name) else {
                continue;
            };

            // Follow the chain to detect if it cycles back to param i.
            // Only report if the chain leads back to the starting parameter itself,
            // not if it merely reaches some other cycle.
            let mut current = next_idx;
            let mut steps = 0;
            let max_steps = params.len();

            let is_in_cycle = loop {
                if current == i {
                    break true;
                }
                steps += 1;
                if steps > max_steps {
                    break false;
                }

                // Follow the constraint of the current param
                let Some(next_constraint) = params[current].constraint else {
                    break false;
                };
                let next_info = type_param_info(self.ctx.types, next_constraint);
                let Some(next_info) = next_info else {
                    break false;
                };
                let next_name = self.ctx.types.resolve_atom(next_info.name).to_string();
                let Some(&next) = name_to_idx.get(&next_name) else {
                    break false;
                };
                current = next;
            };

            if is_in_cycle {
                let node_idx = param_indices[i];
                if let Some(node) = self.ctx.arena.get(node_idx)
                    && let Some(data) = self.ctx.arena.get_type_parameter(node)
                    && data.constraint != NodeIndex::NONE
                {
                    self.error_at_node_msg(
                        data.constraint,
                        crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                        &[&param_names[i]],
                    );
                }
            }
        }
    }

    /// Check if a constraint type creates a circular constraint for a type parameter.
    ///
    /// This detects:
    /// - Direct self-reference: `T extends T`
    /// - Structural self-reference along the constraint resolution path:
    ///   `T extends { [P in T]: number }`, `T extends Foo | T["hello"]`, etc.
    ///
    /// But NOT safe type-argument references like `T extends Array<T>` or
    /// `S extends Foo<S>`, which are valid in TypeScript.
    pub(crate) fn is_same_type_parameter(
        &self,
        constraint_type: TypeId,
        param_type_id: TypeId,
        param_name: &str,
    ) -> bool {
        // Direct match
        if constraint_type == param_type_id {
            return true;
        }

        // Check if constraint is a TypeParameter with the same name
        if let Some(info) = type_param_info(self.ctx.types, constraint_type) {
            let name_str = self.ctx.types.resolve_atom(info.name);
            if name_str == param_name {
                return true;
            }
        }

        // Check if constraint references the type parameter along the base-constraint
        // resolution path (e.g., mapped type key source, union/intersection members,
        // conditional types, index access). This catches cases like
        // `T extends { [P in T]: number }` without false-positiving on `T extends Array<T>`.
        //
        let atom = self.ctx.types.intern_string(param_name);
        // For mapped type constraints, only consider it circular if the type parameter
        // appears directly in the key source (e.g., `[P in T]` is circular), not when it
        // appears through `keyof` (e.g., `[K in keyof T]` is valid).
        // `T extends { [K in keyof T]: T[K] }` is a common valid TypeScript pattern.
        if let Some(mapped) = crate::query_boundaries::property_access::get_mapped_type(
            self.ctx.types,
            constraint_type,
        ) {
            let key_source = mapped.constraint;
            // Check if the key source directly contains the type parameter without
            // going through a `keyof` wrapper. Strip keyof from the key source first,
            // then check if the remainder still references the type parameter.
            let key_without_keyof =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, key_source)
                    .map(|_| {
                        // The key is `keyof X` or `keyof X & Y`. T only appears inside
                        // keyof, which is a valid non-circular reference.
                        false
                    })
                    .unwrap_or_else(|| {
                        // Key is not wrapped in keyof. Check for intersection like
                        // `keyof T & string` - strip intersection members that are keyof.
                        if let Some(members_id) =
                            crate::query_boundaries::common::intersection_list_id(
                                self.ctx.types,
                                key_source,
                            )
                        {
                            let members = self.ctx.types.type_list(members_id);
                            // Check if T only appears inside keyof members of the intersection
                            members.iter().any(|&member| {
                                crate::query_boundaries::common::keyof_inner_type(
                                    self.ctx.types,
                                    member,
                                )
                                .is_none()
                                    && crate::query_boundaries::common::contains_type_parameter_named_shallow(
                                        self.ctx.types,
                                        member,
                                        atom,
                                    )
                            })
                        } else {
                            // Not keyof, not intersection - check directly
                            crate::query_boundaries::common::contains_type_parameter_named_shallow(
                                self.ctx.types,
                                key_source,
                                atom,
                            )
                        }
                    });
            return key_without_keyof;
        }
        crate::query_boundaries::common::constraint_references_type_param_in_resolution_path(
            self.ctx.types,
            constraint_type,
            atom,
        )
    }

    pub(crate) fn provisional_circular_function_symbol_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        use tsz_solver::{CallableShape, FunctionShape};

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::FUNCTION)
            || symbol.has_any_flags(symbol_flags::INTERFACE)
        {
            return None;
        }

        let declarations = symbol.declarations.clone();
        let factory = self.ctx.types.factory();
        let mut overloads = Vec::new();
        let mut implementation_sig = None;

        for decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };

            let sig = if self.ctx.is_declaration_file() {
                self.provisional_declaration_file_call_signature(func)
            } else {
                self.call_signature_from_function(func, decl_idx)
            };
            if func.body.is_none() {
                overloads.push(sig);
            } else if implementation_sig.is_none() {
                implementation_sig = Some(sig);
            }
        }

        if !overloads.is_empty() {
            return Some(factory.callable(CallableShape {
                call_signatures: overloads,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            }));
        }

        let sig = implementation_sig?;
        let func_type = factory.function(FunctionShape {
            type_params: sig.type_params,
            params: sig.params,
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: false,
        });
        Some(func_type)
    }

    fn provisional_declaration_file_call_signature(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> tsz_solver::CallSignature {
        let mut params = Vec::with_capacity(func.parameters.nodes.len());

        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let param_name = self.parameter_name_for_error(param.name);
            let name = self.ctx.types.intern_string(&param_name);
            params.push(tsz_solver::ParamInfo {
                name: Some(name),
                type_id: TypeId::ANY,
                optional: param.question_token || param.initializer.is_some(),
                rest: param.dot_dot_dot_token,
            });
        }

        tsz_solver::CallSignature {
            type_params: Vec::new(),
            params,
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: false,
        }
    }

    /// Check if a symbol is a numeric enum and register it in the `TypeEnvironment`.
    ///
    /// This is used for Rule #7 (Open Numeric Enums) where number types are
    /// assignable to/from numeric enums.
    pub(crate) fn maybe_register_numeric_enum(
        &self,
        env: &mut tsz_solver::TypeEnvironment,
        sym_id: SymbolId,
        def_id: tsz_solver::def::DefId,
    ) {
        // Check if the symbol is an enum
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if !symbol.has_any_flags(symbol_flags::ENUM) {
            return;
        }

        // Get the enum declaration to check if it's numeric
        let Some(decl_idx) = symbol.primary_declaration() else {
            return;
        };

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // Check enum members to determine if it's numeric
        let mut saw_string = false;
        let mut saw_numeric = false;

        for &member_idx in &enum_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            if member.initializer.is_some() {
                let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                    continue;
                };
                match init_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => saw_string = true,
                    k if k == SyntaxKind::NumericLiteral as u16 => saw_numeric = true,
                    _ => {}
                }
            } else {
                // Members without initializers are auto-incremented numbers
                saw_numeric = true;
            }
        }

        // Register as numeric enum if it's numeric (not string-only)
        if saw_numeric && !saw_string {
            env.register_numeric_enum(def_id);
        }
    }
}
