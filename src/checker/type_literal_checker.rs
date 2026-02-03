//! Type Literal Checking Module
//!
//! This module contains type literal type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Type resolution within type literals
//! - Type reference resolution in type literals
//! - Parameter extraction from type literal signatures
//! - Full type literal node type computation
//!
//! Type literals represent inline object types like `{ x: string; y: number }` or
//! callable types with call/construct signatures.

use crate::checker::state::{CheckerState, ParamTypeResolutionMode};
use crate::checker::symbol_resolver::TypeSymbolResolution;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::TypeId;

// =============================================================================
// Type Literal Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Node Resolution in Type Literals
    // =========================================================================

    /// Get type from a type node within a type literal context.
    ///
    /// This handles special resolution needed for types declared within
    /// type literals, such as recursive type references.
    pub(crate) fn get_type_from_type_node_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self.get_type_from_type_reference_in_type_literal(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            return self.get_type_from_type_query(idx);
        }
        if node.kind == syntax_kind_ext::UNION_TYPE {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                let members = composite
                    .types
                    .nodes
                    .iter()
                    .map(|&member_idx| self.get_type_from_type_node_in_type_literal(member_idx))
                    .collect::<Vec<_>>();
                return self.ctx.types.union(members);
            }
            return TypeId::ERROR;
        }
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                let elem_type =
                    self.get_type_from_type_node_in_type_literal(array_type.element_type);
                return self.ctx.types.array(elem_type);
            }
            return TypeId::ERROR; // Missing array type data - propagate error
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR {
            // Handle readonly and other type operators in type literals
            return self.get_type_from_type_operator(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.get_type_from_type_literal(idx);
        }

        self.get_type_from_type_node(idx)
    }

    fn get_type_from_type_reference_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::solver::TypeKey;

        // Phase 4.3: Migration to TypeKey::Lazy(DefId) is complete for this file.
        // Type references now use create_lazy_type_ref() instead of TypeKey::Ref(SymbolRef).

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type reference data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && name_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(sym_id) => sym_id,
                TypeSymbolResolution::ValueOnly(_) => {
                    let name = self
                        .entity_name_text(type_name_idx)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    self.error_value_only_type_at(&name, type_name_idx);
                    return TypeId::ERROR;
                }
                TypeSymbolResolution::NotFound => {
                    let _ = self.resolve_qualified_name(type_name_idx);
                    return TypeId::ERROR;
                }
            };
            // Phase 4.3: Use Lazy(DefId) instead of Ref(SymbolRef)
            let base_type = self.ctx.create_lazy_type_ref(sym_id);
            if has_type_args {
                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return self.ctx.types.application(base_type, type_args);
            }
            return base_type;
        }

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.as_str();

            if has_type_args {
                let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                let type_param = self.lookup_type_parameter(name);
                let type_resolution =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx);
                let sym_id = match type_resolution {
                    TypeSymbolResolution::Type(sym_id) => Some(sym_id),
                    TypeSymbolResolution::ValueOnly(_) => {
                        self.error_value_only_type_at(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => None,
                };

                if is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    // Array/ReadonlyArray not found - check if lib files are loaded
                    // When --noLib is used, emit TS2318 instead of silently creating Array type
                    if !self.ctx.has_lib_loaded() {
                        // No lib files loaded - emit TS2318 for missing global type
                        self.error_cannot_find_global_type(name, type_name_idx);
                        // Still process type arguments to avoid cascading errors
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        return TypeId::ERROR;
                    }
                    // Lib files are loaded but Array not found - fall back to creating Array type
                    let elem_type = type_ref
                        .type_arguments
                        .as_ref()
                        .and_then(|args| args.nodes.first().copied())
                        .map(|idx| self.get_type_from_type_node_in_type_literal(idx))
                        .unwrap_or(TypeId::UNKNOWN);
                    let array_type = self.ctx.types.array(elem_type);
                    if name == "ReadonlyArray" {
                        return self.ctx.types.intern(TypeKey::ReadonlyType(array_type));
                    }
                    return array_type;
                }

                if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    if self.is_known_global_type_name(name) {
                        // TS2318/TS2583: Emit error for missing global type
                        // Process type arguments for validation first
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        // Emit the appropriate error
                        self.error_cannot_find_global_type(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if name == "await" {
                        self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_name_idx) {
                        return TypeId::ANY;
                    }
                    self.error_cannot_find_name_at(name, type_name_idx);
                    return TypeId::ERROR;
                }
                let base_type = if let Some(type_param) = type_param {
                    type_param
                } else if let Some(sym_id) = sym_id {
                    // Phase 4.3: Use Lazy(DefId) instead of Ref(SymbolRef)
                    self.ctx.create_lazy_type_ref(sym_id)
                } else {
                    TypeId::ERROR
                };

                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return self.ctx.types.application(base_type, type_args);
            }

            if name == "Array" || name == "ReadonlyArray" {
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
                {
                    // Phase 4.3: Use Lazy(DefId) instead of Ref(SymbolRef)
                    return self.ctx.create_lazy_type_ref(sym_id);
                }
                if let Some(type_param) = self.lookup_type_parameter(name) {
                    return type_param;
                }
                // Array/ReadonlyArray not found - check if lib files are loaded
                // When --noLib is used, emit TS2318 instead of silently creating Array type
                if !self.ctx.has_lib_loaded() {
                    // No lib files loaded - emit TS2318 for missing global type
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                // Lib files are loaded but Array not found - fall back to creating Array type
                let elem_type = type_ref
                    .type_arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first().copied())
                    .map(|idx| self.get_type_from_type_node_in_type_literal(idx))
                    .unwrap_or(TypeId::UNKNOWN);
                let array_type = self.ctx.types.array(elem_type);
                if name == "ReadonlyArray" {
                    return self.ctx.types.intern(TypeKey::ReadonlyType(array_type));
                }
                return array_type;
            }

            match name {
                "number" => return TypeId::NUMBER,
                "string" => return TypeId::STRING,
                "boolean" => return TypeId::BOOLEAN,
                "void" => return TypeId::VOID,
                "any" => return TypeId::ANY,
                "never" => return TypeId::NEVER,
                "unknown" => return TypeId::UNKNOWN,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "object" => return TypeId::OBJECT,
                "bigint" => return TypeId::BIGINT,
                "symbol" => return TypeId::SYMBOL,
                _ => {}
            }

            if name != "Array" {
                if let TypeSymbolResolution::ValueOnly(_) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
                {
                    self.error_value_only_type_at(name, type_name_idx);
                    return TypeId::ERROR;
                }
            }

            if let Some(type_param) = self.lookup_type_parameter(name) {
                return type_param;
            }
            if let TypeSymbolResolution::Type(sym_id) =
                self.resolve_identifier_symbol_in_type_position(type_name_idx)
            {
                // Phase 4.3: Use Lazy(DefId) instead of Ref(SymbolRef)
                return self.ctx.create_lazy_type_ref(sym_id);
            }

            if name == "await" {
                self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                return TypeId::ERROR;
            }
            if self.is_known_global_type_name(name) {
                // TS2318/TS2583: Emit error for missing global type
                self.error_cannot_find_global_type(name, type_name_idx);
                return TypeId::ERROR;
            }
            // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
            if self.is_unresolved_import_symbol(type_name_idx) {
                return TypeId::ANY;
            }
            self.error_cannot_find_name_at(name, type_name_idx);
            return TypeId::ERROR;
        }

        TypeId::ANY
    }

    // =========================================================================
    // Parameter Extraction
    // =========================================================================

    pub(crate) fn extract_params_from_signature_in_type_literal(
        &mut self,
        sig: &crate::parser::node::SignatureData,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::InTypeLiteral,
        )
    }

    // =========================================================================
    // Type Literal Resolution
    // =========================================================================

    /// Get type from a type literal node (anonymous object types).
    ///
    /// Type literals represent inline object types like `{ x: string; y: number }` or
    /// callable types with call/construct signatures. This function parses the type
    /// literal and creates the appropriate type representation.
    ///
    /// ## Type Literal Members:
    /// - **Property Signatures**: Named properties with types (`{ x: string }`)
    /// - **Method Signatures**: Function-typed methods (`{ method(): void }`)
    /// - **Call Signatures**: Callable objects (`{ (): string }`)
    /// - **Construct Signatures**: Constructor functions (`{ new(): T }`)
    /// - **Index Signatures**: Dynamic property access (`{ [key: string]: T }`)
    ///
    /// ## Modifiers:
    /// - `?`: Optional property (can be undefined)
    /// - `readonly`: Read-only property (cannot be assigned to)
    ///
    /// ## Type Resolution:
    /// - Property types are resolved via `get_type_from_type_node_in_type_literal`
    /// - Type parameters are pushed/popped for each member
    /// - Index signatures are tracked by key type (string or number)
    ///
    /// ## Result Type:
    /// - **Callable**: If has call/construct signatures
    /// - **ObjectWithIndex**: If has index signatures
    /// - **Object**: Plain object type otherwise
    pub(crate) fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use crate::solver::{
            CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectFlags, ObjectShape,
            PropertyInfo,
        };

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR; // Missing type literal data - propagate error
        };

        let mut properties = Vec::new();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(sig) = self.ctx.arena.get_signature(member) {
                match member.kind {
                    CALL_SIGNATURE => {
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) =
                            self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                        call_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) =
                            self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                        construct_signatures.push(CallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: false,
                        });
                        self.pop_type_parameters(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);

                        if member.kind == METHOD_SIGNATURE {
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, this_type) =
                                self.extract_params_from_signature_in_type_literal(sig);
                            let (return_type, type_predicate) =
                                self.return_type_and_predicate_in_type_literal(sig.type_annotation);
                            let shape = FunctionShape {
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate,
                                is_constructor: false,
                                is_method: true,
                            };
                            let method_type = self.ctx.types.function(shape);
                            self.pop_type_parameters(type_param_updates);
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id: method_type,
                                write_type: method_type,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: true,
                            });
                        } else {
                            let type_id = if !sig.type_annotation.is_none() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            properties.push(PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: sig.question_token,
                                readonly: self.has_readonly_modifier(&sig.modifiers),
                                is_method: false,
                            });
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if !param_data.type_annotation.is_none() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };
                let value_type = if !index_sig.type_annotation.is_none() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                };
                if key_type == TypeId::NUMBER {
                    number_index = Some(info);
                } else {
                    string_index = Some(info);
                }
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            return self.ctx.types.callable(CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: None,
            });
        }

        if string_index.is_some() || number_index.is_some() {
            return self.ctx.types.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties,
                string_index,
                number_index,
            });
        }

        self.ctx.types.object(properties)
    }
}
