//! `using`/`await using` declaration validation (TS2804, TS2803, TS2850,
//! TS2851): checking the initializer type for a required dispose method and
//! reporting the relation-derived failure. Split out of `core.rs` to stay
//! under the checker `src` file-size boundary.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Outcome of routing a failed sync `using` initializer through the shared
/// assignability gateway against `Disposable`.
enum DisposableRelationOutcome {
    /// Keep the `using`-specific TS2850 head message and attach this as its
    /// nested elaboration tail.
    Elaborated(Vec<crate::diagnostics::DiagnosticRelatedInformation>),
    /// Replace the TS2850 head message entirely with this diagnostic — `tsc`
    /// does this when the relation failure is a bare missing-member leaf
    /// (its own self-contained TS2741/TS2739), not a nested mismatch.
    Replaced(crate::diagnostics::Diagnostic),
}

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
            // `tsc` runs `checkTypeAssignableTo(initType, Disposable)` and attaches
            // the relation's failure reason as the nested elaboration (e.g.
            // `Property '[Symbol.dispose]' is missing in type '{ foo: number; }' but
            // required in type 'Disposable'.`). For sync `using` we mirror that by
            // routing through the shared assignability gateway so the tail is the
            // real relation reason rather than a hand-built string. `await using`
            // (TS2851) carries no tail in `tsc`, so it keeps the flat top line.
            match (!is_await_using)
                .then(|| self.disposable_relation_outcome(init_type, initializer))
                .flatten()
            {
                Some(DisposableRelationOutcome::Elaborated(related)) => {
                    self.error_at_node_with_related(initializer, message, code, related);
                }
                Some(DisposableRelationOutcome::Replaced(replacement)) => {
                    self.ctx.push_diagnostic(replacement);
                }
                None => {
                    self.error_at_node(initializer, message, code);
                }
            }
        }
    }

    /// Build the relation-derived outcome for a failed sync `using`
    /// initializer, mirroring `tsc`'s `checkTypeAssignableTo(initType, Disposable)`
    /// nested reason. Returns `None` when the global `Disposable` interface is
    /// unavailable or the relation reports no structured failure reason, in which
    /// case the caller emits the flat top-line message alone (matching `tsc`,
    /// which also drops the tail when it has no relation reason to show).
    fn disposable_relation_outcome(
        &mut self,
        init_type: TypeId,
        anchor: NodeIndex,
    ) -> Option<DisposableRelationOutcome> {
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
        // `tsc`'s relation reporter (`reportRelationError`) only NESTS the
        // structured failure reason beneath the `using`-specific head message
        // (TS2850/`checkTypeAssignableTo(initType, Disposable, headMessage)`)
        // when that reason itself elaborates a deeper mismatch. When the
        // *entire* failure is that the disposable member is absent outright —
        // `SubtypeFailureReason::MissingProperty`/`MissingProperties`, which
        // renders as its own self-contained top-level message (TS2741/TS2739)
        // — tsc promotes that message and REPLACES the head message rather than
        // nesting it: `using r = { foo: 1 }` (member missing) reports only
        // `Property '[Symbol.dispose]' is missing … required in type
        // 'Disposable'.`, no `TS2850` line at all, verified against the pinned
        // `typescript@7.0.2` oracle. A present-but-incompatible member (wrong
        // type, wrong arity) is a `PropertyTypeMismatch` with its own nested
        // chain, not a bare `MissingProperty`, and keeps the TS2850 head with a
        // nested tail (`using_type_mismatch_dispose_signature_attaches_incompatible_tail`).
        if matches!(
            reason,
            tsz_solver::SubtypeFailureReason::MissingProperty { .. }
                | tsz_solver::SubtypeFailureReason::MissingProperties { .. }
        ) {
            let rendered =
                self.render_failure_reason(&reason, init_type, disposable_type, anchor, 0);
            return Some(DisposableRelationOutcome::Replaced(rendered));
        }
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
        let related = if rendered.code
            == crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && !rendered.related_information.is_empty()
        {
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
        Some(DisposableRelationOutcome::Elaborated(related))
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
