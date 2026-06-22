//! Indexed-access helpers for generic inference constraints.

use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::TypeId;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Reduce an indexed-access type `object[index]` to its member type during
    /// inference constraint collection.
    ///
    /// The bare-interner reduction (`evaluate_index_access`) cannot resolve an
    /// object that is itself a `Lazy`/`Application` of a generic type, because it
    /// runs without a resolver: `Ord<A>['compare']` (a parameter type during a
    /// generic call) is left unevaluated, so the access never exposes its member
    /// type and no candidate is collected for `A`, which then collapses to its
    /// default (`unknown`) — the inference half of #14261.
    ///
    /// When the bare reduction makes no progress, expand the object through the
    /// checker's resolver via [`AssignabilityChecker::expand_type_alias_application`],
    /// which instantiates the generic body while *preserving* inference
    /// placeholders (rather than collapsing them to their constraints, as a full
    /// evaluation would), and re-index the expanded object. For `Ord<A>` this
    /// yields `{ compare: (first: A, second: A) => Ordering }['compare']`, which
    /// the bare reducer then resolves to `(first: A, second: A) => Ordering`,
    /// exposing the inference site for `A`. Returns `original` unchanged when no
    /// reduction is possible, matching the prior no-progress contract callers
    /// already guard with `evaluated != original`.
    pub(super) fn reduce_index_access_for_inference(
        &mut self,
        original: TypeId,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        let evaluated = self.interner.evaluate_index_access(object_type, index_type);
        if evaluated != original {
            return evaluated;
        }
        if let Some(expanded_object) = self.checker.expand_type_alias_application(object_type)
            && expanded_object != object_type
        {
            return self
                .interner
                .evaluate_index_access(expanded_object, index_type);
        }
        // No reduction was possible; return the unchanged indexed access (which
        // equals `evaluated` here) so the caller's `!= original` guard reads it
        // as no progress.
        original
    }
}
