use crate::instantiation::instantiate::instantiate_generic_cached;

use super::*;

/// Maximum alias-chain hops when transitively normalising a `Lazy` type arg.
/// TypeScript disallows circular aliases, so real chains are 1–3 levels deep;
/// this ceiling only fires for malformed or pathological input.
const MAX_LAZY_CHAIN_DEPTH: usize = 32;

/// Per-property `(optional, readonly)` modifier map keyed by property-name atom,
/// or `None` for members that contribute no object properties. Used by
/// intersection simplification to AND-merge modifiers when deciding whether a
/// structurally subsumed member can be dropped.
type MemberModifierMap = Option<FxHashMap<u32, (bool, bool)>>;

include!("support_parts/part1.rs");
include!("support_parts/part2.rs");
