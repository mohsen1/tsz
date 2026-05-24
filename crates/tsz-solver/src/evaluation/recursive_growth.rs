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
}
