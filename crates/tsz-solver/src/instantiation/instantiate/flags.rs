//! Env and test gates for instantiation policy.

use std::sync::OnceLock;

static INST_RESOLVER_REREDUCE_ENV: OnceLock<bool> = OnceLock::new();

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
