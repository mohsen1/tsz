use crate::query_boundaries::dispatch::is_type_parameter_like;

use crate::query_boundaries::type_checking_utilities as query;

use crate::state::{CheckerState, EnumKind, MemberAccessLevel};

use rustc_hash::FxHashMap;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use super::cycle_guard::{self, CycleSetId};

thread_local! {
    static EVAL_MEMO: std::cell::RefCell<rustc_hash::FxHashMap<NodeIndex, Option<f64>>>
        = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

thread_local! {
    static EVAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

const MAX_EVAL_DEPTH: u32 = 100;

/// Clear the enum evaluation memo cache and reset depth.
/// Called between compilation sessions to prevent stale NodeIndex-keyed results.
pub(crate) fn clear_enum_eval_memo() {
    EVAL_MEMO.with(|m| m.borrow_mut().clear());
    EVAL_DEPTH.with(|d| d.set(0));
}

struct DepthGuard;

impl Drop for DepthGuard {
    fn drop(&mut self) {
        EVAL_DEPTH.with(|d| {
            let new_depth = d.get().saturating_sub(1);
            d.set(new_depth);
            if new_depth == 0 {
                // Clear memoization cache at the end of the top-level evaluation
                // to avoid stale results across unrelated evaluation chains.
                EVAL_MEMO.with(|m| m.borrow_mut().clear());
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveOverlapKind {
    String,
    Number,
    BigInt,
    Boolean,
    Symbol,
}

#[derive(Clone, Copy, Debug)]
enum SimpleOverlapType {
    Primitive(PrimitiveOverlapKind),
    StringLiteral(tsz_common::interner::Atom),
    NumberLiteral(f64),
    BigIntLiteral(tsz_common::interner::Atom),
    BooleanLiteral(bool),
}

#[derive(Clone, Debug, PartialEq)]
enum EnumCompatValue {
    Number(f64),
    String(String),
}

include!("enum_utils_parts/part1.rs");
include!("enum_utils_parts/part2.rs");
