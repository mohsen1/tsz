//! Divergent-recursion detection for the type evaluator.
//!
//! `MAX_DEF_DEPTH` and the tail-recursion budget bound the *number* of times a
//! recursive type alias is re-expanded, but not the *size* of each expansion. A
//! recursive alias whose type argument grows on every step — an unbounded
//! template-literal string, a tuple that gains an element, a widening
//! intersection, all reached through an `infer`/conditional wrapper — can build
//! astronomically large types within those budgets and hang the compiler. tsc
//! reports TS2589 ("Type instantiation is excessively deep and possibly
//! infinite") for such aliases; these helpers let the evaluator do the same.

use crate::def::DefId;
use crate::instantiation::instantiate::instantiate_generic;
use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TypeData, TypeId};

use super::evaluate::TypeEvaluator;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Per-step structural-weight ceiling for a single recursive argument.
    ///
    /// Exponential growth (e.g. a template-literal string or span list that
    /// *doubles* every step) reaches an enormous single argument within a couple
    /// dozen steps, so one argument this large is itself proof of divergence.
    /// Catching it per-step keeps the largest type ever constructed small, so the
    /// bail stays fast. Comfortably above any legitimate single recursive
    /// argument.
    const MAX_RECURSIVE_GROWTH_STEP: u64 = 100_000;

    /// New maximum argument weights an alias may reach during the TS2589
    /// detection pass before its self-recursion is treated as divergent. The
    /// detection pass is depth-bounded (`MAX_DEF_DEPTH`) and runs in a single
    /// evaluator, so this only needs to be below that ceiling to fire promptly;
    /// it never sees a *terminating* recursion, which defers under free type
    /// parameters instead of recursing.
    const MAX_DETECTION_GROWTH_STEPS: u32 = 25;

    /// Cheap structural-weight estimate for the divergent-growth detector.
    ///
    /// Measures the dimensions along which recursive type arguments diverge:
    /// concrete string-literal length and generic template-literal span count
    /// (template-literal growth, whether the argument has collapsed to a literal
    /// yet or is still a generic `${A}${A}` that doubles its spans each step),
    /// tuple arity (tuple growth), and union/intersection arity (intersection
    /// growth). Other shapes count as a single unit. Intentionally shallow — one
    /// level for lists/spans — so the estimate stays O(arity) and never itself
    /// walks an exploding type tree.
    fn recursive_growth_weight(&self, type_id: TypeId) -> u64 {
        match self.interner().lookup(type_id) {
            Some(TypeData::Literal(LiteralValue::String(atom))) => {
                self.interner().resolve_atom_ref(atom).as_ref().len() as u64
            }
            Some(TypeData::TemplateLiteral(spans)) => {
                self.interner().template_list(spans).len() as u64
            }
            Some(TypeData::Tuple(list)) => self.interner().tuple_list(list).len() as u64,
            Some(TypeData::Union(list) | TypeData::Intersection(list)) => {
                self.interner().type_list(list).len() as u64
            }
            _ => 1,
        }
    }

    /// Detect divergent recursion as a recursing alias (`def_id`) is re-applied
    /// with a fresh set of arguments. Returns `true` when the recursion is
    /// diverging and the caller should bail with TS2589.
    ///
    /// Three complementary signals:
    ///
    /// 1. A single argument whose structural weight exceeds
    ///    `MAX_RECURSIVE_GROWTH_STEP` is itself proof of runaway *exponential*
    ///    growth (a doubling template-literal) and bails immediately, keeping the
    ///    largest type ever built small. Stateless, so it is robust to the
    ///    recursion spanning fresh `TypeEvaluator` instances.
    /// 2. During the TS2589 detection pass, a sustained run of new maximum
    ///    argument weights for `def_id` (`MAX_DETECTION_GROWTH_STEPS`) marks the
    ///    alias as unconditionally divergent and anchors TS2589 at its definition.
    ///    The detection pass runs in one evaluator and only drives *unconditional*
    ///    recursion, so a terminating alias (whose condition defers under free
    ///    parameters) never reaches this tracker.
    /// 3. Otherwise each growing argument's weight is charged to the interner's
    ///    cross-evaluator instantiation fuel (tsc's `instantiationCount` analog).
    ///    Use-site evaluation of a divergent alias can fragment across fresh
    ///    sub-evaluators that reset per-evaluator state; the shared fuel still
    ///    accumulates and bounds the expansion so it terminates rather than hangs.
    pub(crate) fn detect_recursive_growth(&mut self, def_id: DefId, args: &[TypeId]) -> bool {
        let weight: u64 = args
            .iter()
            .map(|&arg| self.recursive_growth_weight(arg))
            .sum();
        if weight >= Self::MAX_RECURSIVE_GROWTH_STEP {
            return true;
        }
        if self.is_depth_detection_pass() {
            let entry = self.detection_growth_runs.entry(def_id).or_insert((0, 0));
            let (max_weight, new_maxima) = *entry;
            if weight > max_weight {
                let next = new_maxima + 1;
                *entry = (weight, next);
                if next >= Self::MAX_DETECTION_GROWTH_STEPS {
                    return true;
                }
            }
        }
        let charge = u32::try_from(weight).unwrap_or(u32::MAX);
        self.interner().consume_evaluation_fuel(charge)
    }

    /// Instantiate an Application type WITHOUT recursively evaluating the result.
    ///
    /// For tail-call optimization in conditional types: expands `TrimLeft<T>`
    /// to its body with args substituted, but does NOT call `evaluate()` on
    /// the result. This avoids incrementing the depth guard, allowing the
    /// tail-call loop in `evaluate_conditional` to handle the result directly.
    ///
    /// Returns `Some(instantiated_body)` if the type is an Application that
    /// could be instantiated. Returns `None` if the type is not an Application,
    /// or if it couldn't be resolved/instantiated.
    ///
    /// The per-`DefId` depth guard is deliberately *not* incremented here so
    /// convergent tail recursion (e.g. `Trim`, which needs 128+ iterations for
    /// long strings) is not capped at `MAX_DEF_DEPTH`. `detect_recursive_growth`
    /// bounds the complementary *divergent* case instead.
    pub(crate) fn try_instantiate_application_for_tail_call(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let app_id = match self.interner().lookup(type_id) {
            Some(TypeData::Application(app_id)) => app_id,
            _ => return None,
        };

        let app = self.interner().type_application(app_id);

        let base_key = self.interner().lookup(app.base)?;
        let def_id = match base_key {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref),
            _ => None,
        }?;

        let type_params = self.resolver().get_lazy_type_params(def_id)?;
        let resolved = self.resolver().resolve_lazy(def_id, self.interner())?;

        let body_is_conditional_with_app_infer =
            self.is_conditional_with_application_infer(resolved);
        let expanded_args: std::borrow::Cow<'_, [TypeId]> = if body_is_conditional_with_app_infer {
            std::borrow::Cow::Owned(self.expand_type_args_preserve_applications(&app.args))
        } else {
            self.expand_type_args(&app.args)
        };

        // Bail with TS2589 when the recursive argument is diverging rather than
        // building an ever-larger type.
        if self.detect_recursive_growth(def_id, &expanded_args) {
            self.mark_depth_exceeded();
            return Some(TypeId::ERROR);
        }

        // Instantiate the body with the type arguments — but do NOT evaluate.
        let instantiated =
            instantiate_generic(self.interner(), resolved, &type_params, &expanded_args);
        Some(instantiated)
    }
}
