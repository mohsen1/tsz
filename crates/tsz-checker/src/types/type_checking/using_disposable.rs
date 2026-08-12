//! `using` / `await using` declaration disposability checking.
//!
//! Extracted from `type_checking::core` (which owns the surrounding statement
//! validation) so the self-contained disposable-relation concern lives in one
//! place. The public entry point is
//! [`CheckerState::check_using_declaration_disposable`], invoked from the
//! variable-declaration walk in `core`.
//!
//! Structural rule (#16872): a failed sync `using` initializer does not always
//! report `TS2850`. The check runs the ordinary assignability relation against
//! the global `Disposable` interface and lets the *shape* of that failure pick
//! the diagnostic — see [`DisposableInitializerFailure`].

use crate::error_reporter::RelatedInformationPolicy;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// How a failed sync `using` initializer maps onto `tsc`'s diagnostic.
///
/// `tsc` does not always report `TS2850` for a non-disposable `using`
/// initializer. It runs the ordinary assignability relation against the global
/// `Disposable` interface and lets the *shape* of that failure choose the code:
///
/// - A single plain object type (interface, class, or object literal) that is
///   missing the `[Symbol.dispose]` member entirely falls through to the
///   natural missing-property error, so the primary diagnostic is `TS2741`
///   (`Property '[Symbol.dispose]' is missing … but required in type
///   'Disposable'.`) with no `TS2850` at all.
/// - Everything else — a member that is present but structurally incompatible,
///   or a composite source (union / intersection / type parameter) whose
///   constituents each miss the member — keeps the `TS2850` head message and
///   nests the relation's reason beneath it as the elaboration tail.
/// - A bare whole-type mismatch that carries no deeper structural reason (e.g.
///   a primitive initializer such as `using x = 42`) reports the flat `TS2850`
///   with no tail, mirroring `tsc` dropping the generic
///   `Type 'S' is not assignable to type 'T'.` frame the `TS2850` head replaces.
enum DisposableInitializerFailure {
    /// Plain object source missing the member: emit the rendered `TS2741`
    /// (or whatever specific self-heading reason the relation produced) as the
    /// primary diagnostic instead of `TS2850`.
    NaturalRelationError {
        code: u32,
        message: String,
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    },
    /// Structural incompatibility or composite source: keep `TS2850` and attach
    /// the relation reason as the elaboration tail.
    Ts2850WithTail(Vec<crate::diagnostics::DiagnosticRelatedInformation>),
    /// No usable structural reason: emit the flat `TS2850`.
    Ts2850Flat,
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
            // `tsc` runs `checkTypeAssignableTo(initType, Disposable)` and lets the
            // shape of that failure choose the diagnostic. For sync `using` we
            // classify the relation reason (see `DisposableInitializerFailure`):
            // a plain object missing the member reports the natural `TS2741`
            // directly, a structural/composite failure keeps `TS2850` with the
            // relation tail, and a bare mismatch reports the flat `TS2850`.
            // `await using` (TS2851) always carries the flat top line in `tsc`.
            if is_await_using {
                self.error_at_node(initializer, message, code);
            } else {
                match self.classify_disposable_initializer_failure(init_type, initializer) {
                    DisposableInitializerFailure::NaturalRelationError {
                        code: natural_code,
                        message: natural_message,
                        related,
                    } => {
                        self.error_at_node_with_related(
                            initializer,
                            &natural_message,
                            natural_code,
                            related,
                        );
                    }
                    DisposableInitializerFailure::Ts2850WithTail(related) => {
                        self.error_at_node_with_related(initializer, message, code, related);
                    }
                    DisposableInitializerFailure::Ts2850Flat => {
                        self.error_at_node(initializer, message, code);
                    }
                }
            }
        }
    }

    /// Classify a failed sync `using` initializer against the global
    /// `Disposable` interface, mirroring `tsc`'s
    /// `checkTypeAssignableTo(initType, Disposable)` and letting the *shape* of
    /// the relation failure pick the diagnostic — see
    /// [`DisposableInitializerFailure`]. Returns `Ts2850Flat` when the global
    /// `Disposable` interface is unavailable or the relation reports no
    /// structured reason (matching `tsc`, which then shows only the flat head).
    fn classify_disposable_initializer_failure(
        &mut self,
        init_type: TypeId,
        anchor: NodeIndex,
    ) -> DisposableInitializerFailure {
        let Some(disposable_type) = self.resolve_disposable_interface_type(false) else {
            return DisposableInitializerFailure::Ts2850Flat;
        };
        // Widen freshness before running the relation, mirroring the gate in
        // `type_has_disposable_method`: `tsc` never excess-property-checks a
        // `using` initializer (#16862), so a fresh object literal that carries a
        // *signature-incompatible* `[Symbol.dispose]` alongside extra properties
        // must elaborate the signature failure, not a leaked "Object literal may
        // only specify known properties" tail. The gate and the tail must see
        // the same regular type or they disagree on the fresh-plus-extra case.
        let init_type = crate::query_boundaries::common::widen_freshness(self.ctx.types, init_type);
        let analysis = self.analyze_assignability_failure(init_type, disposable_type);
        let Some(reason) = analysis.failure_reason else {
            return DisposableInitializerFailure::Ts2850Flat;
        };
        let rendered = self.render_failure_reason(&reason, init_type, disposable_type, anchor, 0);
        // `tsc` runs the ordinary assignability relation and lets its result
        // choose the code. Three cases, keyed on how `render_failure_reason`
        // shaped the reason at depth 0 (which mirrors `tsc`'s error chain):
        //
        // 1. A *single plain object type* missing `[Symbol.dispose]` entirely
        //    self-heads with the missing-property reason (code TS2741,
        //    `Property '[Symbol.dispose]' is missing … but required in type
        //    'Disposable'.`). `tsc` reports that reason as the **primary**
        //    diagnostic — there is no TS2850 at all. (`{ notDispose() {} }`,
        //    `declare const x: { foo: number }`, a class instance, an
        //    interface all take this path.)
        //
        // 2. A member that is present but structurally incompatible, or a
        //    *composite* source (union / intersection / type parameter, whose
        //    constituents each miss the member), renders `tsc`'s generic outer
        //    `Type 'S' is not assignable to type 'T'.` frame at the top with a
        //    deeper chain beneath it. The TS2850 head message replaces that
        //    generic frame, so keep TS2850 and promote the already-nested
        //    children directly beneath it as the tail. Any other self-heading
        //    *non-missing-property* reason (defensive) is likewise nested under
        //    TS2850, one level deeper.
        //
        // 3. A bare whole-type mismatch with no deeper chain (e.g. a primitive
        //    initializer such as `using x = 42`, which renders as a childless
        //    `Type 'number' is not assignable to type 'Disposable'.`) leaves
        //    nothing once the generic frame is replaced, so `tsc` shows the flat
        //    TS2850 with no tail.
        //
        // A self-heading reason that is neither of those (defensive; the
        // single-property `Disposable` target always wraps a member
        // incompatibility in the generic frame) is nested beneath TS2850 with
        // the shared `WRAPPED_DIAGNOSTIC` policy, the same demotion
        // `assignability_satisfies` uses for a head-message-wrapped failure.
        use crate::diagnostics::diagnostic_codes;
        match rendered.code {
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE => {
                DisposableInitializerFailure::NaturalRelationError {
                    code: rendered.code,
                    message: rendered.message_text,
                    related: rendered.related_information,
                }
            }
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                if rendered.related_information.is_empty() =>
            {
                DisposableInitializerFailure::Ts2850Flat
            }
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE => {
                DisposableInitializerFailure::Ts2850WithTail(rendered.related_information)
            }
            _ => {
                let related = self.related_from_diagnostic(
                    &rendered,
                    RelatedInformationPolicy::WRAPPED_DIAGNOSTIC,
                );
                DisposableInitializerFailure::Ts2850WithTail(related)
            }
        }
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
            .is_some_and(|target| {
                self.disposable_resource_relation_outcome(source, target)
                    .related
            });

        if is_await_using {
            // await using accepts either Symbol.asyncDispose or Symbol.dispose
            return is_disposable
                || self
                    .resolve_disposable_interface_type(true)
                    .is_some_and(|target| {
                        self.disposable_resource_relation_outcome(source, target)
                            .related
                    });
        }

        is_disposable
    }
}
