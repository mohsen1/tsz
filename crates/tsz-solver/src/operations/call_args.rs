use super::{AssignabilityChecker, CallEvaluator, CallResult};

use crate::operations::iterators::{get_iterator_info, target_has_non_iterable_property_shape};

use crate::types::{ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId};

use crate::utils::{self, TupleRestExpansion};

use rustc_hash::{FxHashMap, FxHashSet};

use std::cell::RefCell;

use tracing::trace;

thread_local! {
    static EVALUATES_VISITED_POOL: RefCell<Option<FxHashSet<crate::TypeId>>> =
        const { RefCell::new(None) };
}

#[inline]
fn with_evaluates_visited<R>(f: impl FnOnce(&mut FxHashSet<crate::TypeId>) -> R) -> R {
    let mut visited = EVALUATES_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    EVALUATES_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

mod string_helpers;

include!("call_args_parts/part1.rs");
include!("call_args_parts/part2.rs");
