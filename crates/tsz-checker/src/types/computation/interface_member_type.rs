//! Interface-member type computation for `CheckerState`.
//!
//! `get_type_of_interface_member[_simple]` and `merged_method_signature_type`.
//! Extracted from `types/computation/helpers.rs` to keep it under the 2000-LOC
//! cap; behavior is unchanged (inherent `CheckerState` methods, same crate).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{TypeId, Visibility};

impl<'a> CheckerState<'a> {
    /// Get the simple type of an interface member (without wrapping in object type).
    ///
    /// For property signatures: returns the property type
    /// For method signatures: returns the function type
    pub(crate) fn get_type_of_interface_member_simple(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::FunctionShape;
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        if member_node.kind == METHOD_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            let (type_params, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
            let (params, this_type) = self.extract_params_from_signature_in_type_literal(sig);
            let (return_type, type_predicate) = self.return_type_and_predicate_in_type_literal(
                sig.type_annotation,
                &params,
                crate::signature_builder::signature_param_nodes(&sig.parameters),
            );

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: true,
            };
            self.pop_type_parameters(type_param_updates);
            return factory.function(shape);
        }

        if member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            if sig.type_annotation.is_some() {
                let base = self.get_type_from_type_node_in_type_literal(sig.type_annotation);
                let evaluated = self.evaluate_type_with_env(base);
                let base = if evaluated != TypeId::ERROR && evaluated != TypeId::UNKNOWN {
                    let has_members = crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        evaluated,
                    )
                    .is_some_and(|shape| !shape.properties.is_empty());
                    if has_members { evaluated } else { base }
                } else {
                    base
                };
                // Optional property signatures carry an implicit `| undefined`
                // in their type. The sibling helper `get_type_of_interface_member`
                // preserves this via `PropertyInfo.optional`; this "simple"
                // variant (used by cross-arena / cross-file delegation) was
                // dropping `?:` so `configuration.server?: IServer` flowed
                // across `import = require(...)` aliases as plain `IServer`.
                if sig.question_token {
                    return factory.union(vec![base, TypeId::UNDEFINED]);
                }
                return base;
            }
            return TypeId::ANY;
        }

        TypeId::ANY
    }

    /// Prefer the merged method type (with all overloads) from `iface_type`
    /// over the single-node `get_type_of_interface_member_simple` result.
    ///
    /// For `interface I { bar(): any; bar(): any; [s: string]: number; }`
    /// this returns the Callable `{ (): any; (): any; }` that tsc displays
    /// in TS2411, instead of just `() => any` from the first signature.
    pub(crate) fn merged_method_signature_type(
        &mut self,
        iface_type: TypeId,
        name: &str,
        member_idx: NodeIndex,
    ) -> TypeId {
        if let tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } =
            self.resolve_property_access_with_env(iface_type, name)
        {
            return type_id;
        }
        self.get_type_of_interface_member_simple(member_idx)
    }

    /// Get the type of an interface member.
    ///
    /// Returns an object type containing the member. For method signatures,
    /// creates a callable type. For property signatures, creates a property type.
    pub(crate) fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::{FunctionShape, PropertyInfo};
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR;
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR;
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR;
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) = self.return_type_and_predicate(
                    sig.type_annotation,
                    &params,
                    crate::signature_builder::signature_param_nodes(&sig.parameters),
                );

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = factory.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                    non_widening: false,
                };
                return factory.object(vec![prop]);
            }

            let type_id = if sig.type_annotation.is_some() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
                non_widening: false,
            };
            return factory.object(vec![prop]);
        }

        TypeId::ANY
    }
}
