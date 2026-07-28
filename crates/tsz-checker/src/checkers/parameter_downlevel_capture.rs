//! TS2373 for parameter initializers that the emitter must move into the
//! function body (microsoft/TypeScript#36295).
//!
//! When any parameter in a list contains a construct that is downleveled for
//! the current target, the transform hoists parameter initializers into the
//! body, where hoisted `var`/function declarations shadow the outer bindings
//! those initializers reference. `tsc` reports each such reference as TS2373
//! rather than silently changing its meaning.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl CheckerState<'_> {
    /// TS2373 for parameter initializers that the emitter must move into the
    /// function body (microsoft/TypeScript#36295).
    ///
    /// Structural rule: when any parameter in the list contains a construct
    /// that is downleveled for the current target — `??`/`??=`/`?.` below
    /// ES2020, or a class expression carrying a static property declaration
    /// outside standard class-field emit (`useDefineForClassFields !== false`
    /// and target >= ES2022) — the transform hoists parameter initializers
    /// into the body, where hoisted `var`/function declarations shadow the
    /// outer bindings those initializers reference. `tsc` reports each such
    /// reference as TS2373 rather than silently changing its meaning. With no
    /// downleveled construct in the list, the same references are legal and
    /// stay silent.
    pub(crate) fn check_parameter_downlevel_body_capture(&mut self, parameters: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_messages, format_message};
        use tsz_common::common::ScriptTarget;

        if parameters.is_empty() {
            return;
        }
        let target = self.ctx.compiler_options.target as u32;
        let nullish_downleveled = target < ScriptTarget::ES2020 as u32;
        let standard_class_fields = self.ctx.compiler_options.use_define_for_class_fields
            != Some(false)
            && target >= ScriptTarget::ES2022 as u32;
        if !nullish_downleveled && standard_class_fields {
            return;
        }

        let has_trigger = parameters.iter().any(|&param_idx| {
            self.node_contains_downleveled_construct(
                param_idx,
                nullish_downleveled,
                !standard_class_fields,
            )
        });
        if !has_trigger {
            return;
        }

        let Some(func_idx) = self.enclosing_function_like_for_parameter(parameters[0]) else {
            return;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.ctx.arena.get_function(func_node) else {
            return;
        };
        if func.body.is_none() {
            return;
        }
        let mut hoisted_names: Vec<String> = Vec::new();
        self.collect_body_hoisted_declaration_names(func.body, &mut hoisted_names);
        if hoisted_names.is_empty() {
            return;
        }

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let param_display = self
                .get_parameter_name(param.name)
                .or_else(|| self.node_text(param.name));
            let Some(param_display) = param_display else {
                continue;
            };
            // (root, display name): a ref inside a binding element's default is
            // reported against that element's own name (`Parameter 'd'`), while
            // computed keys and the parameter's outer initializer report the
            // whole pattern (`Parameter '{ [a() ?? "d"]: c = "" }'`).
            let mut roots: Vec<(NodeIndex, String)> = Vec::new();
            if param.initializer.is_some() {
                roots.push((param.initializer, param_display.clone()));
            }
            self.collect_binding_pattern_expression_roots(param.name, &param_display, &mut roots);
            for name in &hoisted_names {
                let mut refs: Vec<(NodeIndex, &str)> = Vec::new();
                for (root, display) in &roots {
                    let mut root_refs = Vec::new();
                    self.collect_parameter_forward_references_recursive(
                        *root,
                        name,
                        &mut root_refs,
                    );
                    refs.extend(root_refs.into_iter().map(|r| (r, display.as_str())));
                }
                // A property-access *name* is not an identifier reference:
                // `class { static x = 1 }.x` must not count its `.x` as a use
                // of a body-hoisted `x`.
                refs.retain(|&(ref_idx, _)| !self.is_property_access_name_position(ref_idx));
                if refs.is_empty() {
                    continue;
                }
                for (ref_node, display) in refs {
                    let msg = format_message(
                        diagnostic_messages::PARAMETER_CANNOT_REFERENCE_IDENTIFIER_DECLARED_AFTER_IT,
                        &[display, name],
                    );
                    self.error_at_node(
                        ref_node,
                        &msg,
                        crate::diagnostics::diagnostic_codes::PARAMETER_CANNOT_REFERENCE_IDENTIFIER_DECLARED_AFTER_IT,
                    );
                }
            }
        }
    }

    /// Whether `ref_idx` is the *name* of a property access rather than an
    /// identifier reference (`class { static x = 1 }.x` — its `.x` is not a
    /// use of any `x` binding).
    pub(crate) fn is_property_access_name_position(&self, ref_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .parent_of(ref_idx)
            .and_then(|parent| self.ctx.arena.get(parent))
            .is_some_and(|parent_node| {
                self.ctx
                    .arena
                    .get_access_expr(parent_node)
                    .is_some_and(|access| access.name_or_argument == ref_idx)
            })
    }

    /// Whether a parameter subtree contains a construct the emitter rewrites
    /// for the current target. Nested function bodies are excluded — a
    /// construct inside a deferred closure is transformed in place, not
    /// hoisted with the initializer.
    fn node_contains_downleveled_construct(
        &self,
        node_idx: NodeIndex,
        nullish_downleveled: bool,
        class_fields_downleveled: bool,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        if node_idx.is_none() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.is_function_expression_or_arrow()
            || matches!(
                node.kind,
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
            )
        {
            return false;
        }
        if nullish_downleveled {
            if let Some(binary) = self.ctx.arena.get_binary_expr(node)
                && (binary.operator_token == SyntaxKind::QuestionQuestionToken as u16
                    || binary.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16)
            {
                return true;
            }
            if self
                .ctx
                .arena
                .get_access_expr(node)
                .is_some_and(|access| access.question_dot_token)
            {
                return true;
            }
            if self
                .ctx
                .arena
                .get_call_expr(node)
                .is_some_and(|call| call.question_dot_token)
            {
                return true;
            }
        }
        // An object binding pattern with a rest element downlevels below
        // ES2018 (object rest/spread), moving the whole initializer into the
        // body preamble like the other triggers.
        let object_rest_downleveled = (self.ctx.compiler_options.target as u32)
            < (tsz_common::common::ScriptTarget::ES2018 as u32);
        if object_rest_downleveled
            && node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(node)
            && pattern.elements.nodes.iter().any(|&element_idx| {
                self.ctx
                    .arena
                    .get(element_idx)
                    .and_then(|n| self.ctx.arena.get_binding_element(n))
                    .is_some_and(|element| element.dot_dot_dot_token)
            })
        {
            return true;
        }
        if class_fields_downleveled
            && node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class) = self.ctx.arena.get_class(node)
            && class.members.nodes.iter().any(|&member_idx| {
                self.ctx.arena.get(member_idx).is_some_and(|member_node| {
                    member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self
                            .ctx
                            .arena
                            .get_property_decl(member_node)
                            .is_some_and(|prop| {
                                self.ctx.arena.has_modifier(
                                    &prop.modifiers,
                                    tsz_scanner::SyntaxKind::StaticKeyword,
                                )
                            })
                })
            })
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| {
                self.node_contains_downleveled_construct(
                    child,
                    nullish_downleveled,
                    class_fields_downleveled,
                )
            })
    }

    /// Names of `var` and function declarations hoisted across the enclosing
    /// function body, including those inside nested blocks/loops/switches but
    /// not inside nested functions.
    fn collect_body_hoisted_declaration_names(&self, node_idx: NodeIndex, out: &mut Vec<String>) {
        use tsz_parser::parser::{node_flags, syntax_kind_ext};

        if node_idx.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        if node.is_function_expression_or_arrow()
            || matches!(
                node.kind,
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::CLASS_DECLARATION
            )
        {
            return;
        }
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.ctx.arena.get_function(node)
                && let Some(name) = self
                    .ctx
                    .arena
                    .get(func.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
            {
                out.push(name.escaped_text.to_string());
            }
            return;
        }
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let flags = self.ctx.arena.get_variable_declaration_flags(node_idx);
            if flags & (node_flags::LET | node_flags::CONST) == 0
                && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && let Some(name) = self
                    .ctx
                    .arena
                    .get(var_decl.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
            {
                out.push(name.escaped_text.to_string());
            }
            return;
        }
        for child in self.ctx.arena.get_children(node_idx) {
            self.collect_body_hoisted_declaration_names(child, out);
        }
    }

    /// Expression roots inside a binding pattern that evaluate with the
    /// parameter position itself: element defaults and computed property
    /// keys, recursively through nested patterns. Binding identifiers are
    /// declarations, not references, and are never included.
    fn collect_binding_pattern_expression_roots(
        &self,
        name_idx: NodeIndex,
        pattern_display: &str,
        out: &mut Vec<(NodeIndex, String)>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            && node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
        {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_binding_pattern(node) else {
            return;
        };
        for &element_idx in &pattern.elements.nodes {
            let Some(element) = self
                .ctx
                .arena
                .get(element_idx)
                .and_then(|n| self.ctx.arena.get_binding_element(n))
            else {
                continue;
            };
            if element.property_name.is_some()
                && let Some(property_node) = self.ctx.arena.get(element.property_name)
                && property_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.ctx.arena.get_computed_property(property_node)
            {
                out.push((computed.expression, pattern_display.to_string()));
            }
            if element.initializer.is_some() {
                let element_display = self
                    .get_parameter_name(element.name)
                    .or_else(|| self.node_text(element.name))
                    .unwrap_or_else(|| pattern_display.to_string());
                out.push((element.initializer, element_display));
            }
            self.collect_binding_pattern_expression_roots(element.name, pattern_display, out);
        }
    }
}
