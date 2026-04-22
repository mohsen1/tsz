//! Object literal type computation.
//!
//! Handles typing of object literal expressions including property assignments,
//! shorthand properties, method shorthands, getters/setters, spread properties,
//! duplicate property detection, and contextual type inference.
//!
//! Split into submodules:
//! - `computation` — the main `get_type_of_object_literal_with_request` function

mod computation;

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn is_object_define_property_descriptor_literal(&self, idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(parent_idx) = self.ctx.arena.parent_of(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(parent_node) else {
            return false;
        };
        let Some(args) = call.arguments.as_ref() else {
            return false;
        };
        if args.nodes.len() < 3 || args.nodes[2] != idx {
            return false;
        }

        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(callee_access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(object_node) = self.ctx.arena.get(callee_access.expression) else {
            return false;
        };
        if object_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(object_ident) = self.ctx.arena.get_identifier(object_node) else {
            return false;
        };
        if object_ident.escaped_text != "Object" {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(callee_access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        name_ident.escaped_text == "defineProperty"
    }

    fn define_property_descriptor_accessor_type(
        &mut self,
        object_literal_idx: NodeIndex,
        elements: &[NodeIndex],
        method_name: &str,
    ) -> Option<TypeId> {
        if !self.is_object_define_property_descriptor_literal(object_literal_idx) {
            return None;
        }

        for &element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };

            if let Some(method) = self.ctx.arena.get_method_decl(element_node)
                && self
                    .get_property_name_resolved(method.name)
                    .is_some_and(|name| name == method_name)
            {
                let method_type = self.get_type_of_function(element_idx);
                return crate::query_boundaries::assignability::get_function_return_type(
                    self.ctx.types,
                    method_type,
                );
            }

            if method_name == "get"
                && element_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(accessor) = self.ctx.arena.get_accessor(element_node)
                && self
                    .get_property_name_resolved(accessor.name)
                    .is_some_and(|name| name == method_name)
            {
                self.get_type_of_function(element_idx);
                return Some(if accessor.type_annotation.is_none() {
                    self.infer_getter_return_type(accessor.body)
                } else {
                    self.get_type_from_type_node(accessor.type_annotation)
                });
            }
        }

        None
    }

    fn define_property_descriptor_setter_context_type(
        &mut self,
        object_literal_idx: NodeIndex,
        elements: &[NodeIndex],
    ) -> Option<TypeId> {
        let getter_type =
            self.define_property_descriptor_accessor_type(object_literal_idx, elements, "get")?;
        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape::new(
                    vec![tsz_solver::ParamInfo::unnamed(getter_type)],
                    TypeId::VOID,
                )),
        )
    }

    /// Check if a function node is a "set" method inside an Object.defineProperty descriptor.
    /// This is used to suppress TS7006 for setter parameters since they are contextually typed
    /// from the getter (same as true `SET_ACCESSOR` nodes).
    pub(crate) fn is_object_define_property_setter(&mut self, func_idx: NodeIndex) -> bool {
        // func_idx is the METHOD_DECLARATION node itself
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        // Check if this is a method declaration named "set"
        let is_set_method = if let Some(method) = self.ctx.arena.get_method_decl(func_node) {
            self.get_property_name_resolved(method.name)
                .is_some_and(|name| name == "set")
        } else {
            false
        };
        if !is_set_method {
            return false;
        }

        // Get parent (object literal)
        let Some(func_ext) = self.ctx.arena.get_extended(func_idx) else {
            return false;
        };
        let object_literal_idx = func_ext.parent;
        let Some(obj_node) = self.ctx.arena.get(object_literal_idx) else {
            return false;
        };
        if obj_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check if object literal is an Object.defineProperty descriptor
        self.is_object_define_property_descriptor_literal(object_literal_idx)
    }

    /// Get the type of an object literal expression.
    ///
    /// Computes the type of object literals like `{ x: 1, y: 2 }` or `{ foo, bar }`.
    /// Handles:
    /// - Property assignments: `{ x: value }`
    /// - Shorthand properties: `{ x }`
    /// - Method shorthands: `{ foo() {} }`
    /// - Getters/setters: `{ get foo() {}, set foo(v) {} }`
    /// - Spread properties: `{ ...obj }`
    /// - Duplicate property detection
    /// - Contextual type inference
    /// - Implicit any reporting (TS7008)
    #[allow(dead_code)]
    pub(crate) fn get_type_of_object_literal(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_object_literal_with_request(idx, &TypingRequest::NONE)
    }
}
