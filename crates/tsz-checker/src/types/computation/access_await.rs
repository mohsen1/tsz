//! Await expression type computation and Promise helper types.

use crate::context::TypingRequest;
use crate::query_boundaries::common as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

const MAX_AWAIT_DEPTH: u32 = 10;

impl<'a> CheckerState<'a> {
    /// Get the type of an await expression with contextual typing support.
    ///
    /// Propagate contextual type to await operand.
    ///
    /// When awaiting with a contextual type T (e.g., `const x: T = await expr`),
    /// the operand should receive T | `PromiseLike`<T> as its contextual type.
    /// This allows both immediate values and Promises to be inferred correctly.
    ///
    /// Example:
    /// ```typescript
    /// async function fn(): Promise<Obj> {
    ///     const obj: Obj = await { key: "value" };  // Operand gets Obj | PromiseLike<Obj>
    ///     return obj;
    /// }
    /// ```
    #[allow(dead_code)]
    pub(crate) fn get_type_of_await_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_await_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_await_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) else {
            return TypeId::ERROR;
        };

        // TS2524: 'await' expressions cannot be used in a parameter initializer.
        // Only emit when there are no nearby parse errors (to avoid cascading diagnostics
        // after parser recovery, e.g. `async function f(a = await => x) {}`).
        if self.is_in_default_parameter(idx) && !self.node_has_nearby_parse_error(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
        }

        // Match tsc's special-case for `await(...)` inside sync functions.
        // In these contexts TypeScript treats this as an unresolved identifier use
        // and reports TS2311 instead of await-context diagnostics.
        if !self.ctx.in_async_context()
            && self.ctx.function_depth > 0
            && !self.ctx.binder.is_external_module()
            && self.await_expression_uses_call_like_syntax(idx)
        {
            if let Some((start, _)) = self.get_node_span(idx) {
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION,
                    &["await"],
                );
                self.error_at_position(
                    start,
                    5,
                    &message,
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION,
                );
            }
            return TypeId::ANY;
        }

        // Propagate contextual type to await operand
        // If we have a contextual type T, transform it to T | PromiseLike<T> | Promise<T>
        // Including Promise<T> is critical for generic constructor inference:
        // `const obj: Obj = await new Promise(resolve => ...)` needs the constraint
        // `Promise<__infer_0> <: Promise<Obj>` (same base) to infer T = Obj.
        // Without Promise<T>, we'd only have PromiseLike<Obj> which has a different
        // base and can't be directly unified through type argument matching.
        let operand_request = if let Some(contextual) = request.contextual_type {
            // Skip transformation for error types, any, unknown, or never
            if contextual != TypeId::ANY
                && contextual != TypeId::UNKNOWN
                && contextual != TypeId::NEVER
                && !self.type_contains_error(contextual)
            {
                let promise_like_t = self.get_promise_like_type(contextual);
                let promise_t = self.get_promise_type(contextual);
                let mut members = vec![contextual, promise_like_t];
                if let Some(pt) = promise_t {
                    members.push(pt);
                }
                let union_context = self.ctx.types.factory().union(members);
                request.read().contextual(union_context)
            } else {
                request.read().contextual_opt(None)
            }
        } else {
            request.read().contextual_opt(None)
        };

        // Ensure awaited dynamic imports report TS2712 even when call-expression
        // checking paths skip nested async callback bodies.
        if self.ctx.promise_constructor_diagnostics_required()
            && let Some(import_call_idx) = self.await_operand_dynamic_import_call(unary.expression)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                import_call_idx,
                diagnostic_messages::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                diagnostic_codes::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
            );
        }

        // Get the type of the await operand with transformed contextual type
        // Guard: if the operand is missing (e.g. `await;`), return ANY
        if unary.expression.is_none() {
            return TypeId::ANY;
        }
        let expr_type = self.get_type_of_node_with_request(unary.expression, &operand_request);

        if self
            .await_operand_invalid_thenable_this_type(expr_type)
            .is_some()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA,
                diagnostic_codes::TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA,
            );
        }

        // TS1062: check for self-referencing Promise cycles before unwrapping.
        // Types like `type T1 = 1 | Promise<T1> | T1[]` create infinite cycles
        // when resolving Awaited<T>. Detect this and emit TS1062.
        self.check_self_referencing_promise_cycle(expr_type, idx);

        // Recursively unwrap Promise<T> to get T (simulating Awaited<T>)
        // TypeScript's await recursively unwraps nested Promises.
        // For example: await Promise<Promise<number>> should have type `number`
        let mut current_type = expr_type;
        let mut depth = 0;

        while let Some(inner) = self.promise_like_return_type_argument(current_type) {
            current_type = inner;
            depth += 1;
            if depth > MAX_AWAIT_DEPTH {
                break;
            }
        }
        current_type
    }

    fn await_operand_dynamic_import_call(&self, operand_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(operand_idx)?;

        if let Some(call) = self.ctx.arena.get_call_expr(node)
            && self.is_dynamic_import(call)
        {
            return Some(operand_idx);
        }

        if node.kind != SyntaxKind::ImportKeyword as u16 {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(operand_idx)?.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(parent_node)?;
        if call.expression != operand_idx || !self.is_dynamic_import(call) {
            return None;
        }
        Some(parent_idx)
    }

    fn await_expression_uses_call_like_syntax(&self, idx: NodeIndex) -> bool {
        let Some((start, end)) = self.get_node_span(idx) else {
            return false;
        };
        if end <= start {
            return false;
        }
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };
        source_file
            .text
            .get(start as usize..end as usize)
            .is_some_and(|text| text.starts_with("await("))
    }

    /// Get `PromiseLike`<T> for a given type T.
    ///
    /// Helper function for await contextual typing.
    /// Returns the type application `PromiseLike`<T>.
    ///
    /// If `PromiseLike` is not available in lib files, returns the base type T.
    /// This is a conservative fallback that still allows correct typing.
    pub(crate) fn get_promise_like_type(&mut self, type_arg: TypeId) -> TypeId {
        // Try to resolve PromiseLike from lib files
        if let Some(promise_like_base) = self.resolve_global_interface_type("PromiseLike") {
            // Check if we successfully got a PromiseLike type
            if promise_like_base != TypeId::ANY
                && promise_like_base != TypeId::ERROR
                && promise_like_base != TypeId::UNKNOWN
            {
                // Create PromiseLike<T> application
                return self
                    .ctx
                    .types
                    .application(promise_like_base, vec![type_arg]);
            }
        }

        // Fallback: If PromiseLike is not available, return the base type
        // This allows await to work even without full lib files
        type_arg
    }

    /// Get `Promise`<T> for a given type T.
    ///
    /// Helper for await contextual typing — enables same-base constraint matching
    /// when the await operand is `new Promise(resolve => ...)`.
    /// Returns `None` if `Promise` is not available in lib files.
    pub(crate) fn get_promise_type(&mut self, type_arg: TypeId) -> Option<TypeId> {
        if let Some(promise_base) = self.resolve_global_interface_type("Promise")
            && promise_base != TypeId::ANY
            && promise_base != TypeId::ERROR
            && promise_base != TypeId::UNKNOWN
        {
            return Some(self.ctx.types.application(promise_base, vec![type_arg]));
        }
        None
    }

    /// TS1062: detect self-referencing Promise types that would create infinite
    /// cycles when resolving `Awaited<T>`.
    ///
    /// Types like `type T1 = 1 | Promise<T1> | T1[]` contain a cycle through
    /// Promise's fulfillment callback. tsc emits TS1062 at the `await` expression
    /// when this cycle is detected.
    fn check_self_referencing_promise_cycle(&mut self, type_id: TypeId, error_node: NodeIndex) {
        let mut visited_def_ids = rustc_hash::FxHashSet::default();
        if self.has_promise_fulfillment_cycle(type_id, &mut visited_def_ids, 0) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                error_node,
                diagnostic_messages::TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN,
                diagnostic_codes::TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN,
            );
        }
    }

    /// Recursive check for Promise fulfillment cycles.
    ///
    /// Tracks visited `DefIds` (type alias identities) rather than `TypeIds`, since
    /// a recursive type alias like `type T1 = 1 | Promise<T1>` produces
    /// different `TypeIds` at different evaluation stages but shares the same DefId.
    fn has_promise_fulfillment_cycle(
        &mut self,
        type_id: TypeId,
        visited_defs: &mut rustc_hash::FxHashSet<tsz_solver::def::DefId>,
        depth: u32,
    ) -> bool {
        if depth > 10 {
            return false;
        }

        // If this type is a Lazy(DefId), check for DefId cycle and resolve its body
        if let query::PromiseTypeKind::Lazy(def_id) =
            query::classify_promise_type(self.ctx.types, type_id)
        {
            if !visited_defs.insert(def_id) {
                return true;
            }
            if let Some(body) = self.ctx.definition_store.get_body(def_id) {
                return self.has_promise_fulfillment_cycle(body, visited_defs, depth + 1);
            }
            return false;
        }

        // Try to evaluate the type to get concrete structure
        let evaluated = self.evaluate_type_with_env(type_id);
        let target = if evaluated != type_id {
            evaluated
        } else {
            type_id
        };

        // Also check if the evaluated form reveals a Lazy(DefId)
        if target != type_id
            && let query::PromiseTypeKind::Lazy(def_id) =
                query::classify_promise_type(self.ctx.types, target)
        {
            if !visited_defs.insert(def_id) {
                return true;
            }
            if let Some(body) = self.ctx.definition_store.get_body(def_id) {
                return self.has_promise_fulfillment_cycle(body, visited_defs, depth + 1);
            }
            return false;
        }

        match query::classify_promise_type(self.ctx.types, target) {
            query::PromiseTypeKind::Union(members) => {
                for member in members {
                    if let Some(inner) = self.promise_like_return_type_argument(member)
                        && self.has_promise_fulfillment_cycle(inner, visited_defs, depth + 1)
                    {
                        return true;
                    }
                }
            }
            _ => {
                if let Some(inner) = self.promise_like_return_type_argument(target)
                    && self.has_promise_fulfillment_cycle(inner, visited_defs, depth + 1)
                {
                    return true;
                }
            }
        }

        false
    }
}
