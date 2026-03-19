use tsz_solver::TypeId;

use super::{ContextualOrigin, FlowIntent, TypingRequest};

/// Compact request key for audited request-aware caches.
///
/// This intentionally captures only the request dimensions that are known to
/// change expression results in the migrated paths. Ambient checker state must
/// stay on explicit bypass paths until it is made explicit or audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestCacheKey {
    pub contextual_type: Option<TypeId>,
    pub flow: FlowIntent,
    pub origin: ContextualOrigin,
}

impl RequestCacheKey {
    #[inline]
    pub const fn from_request(request: &TypingRequest) -> Option<Self> {
        if request.contextual_type.is_some()
            || !matches!(request.flow, FlowIntent::Read)
            || !matches!(request.origin, ContextualOrigin::Normal)
        {
            return Some(Self {
                contextual_type: request.contextual_type,
                flow: request.flow,
                origin: request.origin,
            });
        }
        None
    }
}

/// Internal counters for request-aware caching and cache-clear churn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestCacheCounters {
    pub request_cache_hits: u64,
    pub request_cache_misses: u64,
    pub contextual_cache_bypasses: u64,
    pub clear_type_cache_recursive_calls: u64,
    pub property_access_request_cache_hits: u64,
    pub property_access_request_cache_lookups: u64,
}

impl RequestCacheCounters {
    pub const fn merge(&mut self, other: Self) {
        self.request_cache_hits += other.request_cache_hits;
        self.request_cache_misses += other.request_cache_misses;
        self.contextual_cache_bypasses += other.contextual_cache_bypasses;
        self.clear_type_cache_recursive_calls += other.clear_type_cache_recursive_calls;
        self.property_access_request_cache_hits += other.property_access_request_cache_hits;
        self.property_access_request_cache_lookups += other.property_access_request_cache_lookups;
    }
}
