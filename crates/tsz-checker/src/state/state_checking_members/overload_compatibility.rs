//! Overload compatibility, signature utilities, and implicit-any return checks.
//!
//! Extracted from `ambient_signature_checks.rs` to keep files focused and under the
//! 2000 LOC limit. Contains:
//! - `lower_type_with_bindings` — type lowering with type parameter bindings
//! - `maybe_report_implicit_any_return` — TS7010/TS7011 implicit-any return diagnostics
//! - `check_overload_compatibility` — TS2394 overload-implementation compatibility
//! - `check_modifier_combinations` — modifier conflict checks (e.g., abstract + private)

use crate::query_boundaries::assignability::{
    erase_function_type_params_to_any, get_function_return_type, replace_function_return_type,
    rewrite_function_error_slots_to_any,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{CallableShape, FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    fn required_parameter_count_for_overload_compatibility(
        &self,
        type_id: tsz_solver::TypeId,
    ) -> Option<usize> {
        if let Some(shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id) {
            return Some(
                shape
                    .params
                    .iter()
                    .filter(|param| param.is_required())
                    .count(),
            );
        }

        tsz_solver::type_queries::get_construct_signatures(self.ctx.types, type_id).and_then(
            |sigs| {
                sigs.first().map(|sig| {
                    sig.params
                        .iter()
                        .filter(|param| param.is_required())
                        .count()
                })
            },
        )
    }

    fn jsdoc_overload_tag_span(
        &self,
        comment: &tsz_common::comments::CommentRange,
        source_text: &str,
    ) -> Option<(u32, u32)> {
        let raw_comment = source_text.get(comment.pos as usize..comment.end as usize)?;
        let offset = raw_comment.find("@overload")?;
        Some((comment.pos + offset as u32 + 1, "overload".len() as u32))
    }

    fn jsdoc_has_explicit_return_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
                .is_some_and(|rest| rest.trim().starts_with('{'))
        })
    }

    fn leading_jsdoc_comments_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Vec<tsz_common::comments::CommentRange> {
        use tsz_common::comments::is_jsdoc_comment;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return Vec::new();
        };
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return Vec::new();
        };

        let mut current_pos = node.pos;
        let mut result = Vec::new();
        for comment in sf.comments.iter().rev() {
            if comment.end > current_pos {
                continue;
            }

            let Ok(start) = usize::try_from(comment.end) else {
                break;
            };
            let Ok(end) = usize::try_from(current_pos) else {
                break;
            };
            let Some(gap) = sf.text.get(start..end) else {
                break;
            };
            if !gap.chars().all(char::is_whitespace) {
                break;
            }
            if !is_jsdoc_comment(comment, &sf.text) {
                break;
            }

            result.push(comment.clone());
            current_pos = comment.pos;
        }

        result.reverse();
        result
    }

    pub(crate) fn check_jsdoc_overload_implicit_any_return(&mut self, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::get_jsdoc_content;

        if !self.ctx.no_implicit_any()
            || !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || self.has_syntax_parse_errors()
        {
            return;
        }

        if self.ctx.arena.get(node_idx).is_none() {
            return;
        }
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        for comment in self.leading_jsdoc_comments_for_node(node_idx) {
            let jsdoc = get_jsdoc_content(&comment, &sf.text);
            if !jsdoc.contains("@overload") || Self::jsdoc_has_explicit_return_tag(&jsdoc) {
                continue;
            }

            let (error_pos, error_len) = self
                .jsdoc_overload_tag_span(&comment, &sf.text)
                .unwrap_or((comment.pos, 0));
            self.ctx.error(
                error_pos,
                error_len,
                crate::diagnostics::format_message(
                    diagnostic_messages::THIS_OVERLOAD_IMPLICITLY_RETURNS_THE_TYPE_BECAUSE_IT_LACKS_A_RETURN_TYPE_ANNOTAT,
                    &["any"],
                ),
                diagnostic_codes::THIS_OVERLOAD_IMPLICITLY_RETURNS_THE_TYPE_BECAUSE_IT_LACKS_A_RETURN_TYPE_ANNOTAT,
            );
        }
    }

    fn jsdoc_constructor_overload_types(
        &mut self,
        ctor_idx: NodeIndex,
    ) -> Vec<(tsz_solver::TypeId, u32, u32)> {
        use tsz_common::comments::get_jsdoc_content;

        if !self.is_js_file() {
            return Vec::new();
        }

        let Some(node) = self.ctx.arena.get(ctor_idx) else {
            return Vec::new();
        };
        let Some(ctor) = self.ctx.arena.get_constructor(node) else {
            return Vec::new();
        };
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return Vec::new();
        };

        let instance_type =
            if let Some(class_idx) = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx) {
                let class_node = self.ctx.arena.get(class_idx);
                if let Some(class) = class_node.and_then(|node| self.ctx.arena.get_class(node)) {
                    self.get_class_instance_type(class_idx, class)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };

        let class_type_params = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|info| info.class_type_parameters.clone())
            .unwrap_or_default();
        let base_signature =
            self.call_signature_from_constructor(ctor, ctor_idx, instance_type, &class_type_params);
        let mut overloads = Vec::new();

        for comment in self.leading_jsdoc_comments_for_node(ctor_idx) {
            let jsdoc = get_jsdoc_content(&comment, &sf.text);
            if !jsdoc.contains("@overload") {
                continue;
            }

            let mut signature = base_signature.clone();
            let jsdoc_params = Self::extract_jsdoc_param_names(&jsdoc);
            signature.params.truncate(jsdoc_params.len());

            for (i, (param_name, _)) in jsdoc_params.iter().enumerate() {
                let Some(param) = signature.params.get_mut(i) else {
                    break;
                };

                let jsdoc_optional = Self::extract_jsdoc_param_type_string(&jsdoc, param_name)
                    .is_some_and(|type_expr| type_expr.trim().ends_with('='))
                    || Self::is_jsdoc_param_optional_by_brackets(&jsdoc, param_name);

                if let Some(jsdoc_type) =
                    self.resolve_jsdoc_param_type_with_pos(&jsdoc, param_name, Some(comment.pos))
                {
                    param.type_id = jsdoc_type;
                }

                // For JSDoc overload signatures, only use JSDoc-specified optionality.
                // Don't inherit the JS implicit-optional from the base signature, since
                // overload params are defined by JSDoc alone.
                param.optional = jsdoc_optional;
            }

            let overload_type = self.ctx.types.factory().callable(CallableShape {
                call_signatures: Vec::new(),
                construct_signatures: vec![signature],
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            });

            let (error_pos, error_len) = self
                .jsdoc_overload_tag_span(&comment, &sf.text)
                .unwrap_or((comment.pos, 0));
            overloads.push((overload_type, error_pos, error_len));
        }

        overloads
    }

    fn constructor_type_for_overload_compatibility(
        &mut self,
        ctor_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(ctor_idx)?;
        let ctor = self.ctx.arena.get_constructor(node)?;

        let class_info = self.ctx.enclosing_class.as_ref()?;
        let class_idx = class_info.class_idx;
        let class_type_params = class_info.class_type_parameters.clone();
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let instance_type = self.get_class_instance_type(class_idx, class);
        let mut sig =
            self.call_signature_from_constructor(ctor, ctor_idx, instance_type, &class_type_params);

        // In JS files, `extract_params_from_parameter_list` marks untyped params as
        // implicitly optional. For overload compatibility, TSC uses the actual AST
        // parameter properties: a param is required only if it lacks both `?` and a
        // default initializer, regardless of whether it has a type annotation.
        // Reset the optional flags to match TSC's overload compatibility semantics.
        if self.is_js_file() {
            for (i, &param_idx) in ctor.parameters.nodes.iter().enumerate() {
                if i >= sig.params.len() {
                    break;
                }
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    sig.params[i].optional = param.question_token || param.initializer.is_some();
                }
            }
        }

        Some(self.ctx.types.factory().function(FunctionShape {
            type_params: sig.type_params,
            params: sig.params,
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: true,
            is_method: false,
        }))
    }

    /// Lower a type node with type parameter bindings.
    ///
    /// This is used to substitute type parameters with concrete types
    /// when extracting type arguments from generic Promise types.
    /// Made pub(crate) so it can be called from `promise_checker.rs`.
    pub(crate) fn lower_type_with_bindings(
        &self,
        type_node: NodeIndex,
        bindings: Vec<(tsz_common::interner::Atom, TypeId)>,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;

        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(bindings);
        lowering.lower_type(type_node)
    }

    // Note: type_contains_any, implicit_any_return_display, should_report_implicit_any_return are in type_checking.rs

    pub(crate) fn maybe_report_implicit_any_return(
        &mut self,
        name: Option<String>,
        name_node: Option<NodeIndex>,
        return_type: TypeId,
        has_type_annotation: bool,
        has_contextual_return: bool,
        fallback_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.no_implicit_any() || has_type_annotation || has_contextual_return {
            return;
        }

        // Suppress TS7010/TS7011 when the file has parse errors.
        // TSC does not emit implicit-any return diagnostics for files with syntax errors,
        // since the parse error itself is sufficient and the AST shape may be unreliable.
        if self.has_syntax_parse_errors() {
            return;
        }

        // In checkJs mode, be conservative and skip implicit-any return diagnostics in JS files.
        if self.is_js_file() {
            return;
        }

        // Suppress TS7010/TS7011 when parse errors exist near the function declaration.
        // Parser error recovery can produce malformed function nodes (e.g. `function =>`)
        // where the implicit-any-return diagnostic is noise on top of the syntax error.
        if self.has_syntax_parse_errors() && self.node_has_nearby_parse_error(fallback_node) {
            return;
        }
        // TypeScript does not report TS7010/TS7011 when all value-return paths use
        // an explicit `as any`/`<any>` assertion.
        if let Some(node) = self.ctx.arena.get(fallback_node) {
            let body = if let Some(func) = self.ctx.arena.get_function(node) {
                Some(func.body)
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                Some(method.body)
            } else {
                self.ctx
                    .arena
                    .get_accessor(node)
                    .map(|accessor| accessor.body)
            };
            if let Some(body_idx) = body
                && body_idx.is_some()
            {
                if self.has_only_explicit_any_assertion_returns(body_idx) {
                    return;
                }
                // When the function has a body, the return type was inferred from it.
                // An inferred `any` (e.g., `return x` where `x: any`) is a valid inference
                // result, not "implicit any". TSC only emits TS7010 for bodyless
                // declarations (interfaces, abstract methods) where `any` is the default.
                if return_type == TypeId::ANY {
                    return;
                }
            }
        }
        if !self.should_report_implicit_any_return(return_type) {
            return;
        }

        // tsc suppresses the function-expression TS7011 in common cases where the
        // same closure already has implicit-any parameter errors (TS7006/TS7019).
        // Avoid double-reporting for unnamed function expressions/arrow functions.
        if name.is_none() && self.has_untyped_value_parameters(fallback_node) {
            return;
        }

        let return_text = self.implicit_any_return_display(return_type);
        if let Some(name) = name {
            self.error_at_node_msg(
                name_node.unwrap_or(fallback_node),
                diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                &[&name, &return_text],
            );
        } else {
            self.error_at_node_msg(
                fallback_node,
                diagnostic_codes::FUNCTION_EXPRESSION_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN,
                &[&return_text],
            );
        }
    }

    pub(crate) fn has_untyped_value_parameters(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        let has_untyped = |param_idx: NodeIndex| {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                return false;
            };
            if param.type_annotation.is_some() {
                return false;
            }
            let name = self.parameter_name_for_error(param.name);
            if name.is_empty() {
                return true;
            }
            name != "this"
        };

        if let Some(func) = self.ctx.arena.get_function(node) {
            return func.parameters.nodes.iter().copied().any(has_untyped);
        }
        if let Some(method) = self.ctx.arena.get_method_decl(node) {
            return method.parameters.nodes.iter().copied().any(has_untyped);
        }
        if let Some(sig) = self.ctx.arena.get_signature(node)
            && let Some(params) = sig.parameters.as_ref()
        {
            return params.nodes.iter().copied().any(has_untyped);
        }

        false
    }

    /// Check overload compatibility: implementation must be assignable to all overload signatures.
    ///
    /// Reports TS2394 when an implementation signature is not compatible with its overload signatures.
    /// This check ensures that the implementation can handle all valid calls that match the overloads.
    ///
    /// Per TypeScript's variance rules:
    /// - Implementation parameters must be supertypes of overload parameters (contravariant)
    /// - Implementation return type must be subtype of overload return type (covariant)
    /// - Effectively: Implementation <: Overload (implementation is assignable to overload)
    ///
    /// This handles:
    /// - Function declarations
    /// - Method declarations (class methods)
    /// - Constructor declarations
    pub(crate) fn check_overload_compatibility(&mut self, impl_node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // 1. Get the implementation's symbol
        let Some(impl_sym_id) = self.ctx.binder.get_node_symbol(impl_node_idx) else {
            return;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(impl_sym_id) else {
            return;
        };

        // Fast path: if there are no overload declarations for this symbol,
        // skip expensive signature lowering/compatibility setup entirely.
        let has_overload_decl = symbol.declarations.iter().copied().any(|decl_idx| {
            if decl_idx == impl_node_idx {
                return false;
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };

            match decl_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .ctx
                    .arena
                    .get_function(decl_node)
                    .is_some_and(|f| f.body.is_none()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(decl_node)
                    .is_some_and(|m| m.body.is_none()),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(decl_node)
                    .is_some_and(|c| c.body.is_none()),
                _ => false,
            }
        });
        let jsdoc_overloads = if has_overload_decl {
            Vec::new()
        } else {
            self.jsdoc_constructor_overload_types(impl_node_idx)
        };
        if !has_overload_decl && jsdoc_overloads.is_empty() {
            return;
        }

        // 2. Create TypeLowering instance for manual signature lowering
        // This unblocks overload validation for methods/constructors where get_type_of_node returns ERROR
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            self.ctx.binder.get_node_symbol(node_idx).map(|id| id.0)
        };
        let lowering = tsz_lowering::TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        // 3. Get the implementation's type using manual lowering.
        // When the implementation has no return type annotation, lower_return_type returns ERROR.
        // We then try to get the inferred return type from the full type system, matching tsc's
        // behavior of using the inferred return type for overload compatibility checking.
        let impl_return_override = self.get_impl_return_type_override(impl_node_idx);
        let mut impl_type = self
            .constructor_type_for_overload_compatibility(impl_node_idx)
            .unwrap_or_else(|| {
                lowering.lower_signature_from_declaration(impl_node_idx, impl_return_override)
            });
        // If lowering produced a function with ERROR return type, prefer get_type_of_node
        // which resolves type references through the full type environment.
        // Manual lowering cannot resolve interface/class type references that require
        // full binder scope resolution (e.g., `Moose` in `function f(): Moose {}`).
        let lowered_ret = get_function_return_type(self.ctx.types, impl_type);
        if impl_type == tsz_solver::TypeId::ERROR || lowered_ret == Some(tsz_solver::TypeId::ERROR)
        {
            let node_type = self.get_type_of_node(impl_node_idx);
            if node_type != tsz_solver::TypeId::ERROR {
                impl_type = node_type;
            } else if impl_type == tsz_solver::TypeId::ERROR {
                return;
            }
        }
        // When the implementation has no return type annotation, the lowered return is ANY
        // (from get_impl_return_type_override). Try to replace it with the inferred return type
        // from the full type system, matching tsc's isImplementationCompatibleWithOverload which
        // uses the actual inferred return type rather than `any`. This correctly detects cases
        // like `function f() { return f; }` where the return type is `typeof f`, not `any`.
        if impl_return_override == Some(tsz_solver::TypeId::ANY)
            && let Some(ret) = get_function_return_type(self.ctx.types, impl_type)
            && ret == tsz_solver::TypeId::ANY
        {
            // The return was our ANY override. Try to get the inferred return type.
            let node_type = self.get_type_of_node(impl_node_idx);
            if node_type != tsz_solver::TypeId::ERROR
                && let Some(inferred_ret) = get_function_return_type(self.ctx.types, node_type)
                && inferred_ret != tsz_solver::TypeId::ERROR
                && inferred_ret != tsz_solver::TypeId::ANY
            {
                impl_type = replace_function_return_type(self.ctx.types, impl_type, inferred_ret);
            }
        }

        // When the implementation has no return type annotation, the manual lowering
        // used ANY as the return type. But TSC uses the actual inferred return type
        // from the body for overload compatibility (via getReturnTypeOfSignature).
        // Try to get the inferred return type and use it instead of ANY.
        if impl_return_override.is_some() {
            let inferred_type = self.get_type_of_node(impl_node_idx);
            if inferred_type != tsz_solver::TypeId::ERROR {
                let inferred_ret = get_function_return_type(self.ctx.types, inferred_type);
                if let Some(ret) = inferred_ret
                    && ret != tsz_solver::TypeId::ERROR
                {
                    impl_type = replace_function_return_type(self.ctx.types, impl_type, ret);
                }
            }
        }

        // Fix up ERROR parameter types in the implementation signature.
        // When implementation params lack type annotations, lowering produces ERROR.
        // Replace with ANY since TypeScript treats untyped impl params as `any`.
        impl_type = self.fix_error_params_in_function(impl_type);

        // 4. Check each overload declaration
        for &decl_idx in &symbol.declarations {
            // Skip the implementation itself
            if decl_idx == impl_node_idx {
                continue;
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // 5. Check if this declaration is an overload (has no body)
            // We must handle Functions, Methods, and Constructors
            let is_overload = match decl_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .ctx
                    .arena
                    .get_function(decl_node)
                    .is_some_and(|f| f.body.is_none()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(decl_node)
                    .is_some_and(|m| m.body.is_none()),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(decl_node)
                    .is_some_and(|c| c.body.is_none()),
                _ => false, // Not a callable declaration we care about
            };

            if !is_overload {
                continue;
            }

            // 6. Get the overload's type using manual lowering
            // For overloads without return type annotations, use VOID (matching tsc behavior).
            let overload_return_override = self.get_overload_return_type_override(decl_idx);
            let mut overload_type = self
                .constructor_type_for_overload_compatibility(decl_idx)
                .unwrap_or_else(|| {
                    lowering.lower_signature_from_declaration(decl_idx, overload_return_override)
                });
            // Same ERROR return fallback for overloads
            let overload_lowered_ret = get_function_return_type(self.ctx.types, overload_type);
            if overload_type == tsz_solver::TypeId::ERROR
                || overload_lowered_ret == Some(tsz_solver::TypeId::ERROR)
            {
                let node_type = self.get_type_of_node(decl_idx);
                if node_type != tsz_solver::TypeId::ERROR {
                    overload_type = node_type;
                } else if overload_type == tsz_solver::TypeId::ERROR {
                    continue;
                }
            }
            // Fix ERROR param types in overload (untyped params → any)
            overload_type = self.fix_error_params_in_function(overload_type);

            // 7. Check compatibility using tsc's bidirectional return type rule:
            // First check if return types are compatible in EITHER direction,
            // then check parameter-only assignability (ignoring return types).
            // This matches tsc's isImplementationCompatibleWithOverload.
            if !self.is_implementation_compatible_with_overload(impl_type, overload_type) {
                // TSC anchors the error at the function/method name, not the whole declaration.
                let error_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                    diagnostic_codes::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                );
                // TSC only reports the first incompatible overload per function.
                break;
            }
        }

        if has_overload_decl {
            return;
        }

        for (mut overload_type, error_pos, error_len) in jsdoc_overloads {
            overload_type = self.fix_error_params_in_function(overload_type);
            if !self.is_implementation_compatible_with_overload(impl_type, overload_type) {
                self.ctx.error(
                    error_pos,
                    error_len,
                    diagnostic_messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE.to_string(),
                    diagnostic_codes::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                );
                break;
            }
        }
    }

    /// Returns `Some(TypeId::ANY)` if the implementation node has no explicit return type annotation.
    /// Replace ERROR parameter types with ANY in a function type.
    /// Used for overload compatibility: untyped implementation params are treated as `any`.
    pub(crate) fn fix_error_params_in_function(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        rewrite_function_error_slots_to_any(self.ctx.types, type_id)
    }

    /// This is used for overload compatibility checking: when the implementation omits a return type,
    /// the lowering would produce ERROR, but TypeScript treats it as `any` for compatibility purposes.
    pub(crate) fn get_impl_return_type_override(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::TypeId> {
        let node = self.ctx.arena.get(node_idx)?;
        let has_annotation = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .is_some_and(|f| f.type_annotation.is_some()),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|m| m.type_annotation.is_some()),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                // Constructors never have return type annotations
                return None;
            }
            _ => return None,
        };
        if has_annotation {
            None
        } else {
            Some(tsz_solver::TypeId::ANY)
        }
    }

    /// Returns `Some(TypeId::VOID)` if an overload node has no explicit return type annotation.
    /// Overloads without return type annotations default to void (matching tsc behavior).
    pub(crate) fn get_overload_return_type_override(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::TypeId> {
        let node = self.ctx.arena.get(node_idx)?;
        let has_annotation = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .is_some_and(|f| f.type_annotation.is_some()),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|m| m.type_annotation.is_some()),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                return None;
            }
            _ => return None,
        };
        if has_annotation {
            None
        } else {
            Some(tsz_solver::TypeId::VOID)
        }
    }

    /// Check overload compatibility using tsc's bidirectional return type rule.
    /// Matches tsc's `isImplementationCompatibleWithOverload`:
    /// 1. Check if return types are compatible in EITHER direction (or target is void)
    /// 2. If so, check parameter-only assignability (with return types ignored)
    ///
    /// Uses bivariant assignability because tsc uses non-strict function types
    /// for overload compatibility (implementation params can be wider or narrower).
    pub(crate) fn is_implementation_compatible_with_overload(
        &mut self,
        impl_type: tsz_solver::TypeId,
        overload_type: tsz_solver::TypeId,
    ) -> bool {
        let constructors_only =
            tsz_solver::type_queries::is_constructor_like_type(self.ctx.types, impl_type)
                && tsz_solver::type_queries::is_constructor_like_type(
                    self.ctx.types,
                    overload_type,
                );

        // Erase type parameters to `any` before comparing, matching TSC's
        // `getErasedSignature` in `isImplementationCompatibleWithOverload`.
        // This ensures positional parameter comparison works when the impl
        // and overload use type params in different structural positions.
        let impl_type = erase_function_type_params_to_any(self.ctx.types, impl_type);
        let overload_type = erase_function_type_params_to_any(self.ctx.types, overload_type);

        // An implementation cannot require more arguments than one of its overloads.
        // Extra arguments are allowed at call sites, but missing required ones are not.
        if let (Some(impl_required), Some(overload_required)) = (
            self.required_parameter_count_for_overload_compatibility(impl_type),
            self.required_parameter_count_for_overload_compatibility(overload_type),
        ) && impl_required > overload_required
        {
            return false;
        }

        // Get return types of both (erased) signatures
        let impl_return = get_function_return_type(self.ctx.types, impl_type);
        let overload_return = get_function_return_type(self.ctx.types, overload_type);

        match (impl_return, overload_return) {
            (Some(impl_ret), Some(overload_ret)) => {
                // Bidirectional return type check: either direction must be assignable,
                // or the overload returns void
                let return_compatible = constructors_only
                    || overload_ret == tsz_solver::TypeId::VOID
                    || self.is_assignable_to_bivariant(overload_ret, impl_ret)
                    || self.is_assignable_to_bivariant(impl_ret, overload_ret);

                if !return_compatible {
                    return false;
                }

                // Now check parameter-only compatibility by creating versions
                // with ANY return types. Use bivariant check to match tsc's
                // non-strict function types for overload compatibility.
                let impl_with_any_ret =
                    self.replace_return_type(impl_type, tsz_solver::TypeId::ANY);
                let overload_with_any_ret =
                    self.replace_return_type(overload_type, tsz_solver::TypeId::ANY);
                self.is_assignable_to_bivariant(impl_with_any_ret, overload_with_any_ret)
            }
            _ => {
                // If we can't get return types, fall back to bivariant assignability
                self.is_assignable_to_bivariant(impl_type, overload_type)
            }
        }
    }

    /// Replace the return type of a function type with the given type.
    /// Returns the original type unchanged if it's not a Function.
    pub(crate) fn replace_return_type(
        &mut self,
        type_id: tsz_solver::TypeId,
        new_return: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        replace_function_return_type(self.ctx.types, type_id, new_return)
    }

    /// TS2385: "Overload signatures must all be public, private or protected."
    ///
    /// When a class method has overload signatures, all overload signatures must have
    /// the same access modifier as the implementation. tsc uses the implementation's
    /// modifier as canonical and flags each overload that disagrees.
    pub(crate) fn check_overload_modifier_consistency(&mut self, impl_node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(impl_sym_id) = self.ctx.binder.get_node_symbol(impl_node_idx) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(impl_sym_id) else {
            return;
        };
        if symbol.declarations.len() < 2 {
            return;
        }

        // Helper: extract access modifier kind from a declaration node
        let get_access_modifier =
            |arena: &tsz_parser::parser::NodeArena, node_idx: NodeIndex| -> u16 {
                let Some(node) = arena.get(node_idx) else {
                    return SyntaxKind::PublicKeyword as u16; // default is public
                };
                if let Some(mods) = arena.get_declaration_modifiers(node) {
                    for &m_idx in &mods.nodes {
                        if let Some(m_node) = arena.get(m_idx)
                            && (m_node.kind == SyntaxKind::PrivateKeyword as u16
                                || m_node.kind == SyntaxKind::ProtectedKeyword as u16
                                || m_node.kind == SyntaxKind::PublicKeyword as u16)
                        {
                            return m_node.kind;
                        }
                    }
                }
                SyntaxKind::PublicKeyword as u16 // no explicit modifier = public
            };

        // Helper: check if a declaration has the `static` modifier
        let has_static = |arena: &tsz_parser::parser::NodeArena, node_idx: NodeIndex| -> bool {
            let Some(node) = arena.get(node_idx) else {
                return false;
            };
            if let Some(mods) = arena.get_declaration_modifiers(node) {
                for &m_idx in &mods.nodes {
                    if let Some(m_node) = arena.get(m_idx)
                        && m_node.kind == SyntaxKind::StaticKeyword as u16
                    {
                        return true;
                    }
                }
            }
            false
        };

        // Use the implementation's modifier as canonical
        let impl_modifier = get_access_modifier(self.ctx.arena, impl_node_idx);
        let impl_is_static = has_static(self.ctx.arena, impl_node_idx);

        // Check each overload signature against the implementation.
        // Only compare declarations with the same static/instance status.
        for &decl_idx in &symbol.declarations {
            if decl_idx == impl_node_idx {
                continue;
            }
            if has_static(self.ctx.arena, decl_idx) != impl_is_static {
                continue;
            }
            let decl_modifier = get_access_modifier(self.ctx.arena, decl_idx);
            if decl_modifier != impl_modifier {
                // TSC anchors TS2385 at the start of the overload declaration (including modifiers),
                // not at the declaration name. Our constructor nodes start at the `constructor`
                // keyword, so we need to extend the span back to the first modifier.
                if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                    let start = self
                        .ctx
                        .arena
                        .get_declaration_modifiers(decl_node)
                        .and_then(|mods| mods.nodes.first().copied())
                        .and_then(|first_mod| self.ctx.arena.get(first_mod))
                        .map_or(decl_node.pos, |mod_node| mod_node.pos);
                    let length = decl_node.end.saturating_sub(start);
                    self.error(
                        start,
                        length,
                        diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED.to_string(),
                        diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                    );
                }
            }
        }
    }

    pub(crate) fn check_modifier_combinations(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(mods) = modifiers else {
            return;
        };

        let mut abstract_node = None;
        let mut conflicting_nodes = Vec::new();

        for &m_idx in &mods.nodes {
            if let Some(m_node) = self.ctx.arena.get(m_idx) {
                let kind = m_node.kind;
                use tsz_scanner::SyntaxKind;
                if kind == SyntaxKind::AbstractKeyword as u16 {
                    abstract_node = Some(m_idx);
                } else if kind == SyntaxKind::PrivateKeyword as u16 {
                    conflicting_nodes.push((m_idx, "private"));
                } else if kind == SyntaxKind::StaticKeyword as u16 {
                    conflicting_nodes.push((m_idx, "static"));
                } else if kind == SyntaxKind::AsyncKeyword as u16 {
                    conflicting_nodes.push((m_idx, "async"));
                }
            }
        }

        if let Some(abs_node) = abstract_node {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            for (conflict_idx, name) in conflicting_nodes {
                let message = format_message(
                    diagnostic_messages::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                    &[name, "abstract"],
                );

                // Point to whichever modifier comes second
                let (abs_start, _) = self.get_node_span(abs_node).unwrap_or((0, 0));
                let (con_start, _) = self.get_node_span(conflict_idx).unwrap_or((0, 0));

                let error_node = if con_start > abs_start {
                    conflict_idx
                } else {
                    abs_node
                };

                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_MODIFIER,
                );
            }
        }
    }

    /// Check that overload signatures for a method agree on optionality (TS2386).
    ///
    /// TS2385 is emitted from the duplicate-identifier pass, which has the full
    /// declaration group and already serves as the canonical overload-modifier path.
    /// Re-emitting it here duplicates diagnostics for class methods.
    pub(crate) fn check_overload_modifier_agreement(&mut self, impl_node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(impl_sym_id) = self.ctx.binder.get_node_symbol(impl_node_idx) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(impl_sym_id) else {
            return;
        };
        if symbol.declarations.len() < 2 {
            return;
        }

        // Collect all overload declarations (signatures without body) for this symbol
        let mut overload_decls: Vec<NodeIndex> = Vec::new();
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let is_signature = match decl_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(decl_node)
                    .is_some_and(|m| m.body.is_none()),
                k if k == syntax_kind_ext::METHOD_SIGNATURE => true,
                _ => false,
            };
            if is_signature || decl_idx == impl_node_idx {
                overload_decls.push(decl_idx);
            }
        }
        if overload_decls.len() < 2 {
            return;
        }

        // TS2385: static method overloads still need the implementation-vs-overload
        // agreement check here. Instance methods get their canonical TS2385s from the
        // duplicate-identifier pass, and re-emitting them here duplicates diagnostics.
        let impl_is_static = self
            .ctx
            .arena
            .get(impl_node_idx)
            .and_then(|node| self.ctx.arena.get_method_decl(node))
            .and_then(|method| method.modifiers.as_ref())
            .is_some_and(|mods| {
                self.ctx
                    .arena
                    .has_modifier_ref(Some(mods), SyntaxKind::StaticKeyword)
            });
        if impl_is_static {
            let get_access = |idx: NodeIndex| -> u8 {
                let Some(node) = self.ctx.arena.get(idx) else {
                    return 0;
                };
                let modifiers = match node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                        .ctx
                        .arena
                        .get_signature(node)
                        .and_then(|s| s.modifiers.as_ref()),
                    _ => None,
                };
                let Some(mods) = modifiers else {
                    return 0;
                };
                if self
                    .ctx
                    .arena
                    .has_modifier_ref(Some(mods), SyntaxKind::PrivateKeyword)
                {
                    1
                } else if self
                    .ctx
                    .arena
                    .has_modifier_ref(Some(mods), SyntaxKind::ProtectedKeyword)
                {
                    2
                } else {
                    0
                }
            };

            let impl_access = get_access(impl_node_idx);
            for &decl_idx in &overload_decls {
                if decl_idx == impl_node_idx {
                    continue;
                }
                let decl_is_static = self
                    .ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| match node.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(node)
                            .and_then(|method| method.modifiers.as_ref()),
                        k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                            .ctx
                            .arena
                            .get_signature(node)
                            .and_then(|sig| sig.modifiers.as_ref()),
                        _ => None,
                    })
                    .is_some_and(|mods| {
                        self.ctx
                            .arena
                            .has_modifier_ref(Some(mods), SyntaxKind::StaticKeyword)
                    });
                if decl_is_static != impl_is_static {
                    continue;
                }
                if get_access(decl_idx) != impl_access {
                    let error_node = self
                        .ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|n| match n.kind {
                            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                self.ctx.arena.get_method_decl(n).map(|m| m.name)
                            }
                            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                                self.ctx.arena.get_signature(n).map(|s| s.name)
                            }
                            _ => None,
                        })
                        .unwrap_or(decl_idx);
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                        diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                    );
                }
            }
        }

        // TS2386: Check optionality consistency
        let get_optional = |idx: NodeIndex| -> bool {
            let Some(node) = self.ctx.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .is_some_and(|m| m.question_token),
                k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                    .ctx
                    .arena
                    .get_signature(node)
                    .is_some_and(|s| s.question_token),
                _ => false,
            }
        };

        let impl_optional = get_optional(impl_node_idx);
        for &decl_idx in &overload_decls {
            if decl_idx == impl_node_idx {
                continue;
            }
            if get_optional(decl_idx) != impl_optional {
                let error_node = self
                    .ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|n| match n.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                            self.ctx.arena.get_method_decl(n).map(|m| m.name)
                        }
                        k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                            self.ctx.arena.get_signature(n).map(|s| s.name)
                        }
                        _ => None,
                    })
                    .unwrap_or(decl_idx);
                self.error_at_node(
                    error_node,
                    diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                    diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                );
            }
        }
    }
}
