use crate::narrowing::guard::TypeGuard;

/// Stable-key budget for the top-level guard narrowing memo.
///
/// Generation slots are already bounded per stable key by `GenerationMemo`;
/// this cap bounds the number of distinct guard payloads retained when the
/// cache is widened beyond predicate guards.
pub(crate) const MAX_NARROW_TYPE_CACHE_KEYS: usize = 4096;

/// Long discriminant paths embed a `Vec<Atom>` in the stable key. Cache only
/// short semantic paths and let longer paths use the existing discriminant index
/// and property caches.
pub(crate) const MAX_CACHED_DISCRIMINANT_PATH_LEN: usize = 4;

pub(crate) const fn guard_can_use_narrow_type_cache(guard: &TypeGuard) -> bool {
    match guard {
        TypeGuard::Typeof(_)
        | TypeGuard::Instanceof(_, _)
        | TypeGuard::LiteralEquality(_)
        | TypeGuard::NullishEquality
        | TypeGuard::Truthy
        | TypeGuard::InProperty(_)
        | TypeGuard::Predicate { .. }
        | TypeGuard::Array
        | TypeGuard::ArrayElementPredicate { .. }
        | TypeGuard::Constructor(_) => true,
        TypeGuard::Discriminant { property_path, .. } => {
            property_path.len() <= MAX_CACHED_DISCRIMINANT_PATH_LEN
        }
    }
}
