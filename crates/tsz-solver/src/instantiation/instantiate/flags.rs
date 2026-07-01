//! Env and test gates for instantiation policy.

use std::cell::Cell;
use std::sync::OnceLock;

static INST_RESOLVER_REREDUCE_ENV: OnceLock<bool> = OnceLock::new();

/// Global re-reduce recursion-depth budget (#14346).
///
/// The `TSZ_INST_RESOLVER_REREDUCE` re-reduce is a cross-arena `URItoKindN`
/// SCC with no fixpoint: each re-reduce turn interns a strictly larger type and
/// re-enters the deferred-leak sites (index-access, `keyof`, conditional,
/// return-context substitution) through a chain of native frames spread across
/// several modules. Per-def and per-`(source,target)` visited-set guards do not
/// bound it (per-def nesting never exceeds 3; the grown pair is always fresh),
/// so unbounded native stack growth overflows.
///
/// This thread-local counts the live native re-reduce depth across ALL
/// flag-gated re-reduce entry points. Each site enters via
/// [`rereduce_depth_try_enter`], which returns a scope guard while depth is
/// below the cap and `None` once the budget is exhausted; the
/// site then bails to its deferred (flag-OFF) form. The guard decrements on
/// `Drop`, so it survives `run_instantiator` returns and panic unwinding, and
/// the counter always reflects true live re-reduce depth.
///
/// The cap is BELOW the observed overflow depth (~503 native frames) and ABOVE
/// the legitimate #15055 clears (small, few hops). The default is overridable
/// via `TSZ_REREDUCE_DEPTH_CAP` for cap tuning; it only widens/narrows the
/// depth budget (a counter-only knob, no name/file string checks).
///
/// Cap chosen from the fp-ts sweep on `TSZ_INST_RESOLVER_REREDUCE=1` (#14346):
/// the crash is UNBOUNDED TYPE GROWTH (each cross-arena `URItoKindN` re-reduce
/// turn interns a strictly larger type, then re-evaluates it), so evaluation
/// cost is super-linear in the depth reached. `N=4` completes (exit 2, ~50s,
/// the #15055 `ReaderEither`/`ReaderTaskEither` TS2322 clears preserved) while
/// `N=8/16/32` never terminate within the run budget (the grown types become
/// too expensive to evaluate). `N=4` sits comfortably below the native-frame
/// overflow while still allowing the small, few-hop legitimate re-reduces.
pub(crate) const REREDUCE_DEPTH_CAP_DEFAULT: u32 = 4;

fn rereduce_depth_cap() -> u32 {
    static CAP: OnceLock<u32> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("TSZ_REREDUCE_DEPTH_CAP")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(REREDUCE_DEPTH_CAP_DEFAULT)
    })
}

thread_local! {
    static REREDUCE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// RAII guard for one live re-reduce recursion frame. Increments the
/// thread-local depth on construction and decrements it on `Drop` (so it
/// survives early returns and panic unwind).
pub(crate) struct RereduceDepthGuard {
    _private: (),
}

impl Drop for RereduceDepthGuard {
    fn drop(&mut self) {
        REREDUCE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Try to enter one re-reduce recursion frame.
///
/// Returns `Some(guard)` (and increments the depth) when the live re-reduce
/// depth is below [`REREDUCE_DEPTH_CAP_DEFAULT`] (or the
/// `TSZ_REREDUCE_DEPTH_CAP` override). Returns `None` — WITHOUT incrementing
/// — once the budget is exhausted, signalling the caller to bail to its
/// deferred (flag-OFF) form. Callers must hold the returned guard for the whole
/// recursive re-reduce call so the counter reflects true native depth.
pub(crate) fn rereduce_depth_try_enter() -> Option<RereduceDepthGuard> {
    let cap = rereduce_depth_cap();
    REREDUCE_DEPTH.with(|d| {
        let depth = d.get();
        if depth >= cap {
            return None;
        }
        d.set(depth + 1);
        Some(RereduceDepthGuard { _private: () })
    })
}

#[cfg(test)]
thread_local! {
    static INST_RESOLVER_REREDUCE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Test guard for forcing the resolver-aware re-reduce flag on one thread.
#[cfg(test)]
pub(crate) struct InstResolverRereduceFlagGuard {
    previous: Option<bool>,
}

#[cfg(test)]
impl InstResolverRereduceFlagGuard {
    pub(crate) fn new(enabled: bool) -> Self {
        let previous = INST_RESOLVER_REREDUCE_TEST_OVERRIDE.with(|slot| {
            let previous = slot.get();
            slot.set(Some(enabled));
            previous
        });
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for InstResolverRereduceFlagGuard {
    fn drop(&mut self) {
        INST_RESOLVER_REREDUCE_TEST_OVERRIDE.with(|slot| slot.set(self.previous));
    }
}

/// #14345 dormant resolver-aware re-reduce of instantiation-time deferred
/// index-access / conditional leaks (default OFF; byte-parity when OFF).
///
/// When `TSZ_INST_RESOLVER_REREDUCE=1`, the instantiator keeps the
/// resolver-aware [`QueryDatabase`](crate::caches::db::QueryDatabase) it was
/// handed and, at the deferred-leak sites, re-runs reduction through that
/// resolver once the base/check became concrete. The flag stays OFF until the
/// materialize-once stages provide the fully published bodies those reductions
/// need.
pub(crate) fn inst_resolver_rereduce_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = INST_RESOLVER_REREDUCE_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }

    *INST_RESOLVER_REREDUCE_ENV
        .get_or_init(|| std::env::var("TSZ_INST_RESOLVER_REREDUCE").is_ok_and(|v| v == "1"))
}

/// `TSZ_OPTIONB_STORE_RESOLVER=1` activates the option-B store-only resolver
/// shim at the instantiation-time index-access re-reduce (issue #14344 /
/// #14345). Default-OFF, byte-parity when OFF.
///
/// This is distinct from [`inst_resolver_rereduce_enabled`] so the gauge can
/// isolate the shim's effect: the rereduce flag is part of both the composed
/// baseline and the option-B configuration, while this flag toggles only the
/// store-only resolver that materializes a cross-arena `Lazy(URItoKindN)` base
/// to its empty-object snapshot.
pub(crate) fn optionb_store_resolver_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_OPTIONB_STORE_RESOLVER").is_ok_and(|v| v == "1"))
}

/// `TSZ_INFER_HKT_REDUCE=1` activates the generic-call inference HKT-reduce
/// lever (issue #14344 / #14345). Default-OFF, byte-parity when OFF.
///
/// Requires the resolver-rereduce and option-B store-only resolver gates plus
/// an attached `DefinitionStore`; callers keep their literal resolver path
/// when any gate is OFF.
pub(crate) fn infer_hkt_reduce_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_INFER_HKT_REDUCE").is_ok_and(|v| v == "1"))
}
