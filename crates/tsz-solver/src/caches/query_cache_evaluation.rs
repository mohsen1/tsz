//! Cache-aware evaluation helpers for `QueryCache`.

use crate::caches::db::TypeDatabase;
use crate::caches::query_cache::QueryCache;
use crate::def::resolver::NoopResolver;
use crate::evaluation::evaluate::TypeEvaluator;

/// A `query_db`-backed evaluator: a `TypeEvaluator` with the `NoopResolver`
/// (matching `evaluate_type_with_options`) whose cross-call caches are wired to
/// a `QueryCache`. See [`QueryCache::query_backed_evaluator`].
pub(crate) type QueryBackedEvaluator<'a> = TypeEvaluator<'a, NoopResolver>;

impl<'a> QueryCache<'a> {
    /// Build a `TypeEvaluator` wired to this cache's cross-call instantiation
    /// and application-eval caches.
    ///
    /// The sub-evaluation entry points (`evaluate_conditional`, `evaluate_keyof`,
    /// `evaluate_mapped`, `evaluate_index_access_with_options`) otherwise fall
    /// through to the `QueryDatabase` trait defaults, which construct a fresh
    /// `TypeEvaluator` with `query_db = None`. That strips the cross-call
    /// instantiation cache (`#12019`) at the entry boundary, so recursive
    /// utility expansion re-walks the same `(body, substitution)` pairs on every
    /// call. Threading `self` as the `query_db` lets those entry points share the
    /// same memoized walks the top-level `evaluate_type_with_options` path
    /// already uses. The resolver stays `Noop` to match that path exactly; only
    /// caching behavior changes, never the computed result.
    pub(crate) fn query_backed_evaluator(&self) -> QueryBackedEvaluator<'_> {
        TypeEvaluator::new(self as &dyn TypeDatabase).with_query_db(self)
    }
}
