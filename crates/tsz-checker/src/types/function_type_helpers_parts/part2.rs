impl<'a> CheckerState<'a> {
    /// Check if a type is a type alias application that resolves to Promise.
    ///
    /// For example, `type PromiseAlias<T> = Promise<T>; async function f(): PromiseAlias<void>`
    /// -- the return type `PromiseAlias<void>` is an Application whose base is a type alias.
    /// This method resolves the alias body and checks if it references the global Promise type.
    ///
    /// This handles tsc's `isReferenceToType` semantics for TS1064, where type aliases
    /// that ultimately resolve to Promise<T> are accepted as valid async return types.
    /// It also handles merged symbols (e.g., `type MyPromise<T> = Promise<T>` combined
    /// with `declare var MyPromise: typeof Promise`) by finding the type alias declaration
    /// among the symbol's declarations.
    pub(crate) fn is_promise_type_through_alias(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::checkers::promise as query;
        use tsz_binder::symbol_flags;

        // Must be an Application type
        let query::PromiseTypeKind::Application { base, .. } =
            query::classify_promise_type(self.ctx.types, type_id)
        else {
            return false;
        };

        // Check if the base is a Lazy(DefId) pointing to a type alias
        let def_id = match query::classify_promise_type(self.ctx.types, base) {
            query::PromiseTypeKind::Lazy(def_id) => def_id,
            _ => return false,
        };

        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only handle type aliases (not classes/interfaces)
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        // Get the alias body type using type_reference_symbol_type_with_params which
        // correctly handles merged symbols (e.g., `type MyPromise<T> = Promise<T>`
        // merged with `declare var MyPromise: typeof Promise`). It finds the type
        // alias declaration in the symbol's declarations list.
        let (body_type, _params) = self.type_reference_symbol_type_with_params(sym_id);
        if self.is_global_promise_type(body_type) {
            return true;
        }

        // The body might itself be an Application (e.g., `Promise<T>`)
        // Check if the Application base refers to the global Promise type
        if let query::PromiseTypeKind::Application {
            base: body_base, ..
        } = query::classify_promise_type(self.ctx.types, body_type)
        {
            // Check if the body's base is Promise
            return self.is_global_promise_type(body_base);
        }

        false
    }

    /// TS2366/TS2355/TS7030: Check that all code paths return a value when required.
    /// For function expressions and arrow functions with return type annotations.
    pub(crate) fn check_function_return_completeness(&mut self, ctx: FunctionReturnCheckCtx) {
        let FunctionReturnCheckCtx {
            is_function_declaration,
            body,
            func_idx,
            annotated_return_type,
            return_type,
            has_type_annotation,
            type_annotation,
            function_is_generator,
            name_node,
            idx,
        } = ctx;
        if is_function_declaration || body.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        // Class methods and constructors have their return completeness checked
        // by ambient_signature_checks.rs during the class checking phase, where
        // enclosing_class is properly set. Skip them here to avoid false
        // positives during the type building phase when enclosing_class is not
        // yet available (needed for `this.method()` never-returning call detection).
        if node.kind == syntax_kind_ext::METHOD_DECLARATION
            || node.kind == syntax_kind_ext::CONSTRUCTOR
        {
            return;
        }
        // Determine if this is an async function or generator
        let (is_async, is_generator) = if let Some(func) = self.ctx.arena.get_function(node) {
            (func.is_async, func.asterisk_token)
        } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
            (
                self.has_async_modifier(&method.modifiers),
                method.asterisk_token,
            )
        } else {
            (false, false)
        };
        let effective_return_type = annotated_return_type.unwrap_or(return_type);
        let mut check_return_type = self.return_type_for_implicit_return_check(
            effective_return_type,
            is_async,
            is_generator,
        );
        // For async functions, suppress return-completeness diagnostics only
        // when the annotation resolves to the actual global Promise. A local
        // or qualified type named Promise still follows normal return checks.
        if is_async
            && check_return_type == effective_return_type
            && has_type_annotation
            && self.return_type_annotation_is_exactly_promise(type_annotation)
        {
            check_return_type = TypeId::VOID;
        }
        let requires_return = self.requires_return_value(check_return_type);
        let has_return = self.body_has_return_with_value(body);
        let falls_through = self.function_body_falls_through(body);
        if has_type_annotation
            && requires_return
            && falls_through
            && check_return_type != TypeId::VOID
        {
            if !has_return {
                self.error_at_node(
                    type_annotation,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else {
                // TS2366: always emit when return type doesn't include undefined
                self.error_at_node(
                    type_annotation,
                    diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
            }
        } else if self.ctx.no_implicit_returns() && has_return && falls_through {
            // TS7030: noImplicitReturns - not all code paths return a value
            // TSC skips TS7030 for functions returning void, any, or unions containing void/any
            let ts7030_check_type = self.return_type_for_implicit_return_check(
                annotated_return_type.unwrap_or(return_type),
                is_async,
                function_is_generator,
            );
            if !self.should_skip_no_implicit_return_check(
                ts7030_check_type,
                has_type_annotation,
                function_is_generator,
            ) {
                // TSC points TS7030 to: return type annotation > function name > node itself
                let error_node = if has_type_annotation {
                    type_annotation
                } else if let Some(nn) = name_node {
                    nn
                } else {
                    idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }
        }
    }

    /// Check if a return context type is or references a const type parameter.
    /// Used to propagate const context into callback bodies during generic inference.
    pub(crate) fn return_context_has_const_type_param(&self, ret_ctx: TypeId) -> bool {
        // Direct check: is the return context itself a const type parameter?
        if let Some(tp_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, ret_ctx)
            && tp_info.is_const
        {
            return true;
        }

        // General check: does the return context reference any const type parameter?
        let referenced =
            crate::query_boundaries::common::collect_referenced_types(self.ctx.types, ret_ctx);
        referenced.into_iter().any(|ty| {
            crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
                .is_some_and(|info| info.is_const)
        })
    }

    pub(crate) fn class_property_arrow_lexical_this_type(
        &mut self,
        arrow_idx: NodeIndex,
    ) -> Option<TypeId> {
        let (property_idx, class_idx) = self.class_property_arrow_owner(arrow_idx)?;
        let property_node = self.ctx.arena.get(property_idx)?;
        let prop = self.ctx.arena.get_property_decl(property_node)?;
        let class_node = self.ctx.arena.get(class_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;
        let is_static = self.has_static_modifier(&prop.modifiers);

        // When the arrow function is itself currently being typed, the arrow
        // node is on `node_resolution_stack`. Triggering a fresh class instance
        // (or constructor) type build here would recursively re-enter
        // `get_class_instance_type_inner`, which in turn calls
        // `get_type_of_node(prop.initializer)` for this same arrow; that
        // re-entry hits the circular-reference guard and poisons the cached
        // class shape. Use the already-cached class type or the enclosing-class
        // snapshot instead.
        if self.ctx.node_resolution_stack.contains(&arrow_idx) {
            let cache = if is_static {
                &self.ctx.class_constructor_type_cache
            } else {
                &self.ctx.class_instance_type_cache
            };
            return cache.get(&class_idx).copied().or_else(|| {
                if is_static {
                    return None;
                }
                self.ctx
                    .enclosing_class
                    .as_ref()
                    .filter(|info| info.class_idx == class_idx)
                    .and_then(|info| info.cached_instance_this_type)
            });
        }

        Some(if is_static {
            self.get_class_constructor_type(class_idx, class_data)
        } else {
            self.get_class_instance_type(class_idx, class_data)
        })
    }

    fn class_property_arrow_owner(&self, arrow_idx: NodeIndex) -> Option<(NodeIndex, NodeIndex)> {
        let mut current = arrow_idx;
        for _ in 0..16 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;

            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let class_idx = self.ctx.arena.get_extended(parent)?.parent;
                let class_node = self.ctx.arena.get(class_idx)?;
                if class_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && class_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    return None;
                }
                return Some((parent, class_idx));
            }

            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent_node.kind == syntax_kind_ext::CONSTRUCTOR
            {
                return None;
            }

            current = parent;
        }

        None
    }
}
