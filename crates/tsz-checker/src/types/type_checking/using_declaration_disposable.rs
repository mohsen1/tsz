//! `using`/`await using` declaration disposable-method validation (TS2803,
//! TS2804, TS2850, TS2851). Extracted from `core.rs` to keep the checker
//! boundary's per-file line cap.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // --- Using Declaration Validation (TS2804, TS2803) ---

    /// Check if a using/await using declaration's initializer type has the required dispose method.
    ///
    /// ## Parameters
    /// - `decl_idx`: The variable declaration node index
    /// - `is_await_using`: Whether this is an await using declaration
    ///
    /// Checks:
    /// - `using` requires type to have `[Symbol.dispose]()` method
    /// - `await using` requires type to have `[Symbol.asyncDispose]()` or `[Symbol.dispose]()` method
    pub(crate) fn check_using_declaration_disposable(
        &mut self,
        decl_idx: NodeIndex,
        is_await_using: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        if self.ctx.arena.get(var_decl.name).is_some_and(|name| {
            name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        }) {
            return;
        }

        // Skip if no initializer
        if var_decl.initializer.is_none() {
            return;
        }

        // Get the type of the initializer
        let init_type = self.get_type_of_node(var_decl.initializer);

        // Skip error type and any (suppressed by convention)
        if init_type == TypeId::ERROR || init_type == TypeId::ANY {
            return;
        }

        tracing::debug!(
            init_type = init_type.0,
            is_await_using,
            "check_using_declaration_disposable: initializer type"
        );
        // Check for the required dispose method
        let initializer = var_decl.initializer;
        if !self.type_has_disposable_method(init_type, is_await_using) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                    diagnostic_codes::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                )
            } else {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                    diagnostic_codes::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                )
            };
            // `tsc` runs `checkTypeAssignableTo(initType, Disposable, headMessage =
            // TS2850)`. When the initializer is object-like and the *only* problem
            // is that `[Symbol.dispose]` is absent entirely, `tsc` drops the
            // `headMessage` frame and reports the relation's own missing-property
            // diagnostic (TS2741) directly — the initializer IS an object, it just
            // lacks the member, so TS2850's "must be either an object with a
            // '[Symbol.dispose]()' method, or be 'null' or 'undefined'" wording
            // does not apply. Any other failure shape (wrong type/arity on a
            // present member, or an initializer that isn't object-like at all)
            // keeps the fixed TS2850 head with the relation reason nested beneath
            // it as elaboration, matching `tsc`.
            if !is_await_using
                && let Some(diag) =
                    self.disposable_missing_property_diagnostic(init_type, initializer)
            {
                self.push_prebuilt_diagnostic(diag);
            } else if !is_await_using
                && let Some(related) = self.disposable_relation_tail(init_type, initializer)
            {
                self.error_at_node_with_related(initializer, message, code, related);
            } else {
                self.error_at_node(initializer, message, code);
            }
        }
    }

    /// When a sync `using` initializer's only defect is a wholly absent
    /// `[Symbol.dispose]` member (`SubtypeFailureReason::MissingProperty`,
    /// not nested under any other reason), build the relation's own TS2741
    /// diagnostic directly instead of the fixed TS2850 head. Returns `None`
    /// for every other failure shape (present-but-incompatible member,
    /// non-object initializer, or no structured reason at all), leaving
    /// those to `disposable_relation_tail`'s nested-elaboration path.
    ///
    /// `render_missing_property` itself downgrades a *primitive* source
    /// (`using a = 42`) to a generic `TYPE_IS_NOT_ASSIGNABLE_TO_TYPE`
    /// message rather than TS2741 — `tsc` never elaborates a non-object
    /// initializer against `Disposable`'s missing member, it just fails the
    /// whole relation and keeps TS2850's head wording. Checking the
    /// rendered code (rather than the reason variant alone) reuses that
    /// same primitive-source classification instead of duplicating it.
    fn disposable_missing_property_diagnostic(
        &mut self,
        init_type: TypeId,
        anchor: NodeIndex,
    ) -> Option<crate::diagnostics::Diagnostic> {
        let disposable_type = self.resolve_disposable_interface_type(false)?;
        let widened = crate::query_boundaries::common::widen_freshness(self.ctx.types, init_type);
        let analysis = self.analyze_assignability_failure(widened, disposable_type);
        let reason = analysis.failure_reason?;
        if !matches!(
            reason,
            tsz_solver::SubtypeFailureReason::MissingProperty { .. }
        ) {
            return None;
        }
        let rendered = self.render_failure_reason(&reason, widened, disposable_type, anchor, 0);
        (rendered.code
            == crate::diagnostics::diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE)
            .then_some(rendered)
    }

    /// Build the relation-derived elaboration tail for a failed sync `using`
    /// initializer, mirroring `tsc`'s `checkTypeAssignableTo(initType, Disposable)`
    /// nested reason. Returns `None` when the global `Disposable` interface is
    /// unavailable or the relation reports no structured failure reason, in which
    /// case the caller emits the flat top-line message alone (matching `tsc`,
    /// which also drops the tail when it has no relation reason to show).
    fn disposable_relation_tail(
        &mut self,
        init_type: TypeId,
        anchor: NodeIndex,
    ) -> Option<Vec<crate::diagnostics::DiagnosticRelatedInformation>> {
        let disposable_type = self.resolve_disposable_interface_type(false)?;
        // Widen freshness before running the relation, mirroring the gate in
        // `type_has_disposable_method`: `tsc` never excess-property-checks a
        // `using` initializer (#16862), so a fresh object literal that carries a
        // *signature-incompatible* `[Symbol.dispose]` alongside extra properties
        // must elaborate the signature failure, not a leaked "Object literal may
        // only specify known properties" tail. The gate and the tail must see
        // the same regular type or they disagree on the fresh-plus-extra case.
        let init_type = crate::query_boundaries::common::widen_freshness(self.ctx.types, init_type);
        let analysis = self.analyze_assignability_failure(init_type, disposable_type);
        let reason = analysis.failure_reason?;
        let rendered = self.render_failure_reason(&reason, init_type, disposable_type, anchor, 0);
        // `tsc` reports this failure through `checkTypeAssignableTo(initType,
        // Disposable, headMessage = TS2850)`. In its `reportRelationError`, a
        // supplied head message *replaces* the generic outer
        // `Type 'S' is not assignable to type 'T'.` frame rather than nesting
        // beneath it — the TS2850 wording ("must be … an object with a
        // '[Symbol.dispose]()' method …") already conveys that relationship, so
        // the tail drills straight to the specific nested reason.
        //
        // `render_failure_reason` at depth 0 reproduces that generic frame as
        // the rendered *top* message (code `TYPE_IS_NOT_ASSIGNABLE_TO_TYPE`)
        // whenever the reason is an object-structural mismatch that carries a
        // deeper chain (e.g. a `[Symbol.dispose]` whose signature is
        // incompatible). Mirror `tsc`: drop that redundant frame and promote its
        // already-correctly-nested children directly beneath the head message.
        // A self-heading leaf reason instead renders as its own specific message
        // (e.g. `Property '[Symbol.dispose]' is missing … required in type
        // 'Disposable'.`, code TS2741) with no generic frame to replace, so keep
        // it as the first tail line with its own chain one level deeper.
        // A source that isn't object-like at all (e.g. `using a = 42`) fails
        // the whole relation with no further structural detail: `render_missing_property`'s
        // own primitive-source downgrade produces a bare `TYPE_IS_NOT_ASSIGNABLE_TO_TYPE`
        // restating exactly what the TS2850 head already says. `tsc` never
        // elaborates this shape (`elaborateError` only attaches property-level
        // reasons to an object-like source) — drop the redundant tail entirely
        // rather than nest a line that adds no information.
        if rendered.code == crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && rendered.related_information.is_empty()
        {
            return None;
        }
        let top_is_generic_frame = rendered.code
            == crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && !rendered.related_information.is_empty();
        let related = if top_is_generic_frame {
            rendered.related_information
        } else {
            let mut related = vec![crate::diagnostics::Diagnostic::related_message(
                rendered.code,
                rendered.file,
                rendered.start,
                rendered.length,
                rendered.message_text,
            )];
            related.extend(
                rendered
                    .related_information
                    .into_iter()
                    .map(|info| info.with_depth_shift(1)),
            );
            related
        };
        Some(related)
    }

    /// Resolve the `Disposable`/`AsyncDisposable` global interface as a
    /// `Lazy(DefId)` type, primed for lowering exactly as a `: Disposable`
    /// annotation would be. Returns `None` when the current lib does not
    /// declare the interface (e.g. `--lib` without `esnext.disposable`).
    ///
    /// A bare `get_type_of_symbol` here would yield an unresolved/member-less
    /// shell when nothing else in the file references the interface (a
    /// `using` initializer is often its only consumer), so the relation
    /// would see no `[Symbol.dispose]`/`[Symbol.asyncDispose]` member.
    fn resolve_disposable_interface_type(&mut self, want_async: bool) -> Option<TypeId> {
        let name = if want_async {
            "AsyncDisposable"
        } else {
            "Disposable"
        };
        let lib_binders = self.get_lib_binders();
        let sym = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)?;
        let def_id = self.ensure_def_ready_for_lowering(sym, name);
        Some(self.ctx.types.lazy(def_id))
    }

    /// Check if a type has the appropriate dispose method.
    ///
    /// For `using`: checks for `[Symbol.dispose]()`
    /// For `await using`: checks for `[Symbol.asyncDispose]()` or `[Symbol.dispose]()`
    fn type_has_disposable_method(&mut self, type_id: TypeId, is_await_using: bool) -> bool {
        fn has_property(
            state: &mut CheckerState<'_>,
            type_id: TypeId,
            property_names: &[&str],
        ) -> bool {
            property_names.iter().any(|property_name| {
                matches!(
                    state.resolve_property_access_with_env(type_id, property_name),
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                        | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: Some(_),
                            ..
                        }
                )
            })
        }

        // Check intrinsic types
        if type_id == TypeId::ANY
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || type_id == TypeId::NEVER
        {
            return true; // Suppress errors on these types
        }

        // null and undefined can be disposed (no-op)
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return true;
        }

        // Only check for dispose methods if Symbol.dispose is available in the current environment
        // Check by looking for the dispose property on SymbolConstructor
        let symbol_type = self.type_of_value_symbol_by_name("Symbol");

        let symbol_has_dispose = has_property(self, symbol_type, &["dispose"]);

        let symbol_has_async_dispose = has_property(self, symbol_type, &["asyncDispose"]);

        // For await using, we need either Symbol.asyncDispose or Symbol.dispose
        if is_await_using && !symbol_has_async_dispose && !symbol_has_dispose {
            // Symbol.asyncDispose and Symbol.dispose are not available in this lib
            // Don't check for them (TypeScript will emit other errors about missing globals)
            return true;
        }

        // For regular using, we need Symbol.dispose
        if !is_await_using && !symbol_has_dispose {
            // Symbol.dispose is not available in this lib
            // Don't check for it
            return true;
        }

        // Check the object type against the full `Disposable`/`AsyncDisposable`
        // structural shape (`tsc`'s `checkTypeAssignableTo`), not mere property
        // existence — a `[Symbol.dispose]`/`[Symbol.asyncDispose]` method with an
        // incompatible signature (extra required params, wrong parameter types,
        // or for the async case a non-`PromiseLike` return type) must still be
        // rejected, while the ordinary void-return exception (a `(): number`
        // dispose method) must still be accepted, exactly as the relation
        // already implements for every other assignability check.
        // Widen freshness first. A disposable only has to *carry* a
        // `[Symbol.dispose]` method; properties beyond it are fine, and tsc
        // does not excess-property-check a `using` initializer. Passing the
        // fresh literal type straight into the relation turns
        // `using x = { [Symbol.dispose]() {}, extra: 1 }` into a `TS2850`
        // whose tail reads "Object literal may only specify known properties",
        // which is the freshness check leaking into a position tsc never runs
        // it (#16862). The same object bound to a variable first was always
        // accepted, which is the tell. Widening here mirrors tsc reaching this
        // relation with `getRegularTypeOfObjectLiteral`, and leaves every
        // signature-shape rejection above intact.
        let source = crate::query_boundaries::common::widen_freshness(self.ctx.types, type_id);

        let is_disposable = self
            .resolve_disposable_interface_type(false)
            .is_some_and(|target| self.is_assignable_to(source, target));

        if is_await_using {
            // await using accepts either Symbol.asyncDispose or Symbol.dispose
            return is_disposable
                || self
                    .resolve_disposable_interface_type(true)
                    .is_some_and(|target| self.is_assignable_to(source, target));
        }

        is_disposable
    }
}
