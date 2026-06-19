use rustc_hash::FxHashMap;
use tsz_solver::TypeId;

/// Cache key for type-node results resolved under active generic bindings.
///
/// Plain `node_types` entries are keyed only by `NodeIndex`, so they are safe
/// only when no dynamic type-parameter scope is active. When generic bindings
/// are present, the same annotation node can resolve to different `TypeId`s.
/// This key records the active name-to-type binding set in deterministic order
/// so equivalent scopes share work without letting different generic contexts
/// alias each other.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeParameterScopeCacheKey(Vec<(String, TypeId)>);

impl TypeParameterScopeCacheKey {
    #[must_use]
    pub fn from_scope(scope: &FxHashMap<String, TypeId>) -> Option<Self> {
        if scope.is_empty() {
            return None;
        }
        let mut entries: Vec<_> = scope
            .iter()
            .map(|(name, &type_id)| (name.clone(), type_id))
            .collect();
        entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        Some(Self(entries))
    }
}
