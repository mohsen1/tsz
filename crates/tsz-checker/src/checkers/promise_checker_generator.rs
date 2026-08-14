//! Generator-iteration helpers split from `promise_checker.rs` to keep that
//! checker shard under the 2000-line size guard. Behavior is unchanged.

use crate::query_boundaries::checkers::promise as query;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Recover a generator-like type argument for a call-result iterable whose
    /// `Generator`/`AsyncGenerator` return type was eagerly materialized into a
    /// structural object, losing the `Generator<Y, R, N>` Application form the
    /// direct extractor reads.
    ///
    /// tsc reads the iteration types (`TYield`/`TReturn`/`TNext`) from this
    /// declared Application, not from the materialized
    /// `next(...[v]: [] | [TNext])` rest-tuple, whose optionality would
    /// otherwise suppress the `TS2763`-`TS2766` send-type diagnostics. The
    /// materialized object still exposes `[Symbol.iterator]()` /
    /// `[Symbol.asyncIterator]()` returning the surviving Application, so we
    /// reach it through the iterator factory's return type and re-extract the
    /// argument. A non-generator iterable (`Set`, `Map`, a user interface that
    /// merely declares `next`) resolves to an iterator that is not a
    /// generator-like Application, so extraction yields `None` and nothing is
    /// forced, matching tsc.
    pub(super) fn recover_generator_arg_from_iterator_factory(
        &mut self,
        type_id: TypeId,
        arg_index: usize,
    ) -> Option<TypeId> {
        for symbol_name in ["[Symbol.iterator]", "[Symbol.asyncIterator]"] {
            let PropertyAccessResult::Success {
                type_id: factory_type,
                ..
            } = self.resolve_property_access_with_env(type_id, symbol_name)
            else {
                continue;
            };
            let iterator_type = self.iterator_factory_return_type(factory_type);
            // Guard against looping back on the same object when the factory
            // returns `this`.
            if iterator_type != type_id
                && let Some(arg) = self.get_generator_arg_direct(iterator_type, arg_index)
            {
                return Some(arg);
            }
        }
        None
    }

    /// Return type of an iterator factory (`[Symbol.iterator]` /
    /// `[Symbol.asyncIterator]`), or `any` when it is not callable.
    fn iterator_factory_return_type(&self, factory_type: TypeId) -> TypeId {
        if factory_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if let Some(shape) = query::function_shape_for_type(self.ctx.types, factory_type) {
            return shape.return_type;
        }
        if let Some(sigs) = query::call_signatures_for_type(self.ctx.types, factory_type) {
            return sigs.first().map_or(TypeId::ANY, |sig| sig.return_type);
        }
        TypeId::ANY
    }

    /// The generator `TReturn` the per-`return;` (bare-return) TS7030 check
    /// compares against, mirroring tsc's
    /// `unwrapReturnType(getReturnTypeOfSignature(func))` in
    /// `isUnwrappedReturnTypeUndefinedVoidOrAny` / `checkReturnStatement`.
    ///
    /// tsc unwraps the signature's `Generator<Y, R, N>` to its `TReturn`
    /// iteration type; when that iteration type cannot be determined
    /// (`getIterationTypeOfGeneratorFunctionReturnType` returns nothing) it
    /// yields `errorType`, an `any`, which *suppresses* TS7030. That is the
    /// crucial difference from the shared `return_type_for_implicit_return_check`,
    /// which falls back to `unknown` — a sentinel the TS2355/TS2366 paths rely
    /// on but one that `should_skip_no_implicit_return_check` does not treat as
    /// skip-worthy, so a bare `return;` would spuriously report.
    ///
    /// - Annotated generator: `effective_return_type` is the annotation's
    ///   `Generator<Y, R, N>`; `generator_return_completeness` is its extracted
    ///   `TReturn`. When extraction fails (`None`) fall back to `any` (tsc's
    ///   `errorType`) — e.g. `Generator<number>`, whose `TReturn` defaults to
    ///   `any`, or a non-generator annotation.
    /// - Unannotated generator: the `Generator<Y, R, N>` wrapper is synthesized
    ///   only *after* this check runs, so `effective_return_type` already holds
    ///   the inferred `TReturn` directly — it must not be unwrapped again, and
    ///   `generator_return_completeness` is `None` here.
    ///
    /// The caller passes the `TReturn` it already extracted (rather than having
    /// this method re-extract) because
    /// [`Self::generator_return_type_for_implicit_return_check`] is not
    /// idempotent: evaluating a `Generator<Y, R, N>` can expand it into a
    /// structural object and lose the `Application` wrapper that carries
    /// `TReturn`, so a second extraction of the same type can spuriously fail.
    pub(crate) const fn generator_bare_return_check_type(
        &self,
        generator_return_completeness: Option<TypeId>,
        effective_return_type: TypeId,
        has_declared_return: bool,
    ) -> TypeId {
        match generator_return_completeness {
            Some(t) => t,
            None if has_declared_return => TypeId::ANY,
            None => effective_return_type,
        }
    }
}
