use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::CommentRange;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::node::{MethodDeclData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::type_queries;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_method_declaration(
        &mut self,
        method_idx: NodeIndex,
    ) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        if self.should_skip_js_augmented_static_method(method_idx, method) {
            self.skip_comments_in_node(method_node.pos, method_node.end);
            return;
        }

        // Get method name as string for overload tracking
        let method_name = self.get_function_name(method_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = method.body.is_none();
        let is_implementation = !is_overload;

        // Check if private
        let is_private = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::PrivateKeyword);

        // Method overload handling:
        // - If this is an overload, emit it and mark that this method has overloads
        // - If this is an implementation and the method already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        // SPECIAL: For private methods with overloads, emit just `private foo;`
        if is_overload {
            // For private methods, emit `private foo;` on first encounter only
            if is_private {
                let already_seen = if let Some(ref name) = method_name {
                    !self.method_names_with_overloads.insert(name.clone())
                } else {
                    false
                };
                if !already_seen {
                    // First private overload: emit `private foo;`
                    self.write_indent();
                    self.emit_member_modifiers(&method.modifiers);
                    self.emit_node(method.name);
                    self.write(";");
                    self.write_line();
                }
                self.skip_comments_in_node(method_node.pos, method_node.end);
                return;
            }
            // Mark that this method name has overload signatures
            if let Some(ref name) = method_name {
                self.method_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = method_name
                && self.method_names_with_overloads.contains(name)
            {
                // Skip implementation signature when overloads exist
                // (for private methods, `private foo;` was already emitted at first overload)
                self.skip_comments_in_node(method_node.pos, method_node.end);
                return;
            }
        }

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&method.modifiers);

        // Name
        self.emit_node(method.name);
        if method.question_token {
            self.write("?");
        }

        // For private methods (no overloads), emit just the name without signature
        if is_private {
            self.write(";");
            self.write_line();
            self.skip_comments_in_node(method_node.pos, method_node.end);
            return;
        }

        // tsc uses property syntax for computed method names in these cases:
        // 1. Computed key with `any` type (from shorthand ambient modules)
        // 2. Optional computed methods (`[key]?()` → `[key]?: (() => T) | undefined`)
        // Non-computed optional methods keep method syntax: `g?(): T`
        let is_computed_name = self
            .arena
            .get(method.name)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);

        let use_property_syntax = is_computed_name
            && (method.question_token
                || self
                    .arena
                    .get(method.name)
                    .and_then(|node| self.arena.get_computed_property(node))
                    .and_then(|cp| self.get_node_type_or_names(&[cp.expression, method.name]))
                    .is_some_and(|t| {
                        t == tsz_solver::types::TypeId::ANY
                            || self.type_interner.is_some_and(|interner| {
                                !tsz_solver::type_queries::is_type_usable_as_property_name(
                                    interner, t,
                                )
                            })
                    }))
            // If the computed name resolves to a known literal (e.g. const enum member),
            // keep method syntax — the name is a valid property name in .d.ts
            && self
                .resolved_computed_property_name_text(method.name)
                .is_none();

        if use_property_syntax {
            self.write(": ");
            if method.question_token {
                self.write("(");
            }
            if let Some(ref type_params) = method.type_parameters
                && !type_params.nodes.is_empty()
            {
                self.emit_type_parameters(type_params);
            }
            self.write("(");
            self.emit_parameters_with_body(&method.parameters, method.body);
            self.write(") => ");
            self.emit_method_function_type_return(method_idx, method);
            if method.question_token {
                self.write(") | undefined;");
            } else {
                self.write(";");
            }
            self.write_line();
            if let Some(body_node) = self.arena.get(method.body) {
                self.skip_comments_in_node(body_node.pos, body_node.end);
            }
            return;
        }

        // Type parameters
        if let Some(ref type_params) = method.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters_with_body(&method.parameters, method.body);
        self.write(")");

        // Return type - SPECIAL CASE: For private methods, TypeScript omits return type in .d.ts
        let method_body = method.body;
        self.emit_method_return_type(method_idx, method, is_private);

        self.write(";");
        self.write_line();

        // Skip comments within the method body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(method_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    pub(in crate::declaration_emitter) fn emit_method_return_type(
        &mut self,
        method_idx: NodeIndex,
        method: &MethodDeclData,
        is_private: bool,
    ) {
        let method_body = method.body;
        let method_name = method.name;
        if method.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(method.type_annotation);
        } else if !is_private
            && let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache)
        {
            let method_type_id = cache
                .node_types
                .get(&method_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[method_name]))
                .or_else(|| self.get_type_via_symbol_for_func(method_idx, method_name));

            if let Some(method_type_id) = method_type_id
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, method_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write(": void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if let Some(method_type_id) = method_type_id {
                if method_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write(": void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(method_type_id));
                }
            } else if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write(": void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        } else if !is_private {
            if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write(": void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        }
    }

    pub(in crate::declaration_emitter) fn emit_method_function_type_return(
        &mut self,
        method_idx: NodeIndex,
        method: &MethodDeclData,
    ) {
        let method_body = method.body;
        let method_name = method.name;
        if method.type_annotation.is_some() {
            self.emit_type(method.type_annotation);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let method_type_id = cache
                .node_types
                .get(&method_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[method_name]))
                .or_else(|| self.get_type_via_symbol_for_func(method_idx, method_name));

            if let Some(method_type_id) = method_type_id
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, method_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write("void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else {
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if let Some(method_type_id) = method_type_id {
                if method_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write("void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else {
                    self.write(&self.print_type_id(method_type_id));
                }
            } else if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write("void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write("any");
                }
            } else if !self.source_is_declaration_file {
                self.write("any");
            }
        } else if method_body.is_some() {
            if self.body_returns_void(method_body) {
                self.write("void");
            } else if let Some(type_text) =
                self.function_body_preferred_return_type_text(method_body)
            {
                self.write(&type_text);
            } else if !self.source_is_declaration_file {
                self.write("any");
            }
        } else if !self.source_is_declaration_file {
            self.write("any");
        }
    }

    pub(in crate::declaration_emitter) fn should_skip_js_augmented_static_method(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        if !self.source_is_js_file
            || !self
                .arena
                .has_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
        {
            return false;
        }
        self.js_augmented_static_method_nodes.contains(&method_idx)
    }

    pub(in crate::declaration_emitter) fn emit_constructor_declaration(
        &mut self,
        ctor_idx: NodeIndex,
    ) {
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = ctor.body.is_none();
        let is_implementation = !is_overload;

        // Constructor overload handling:
        // - If this is an overload, emit it and mark that the class has constructor overloads
        // - If this is an implementation and the class already has constructor overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this class has constructor overloads
            self.class_has_constructor_overloads = true;
        } else if is_implementation {
            // This is an implementation - check if we've seen constructor overloads
            if self.class_has_constructor_overloads {
                // Skip implementation constructor when overloads exist
                return;
            }
        }

        let has_visibility_modifier = ctor.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena.get(mod_idx).is_some_and(|mod_node| {
                    mod_node.kind == SyntaxKind::PrivateKeyword as u16
                        || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                })
            })
        });

        if self.source_is_js_file
            && ctor.parameters.nodes.is_empty()
            && !has_visibility_modifier
            && !self.class_extends_another
        {
            if let Some(body_node) = self.arena.get(ctor.body) {
                self.skip_comments_in_node(body_node.pos, body_node.end);
            }
            return;
        }

        self.write_indent();

        // Emit visibility modifiers (private, protected) on the constructor
        if let Some(ref mods) = ctor.modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PrivateKeyword as u16 => self.write("private "),
                        k if k == SyntaxKind::ProtectedKeyword as u16 => self.write("protected "),
                        _ => {}
                    }
                }
            }
        }

        self.write("constructor(");
        // tsc strips parameters from private constructors in .d.ts output
        let is_private = ctor.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateKeyword as u16)
            })
        });
        let ctor_body = ctor.body;
        if !is_private {
            // Set flag to strip accessibility modifiers from constructor parameters
            self.in_constructor_params = true;
            self.emit_parameters_with_body(&ctor.parameters, ctor.body);
            self.in_constructor_params = false;
        }
        self.write(");");
        self.write_line();

        // Skip comments within the constructor body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(ctor_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    /// Emit parameter properties from constructor as class properties
    /// Parameter properties (e.g., `constructor(public x: number)`) should be emitted
    /// as property declarations in the class body, then stripped from constructor params
    pub(in crate::declaration_emitter) fn emit_parameter_properties(
        &mut self,
        members: &tsz_parser::parser::NodeList,
    ) {
        // Find the constructor
        let ctor_idx = members.nodes.iter().find(|&&idx| {
            self.arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        });

        let Some(&ctor_idx) = ctor_idx else {
            return;
        };

        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Emit parameter properties
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Check if parameter has accessibility modifiers or readonly
                let has_modifier = self.parameter_has_property_modifier(&param.modifiers);

                if has_modifier {
                    let is_destructuring = self.arena.get(param.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                    if is_destructuring {
                        let bindings = self.collect_flattened_binding_entries(
                            param.name,
                            self.preferred_binding_source_type(
                                param.type_annotation,
                                param.initializer,
                                &[param_idx, param.name, param.initializer],
                            ),
                        );
                        for (ident_idx, type_id) in bindings {
                            self.write_indent();
                            let is_private =
                                self.emit_parameter_property_modifiers(&param.modifiers);
                            self.emit_node(ident_idx);
                            if param.question_token {
                                self.write("?");
                            }
                            if !is_private {
                                self.emit_flattened_binding_type_annotation(ident_idx, type_id);
                            }
                            self.write(";");
                            self.write_line();
                        }
                        continue;
                    }

                    // Emit as a property declaration
                    self.write_indent();
                    let is_private = self.emit_parameter_property_modifiers(&param.modifiers);

                    self.emit_node(param.name);

                    // Optional
                    if param.question_token {
                        self.write("?");
                    }

                    // Type annotation (omit for private properties, include for others)
                    if !is_private && param.type_annotation.is_some() {
                        self.write(": ");
                        let before_type = self.writer.len();
                        self.emit_type(param.type_annotation);
                        if param.question_token {
                            // Only append `| undefined` if the type doesn't already
                            // include it (avoid `Type | undefined | undefined`).
                            let full = self.writer.get_output();
                            let type_text = &full[before_type..];
                            if !type_text.ends_with("| undefined") {
                                self.write(" | undefined");
                            }
                        }
                    } else if !is_private
                        && let Some(type_id) = self.get_node_type_or_names(&[param_idx, param.name])
                    {
                        self.write(": ");
                        self.write(&self.print_type_id(type_id));
                    } else if !is_private
                        && param.initializer.is_some()
                        && let Some(type_text) = self.infer_fallback_type_text(param.initializer)
                    {
                        self.write(": ");
                        self.write(&type_text);
                    } else if !is_private && !self.source_is_declaration_file {
                        // Fallback: no explicit type, no inferred type, no initializer
                        self.write(": any");
                    }

                    // Note: No initializer for parameter properties in .d.ts
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn emit_accessor_declaration(
        &mut self,
        accessor_idx: NodeIndex,
        is_getter: bool,
    ) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        // Check if this accessor is private
        let is_private = self
            .arena
            .has_modifier(&accessor.modifiers, SyntaxKind::PrivateKeyword);
        let accessor_body = accessor.body;

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&accessor.modifiers);

        if is_getter {
            self.write("get ");
        } else {
            self.write("set ");
        }

        // Name
        self.emit_node(accessor.name);

        // Parameters - omit types for private accessors
        self.write("(");
        if is_private && !is_getter {
            // TypeScript emits a canonical `value` identifier for private setters in `.d.ts`
            // and intentionally strips the source identifier.
            if let Some(first_param_idx) = accessor.parameters.nodes.first()
                && let Some(first_param_node) = self.arena.get(*first_param_idx)
                && let Some(first_param) = self.arena.get_parameter(first_param_node)
            {
                if first_param.dot_dot_dot_token {
                    self.write("...");
                }

                self.write("value");

                if first_param.question_token {
                    self.write("?");
                }
            }
            self.skip_comments_in_node(accessor_node.pos, accessor_node.end);
        } else {
            self.emit_parameters_without_types(&accessor.parameters, is_private);
        }
        self.write(")");

        // Return type (for getters) - omit for private accessors
        if is_getter && !is_private && accessor.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(accessor.type_annotation);
        } else if is_getter && !is_private {
            if let Some(type_text) = self.matching_setter_parameter_type_text(accessor_idx) {
                self.write(": ");
                self.write(&type_text);
            } else if let Some(type_id) =
                self.get_node_type_or_names(&[accessor_idx, accessor.name])
            {
                // If solver returned `any` but body clearly returns void, prefer void
                if type_id == tsz_solver::types::TypeId::ANY
                    && accessor_body.is_some()
                    && self.body_returns_void(accessor_body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                }
            } else if accessor_body.is_some() {
                if self.body_returns_void(accessor_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(accessor_body)
                {
                    self.write(": ");
                    self.write(&return_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        }

        self.write(";");
        self.write_line();

        // Skip comments within the accessor body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(accessor_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    pub(in crate::declaration_emitter) fn matching_setter_parameter_type_text(
        &mut self,
        accessor_idx: NodeIndex,
    ) -> Option<String> {
        let accessor_name = {
            let accessor_node = self.arena.get(accessor_idx)?;
            let accessor = self.arena.get_accessor(accessor_node)?;
            let name_node = self.arena.get(accessor.name)?;
            self.get_source_slice(name_node.pos, name_node.end)?
        };

        let parent_idx = self.arena.get_extended(accessor_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        let class_decl = self.arena.get_class(parent_node)?;

        for &member_idx in &class_decl.members.nodes {
            if member_idx == accessor_idx {
                continue;
            }

            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                continue;
            }

            let Some(setter) = self.arena.get_accessor(member_node) else {
                continue;
            };
            let Some(setter_name_node) = self.arena.get(setter.name) else {
                continue;
            };
            if self
                .get_source_slice(setter_name_node.pos, setter_name_node.end)
                .as_deref()
                != Some(accessor_name.as_str())
            {
                continue;
            }

            let Some(&param_idx) = setter.parameters.nodes.first() else {
                continue;
            };
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.type_annotation.is_some() {
                let saved_comment_idx = self.comment_emit_idx;
                let saved_pending_source_pos = self.pending_source_pos;
                let saved_writer = std::mem::take(&mut self.writer);
                self.emit_type(param.type_annotation);
                let type_writer = std::mem::replace(&mut self.writer, saved_writer);
                self.comment_emit_idx = saved_comment_idx;
                self.pending_source_pos = saved_pending_source_pos;
                return Some(type_writer.take_output());
            }

            if let Some(type_id) = self.get_node_type_or_names(&[param_idx, param.name]) {
                return Some(self.print_type_id(type_id));
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn emit_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&sig.modifiers);

        self.write("[");
        self.emit_parameters(&sig.parameters);
        self.write("]");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(sig.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_type_alias_declaration(
        &mut self,
        alias_idx: NodeIndex,
    ) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&alias.modifiers)
            && !self.should_emit_public_api_dependency(alias.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&alias.modifiers, Some(alias_idx)) {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::DeclareKeyword)
            && !self.inside_declare_namespace
        {
            self.write("declare ");
        }
        self.write("type ");

        // Name
        self.emit_node(alias.name);

        // Type parameters
        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_enum_declaration(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&enum_data.modifiers)
            && !self.should_emit_public_api_dependency(enum_data.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&enum_data.modifiers, Some(enum_idx)) {
            return;
        }
        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        if is_const {
            self.write("const ");
        }
        self.write("enum ");

        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Evaluate enum member values to get correct auto-increment behavior.
        // Seed the evaluator with accumulated values from prior enums so that
        // cross-enum references (e.g., `enum B { Y = A.X }`) can be resolved.
        let prior = std::mem::take(&mut self.all_enum_values);
        let mut evaluator = EnumEvaluator::with_prior_values(self.arena, prior);
        let member_values = evaluator.evaluate_enum(enum_idx);
        self.all_enum_values = evaluator.take_all_enum_values();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                // For ambient enums (inside declare context or with declare keyword), only
                // emit values for members with explicit initializers.
                // For implementation enums, always emit computed values.
                let is_ambient = self.inside_declare_namespace
                    || self
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword)
                    || self.source_is_declaration_file;
                let has_explicit_init = member.initializer.is_some();
                let should_emit_value = !is_ambient || has_explicit_init || is_const;
                if should_emit_value {
                    let member_name = self.get_enum_member_name(member.name);
                    if let Some(value) = member_values.get(&member_name) {
                        match value {
                            crate::enums::evaluator::EnumValue::Computed => {
                                // Computed values: no initializer in .d.ts
                            }
                            _ => {
                                self.write(" = ");
                                self.emit_enum_value(value);
                            }
                        }
                    } else if !is_ambient {
                        // Fallback to index for non-ambient enums if evaluation failed
                        self.write(" = ");
                        self.write(&i.to_string());
                    }
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    /// Check if an initializer expression is a `Symbol()` call (for unique symbol detection)
    pub(in crate::declaration_emitter) fn is_symbol_call(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        // Check if it's a call expression
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };

        // Check if the function being called is named "Symbol"
        let Some(expr_node) = self.arena.get(call_expr.expression) else {
            return false;
        };

        // Handle both simple identifiers and property access like global.Symbol
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    return ident.escaped_text == "Symbol";
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Handle things like global.Symbol or Symbol.constructor
                if let Some(prop_access) = self.arena.get_access_expr(expr_node) {
                    // Check if the property name is "Symbol"
                    let Some(name_node) = self.arena.get(prop_access.name_or_argument) else {
                        return false;
                    };
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        return ident.escaped_text == "Symbol";
                    }
                }
            }
            _ => {}
        }

        false
    }

    /// Check if a `PrefixUnaryExpression` node is a negative numeric/bigint literal (e.g., `-123`, `-12n`)
    pub(in crate::declaration_emitter) fn is_negative_literal(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if let Some(unary) = self.arena.get_unary_expr(node)
            && unary.operator == SyntaxKind::MinusToken as u16
            && let Some(operand_node) = self.arena.get(unary.operand)
        {
            let k = operand_node.kind;
            return k == SyntaxKind::NumericLiteral as u16 || k == SyntaxKind::BigIntLiteral as u16;
        }
        false
    }

    /// Check whether a property/element access is a simple enum member access (E.A or E["key"]).
    /// Returns true only when the left-hand side is a simple identifier (not a chain like a.b.c).
    pub(in crate::declaration_emitter) fn is_simple_enum_access(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if let Some(access) = self.arena.get_access_expr(node)
            && let Some(expr_node) = self.arena.get(access.expression)
        {
            return expr_node.kind == SyntaxKind::Identifier as u16;
        }
        false
    }

    /// Check whether a computed property name expression is suitable for `.d.ts` emission.
    ///
    /// In tsc, computed property names survive into declaration output when they are
    /// "entity name expressions" — late-bindable names that can be statically resolved:
    /// 1. String literals: `["hello"]`
    /// 2. Numeric literals: `[42]`, `[-1]`
    /// 3. Well-known symbol accesses: `[Symbol.iterator]`, `[Symbol.hasInstance]`, etc.
    /// 4. Identifiers referencing unique symbols or const enums: `[key]`, `[O]`
    /// 5. Property accesses on entity names: `[E.A]`, `[TestEnum.Test1]`
    pub(in crate::declaration_emitter) fn should_emit_computed_property(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return true;
        };

        // Not a computed property name — always emit
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return true;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };

        self.is_entity_name_expression(computed.expression)
    }

    /// Check if an expression is an "entity name expression" — an expression that can
    /// appear as a computed property name in declaration output.
    pub(in crate::declaration_emitter) fn is_entity_name_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            // String literal: ["hello"]
            k if k == SyntaxKind::StringLiteral as u16 => true,
            // Numeric literal: [42]
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            // Identifier: [key], [O], [symb]
            k if k == SyntaxKind::Identifier as u16 => true,
            // Property access: [Symbol.iterator], [E.A], [TestEnum.Test1]
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    self.is_entity_name_expression(access.expression)
                } else {
                    false
                }
            }
            // Prefix unary: [-1]
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => true,
            _ => false,
        }
    }

    /// Get the name `NodeIndex` of a class or interface member, if it has one.
    pub(in crate::declaration_emitter) fn get_member_name_idx(
        &self,
        member_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let member_node = self.arena.get(member_idx)?;

        if let Some(prop) = self.arena.get_property_decl(member_node) {
            return Some(prop.name);
        }
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return Some(method.name);
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return Some(accessor.name);
        }
        if let Some(sig) = self.arena.get_signature(member_node) {
            return Some(sig.name);
        }
        None
    }

    /// Check if a member has a computed property name that should NOT be emitted in `.d.ts`.
    /// Returns `true` if the member should be skipped.
    pub(in crate::declaration_emitter) fn member_has_non_emittable_computed_name(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        if let Some(name_idx) = self.get_member_name_idx(member_idx) {
            !self.should_emit_computed_property(name_idx)
        } else {
            false
        }
    }

    /// Check if a class has any member with a `#private` identifier name.
    /// TypeScript collapses all private-name members into a single `#private;` field.
    pub(in crate::declaration_emitter) fn class_has_private_identifier_member(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            // Check property declarations
            if let Some(prop) = self.arena.get_property_decl(member_node)
                && let Some(name_node) = self.arena.get(prop.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
            // Check method declarations
            if let Some(method) = self.arena.get_method_decl(member_node)
                && let Some(name_node) = self.arena.get(method.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
            // Check accessors
            if let Some(accessor) = self.arena.get_accessor(member_node)
                && let Some(name_node) = self.arena.get(accessor.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a function body has any return statements with value expressions.
    /// Returns true if all returns are bare `return;` or there are no return statements,
    /// meaning the function effectively returns void.
    pub(in crate::declaration_emitter) fn body_returns_void(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return true;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        self.block_returns_void(&block.statements)
    }

    pub(in crate::declaration_emitter) fn block_returns_void(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> bool {
        for &stmt_idx in &statements.nodes {
            if !self.stmt_returns_void(stmt_idx) {
                return false;
            }
        }
        true
    }

    pub(in crate::declaration_emitter) fn stmt_returns_void(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                // Return with expression → non-void
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    return ret.expression.is_none();
                }
                true
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    self.block_returns_void(&block.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.arena.get_if_statement(stmt_node) {
                    // Must check both branches; an if without else can still
                    // contain `return expr;` in the then-branch
                    self.stmt_returns_void(if_data.then_statement)
                        && (if_data.else_statement.is_none()
                            || self.stmt_returns_void(if_data.else_statement))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.arena.get_try(stmt_node) {
                    self.stmt_returns_void(try_data.try_block)
                        && (try_data.catch_clause.is_none()
                            || self.stmt_returns_void(try_data.catch_clause))
                        && (try_data.finally_block.is_none()
                            || self.stmt_returns_void(try_data.finally_block))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.arena.get_catch_clause(stmt_node) {
                    self.stmt_returns_void(catch_data.block)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(stmt_node) {
                    self.block_returns_void(&clause.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                // Check all case clauses inside the switch's case block
                if let Some(switch_data) = self.arena.get_switch(stmt_node) {
                    if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                        && let Some(block) = self.arena.get_block(case_block_node)
                    {
                        self.block_returns_void(&block.statements)
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(stmt_node) {
                    self.stmt_returns_void(loop_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_data) = self.arena.get_for_in_of(stmt_node) {
                    self.stmt_returns_void(for_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(stmt_node) {
                    self.stmt_returns_void(labeled.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_data) = self.arena.get_with_statement(stmt_node) {
                    // with_statement stores its body in then_statement
                    self.stmt_returns_void(with_data.then_statement)
                } else {
                    true
                }
            }
            // Non-compound statements (expression statements, variable declarations, etc.)
            // cannot contain return statements, so they're void-safe.
            _ => true,
        }
    }

    pub(in crate::declaration_emitter) fn emit_variable_declaration_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let has_js_named_export = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena
                .get(decl_list_idx)
                .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                .is_some_and(|decl_list| {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        self.arena
                            .get(decl_idx)
                            .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                            .is_some_and(|decl| self.is_js_named_exported_name(decl.name))
                    })
                })
        });
        if !has_js_named_export && !self.should_emit_public_api_member(&var_stmt.modifiers) {
            // Check if any individual variable is referenced by the public API
            let has_dependency = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        {
                            self.should_emit_public_api_dependency(decl.name)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            });
            if !has_dependency {
                return;
            }
        }
        if self.should_skip_ns_internal_member(&var_stmt.modifiers, None) {
            return;
        }

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // Determine let/const/var
                // `using` and `await using` declarations emit as `const` in .d.ts
                let flags = decl_list_node.flags as u32;
                // USING(4) and AWAIT_USING(6) both have the USING bit set
                let js_var_promoted_to_const;
                let keyword = if flags
                    & (tsz_parser::parser::node_flags::USING
                        | tsz_parser::parser::node_flags::CONST)
                    != 0
                {
                    js_var_promoted_to_const = false;
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    js_var_promoted_to_const = false;
                    "let"
                } else if self.source_is_js_file {
                    js_var_promoted_to_const = true;
                    "const"
                } else {
                    js_var_promoted_to_const = false;
                    "var"
                };

                // Separate destructuring from regular declarations
                let mut regular_decls = Vec::new();
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        let name_node = self.arena.get(decl.name);
                        let is_destructuring = name_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        });

                        if is_destructuring {
                            // Emit destructuring as individual declarations
                            let is_exported =
                                has_export_modifier || self.is_js_named_exported_name(decl.name);
                            self.emit_flattened_variable_declaration(
                                decl_idx,
                                keyword,
                                is_exported,
                            );
                        } else {
                            let is_exported =
                                has_export_modifier || self.is_js_named_exported_name(decl.name);
                            regular_decls.push((is_exported, decl_idx, decl_node, decl));
                        }
                    }
                }

                if regular_decls.len() == 1 {
                    let (is_exported, decl_idx, _decl_node, decl) = regular_decls[0];
                    if self.emit_js_class_like_heuristic_if_needed(decl.name, is_exported) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    if self.emit_js_object_literal_namespace_if_possible(
                        decl.name,
                        decl.initializer,
                        is_exported,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    if self.emit_js_function_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        is_exported,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                }

                // When emitting a non-exported variable statement purely because of
                // dependency tracking, filter to only the declarations that are actually
                // referenced. E.g. `const key = Symbol(), value = 12` should only emit
                // `key` if only `key` is in used_symbols.
                if !has_export_modifier && !has_js_named_export {
                    regular_decls.retain(|(_is_exported, _decl_idx, _decl_node, decl)| {
                        self.should_emit_public_api_dependency(decl.name)
                    });
                }

                // Emit regular declarations in contiguous export/non-export groups.
                let mut group_start = 0;
                while group_start < regular_decls.len() {
                    let is_exported = regular_decls[group_start].0;
                    let mut group_end = group_start;
                    while group_end < regular_decls.len()
                        && regular_decls[group_end].0 == is_exported
                    {
                        group_end += 1;
                    }
                    for (_, _, _, decl) in &regular_decls[group_start..group_end] {
                        self.emit_pending_js_export_equals_for_name(decl.name);
                    }
                    self.write_indent();
                    if is_exported {
                        self.write("export ");
                    }
                    if self.should_emit_declare_keyword(is_exported) {
                        self.write("declare ");
                    }
                    let effective_keyword = if js_var_promoted_to_const {
                        let has_jsdoc = regular_decls[group_start..group_end].iter().any(
                            |(_, decl_idx, _, decl)| {
                                self.jsdoc_name_like_type_expr_for_node(*decl_idx).is_some()
                                    || self.jsdoc_name_like_type_expr_for_node(decl.name).is_some()
                            },
                        ) || self
                            .jsdoc_name_like_type_expr_for_pos(stmt_node.pos)
                            .is_some();
                        if has_jsdoc { "var" } else { keyword }
                    } else {
                        keyword
                    };
                    self.write(effective_keyword);
                    self.write(" ");

                    let mut i = group_start;
                    while i < group_end {
                        if i > group_start {
                            self.write(", ");
                        }
                        let (_is_exported, decl_idx, _decl_node, decl) = &regular_decls[i];

                        // Emit inline comments between keyword and name
                        // (e.g. `var /*4*/ point = ...` → `declare var /*4*/ point: ...`)
                        if let Some(name_node) = self.arena.get(decl.name) {
                            self.emit_inline_block_comments(name_node.pos);
                        }
                        self.emit_node(decl.name);
                        // When a variable's initializer is a simple reference to an
                        // import-equals alias (e.g. `var bVal2 = b` where `import b = a.foo`),
                        // tsc emits `typeof b` instead of expanding the type.
                        if !decl.type_annotation.is_some()
                            && decl.initializer.is_some()
                            && let Some(alias_text) =
                                self.initializer_import_alias_typeof_text(decl.initializer)
                        {
                            self.write(": typeof ");
                            self.write(&alias_text);
                        } else if !decl.type_annotation.is_some()
                            && self.emit_arrow_fn_type_from_ast(decl.initializer)
                        {
                            // Emitted function type directly from AST
                        } else {
                            self.emit_variable_decl_type_or_initializer(
                                keyword,
                                stmt_node.pos,
                                *decl_idx,
                                decl.name,
                                decl.type_annotation,
                                decl.initializer,
                            );
                        }

                        // Skip comments within the declaration's omitted parts (initializer,
                        // inline type comments) to prevent them from leaking as leading
                        // comments on the next statement.
                        // Use the initializer/type end position as the bound, not the full
                        // declaration's end — the parser may set `end` to include trailing
                        // trivia that extends into the next statement's leading JSDoc comments.
                        {
                            let skip_end = if decl.initializer.is_some() {
                                self.arena.get(decl.initializer).map_or(0, |n| n.end)
                            } else if decl.type_annotation.is_some() {
                                self.arena.get(decl.type_annotation).map_or(0, |n| n.end)
                            } else {
                                self.arena.get(decl.name).map_or(0, |n| n.end)
                            };
                            if skip_end > 0
                                && let Some(dn) = self.arena.get(*decl_idx)
                            {
                                self.skip_comments_in_node(dn.pos, skip_end);
                            }
                        }
                        i += 1;
                    }

                    self.write(";");
                    self.write_line();
                    group_start = group_end;
                }
            }
        }
    }
}
