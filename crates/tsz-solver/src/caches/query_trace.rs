//! Structured query tracing for solver query entry points.
//!
//! Events use target `tsz::query_json` and are intended to be consumed with:
//! `TSZ_LOG=tsz::query_json=trace TSZ_LOG_FORMAT=json`.
//!
//! Environment:
//! - `TSZ_QUERY_RUN_ID`: optional run identifier attached to every event.

use crate::TypeId;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{Level, trace};

static NEXT_QUERY_ID: AtomicU64 = AtomicU64::new(1);
static QUERY_RUN_ID: OnceLock<String> = OnceLock::new();

#[inline]
pub(crate) fn enabled() -> bool {
    tracing::enabled!(target: "tsz::query_json", Level::TRACE)
}

#[inline]
pub(crate) fn next_query_id() -> u64 {
    NEXT_QUERY_ID.fetch_add(1, Ordering::Relaxed)
}

#[inline]
fn run_id() -> &'static str {
    QUERY_RUN_ID
        .get_or_init(|| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::env::var("TSZ_QUERY_RUN_ID").unwrap_or_else(|_| "default".to_string())
            }
            #[cfg(target_arch = "wasm32")]
            {
                "default".to_string()
            }
        })
        .as_str()
}

#[inline]
pub(crate) fn unary_start(query_id: u64, op: &'static str, input: TypeId, no_unchecked: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "start",
        run_id = run_id(),
        query_id,
        op,
        input_type_id = input.0,
        no_unchecked_indexed_access = no_unchecked
    );
}

#[inline]
pub(crate) fn unary_end(query_id: u64, op: &'static str, result_type: TypeId, cache_hit: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "end",
        run_id = run_id(),
        query_id,
        op,
        result_type_id = result_type.0,
        cache_hit
    );
}

#[inline]
pub(crate) fn relation_start(
    query_id: u64,
    op: &'static str,
    source: TypeId,
    target: TypeId,
    flags: u16,
) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "start",
        run_id = run_id(),
        query_id,
        op,
        source_type_id = source.0,
        target_type_id = target.0,
        flags
    );
}

#[inline]
pub(crate) fn relation_end(query_id: u64, op: &'static str, result: bool, cache_hit: bool) {
    trace!(
        target: "tsz::query_json",
        event = "query",
        phase = "end",
        run_id = run_id(),
        query_id,
        op,
        result,
        cache_hit
    );
}
