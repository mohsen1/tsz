//! #14345 Stage-3 name-aware alpha-rename pairing.
//!
//! When two same-arity generic signatures are related, the alpha-rename that
//! normalizes them onto one set of type-parameter identities historically
//! pairs the source/target type parameters by declaration POSITION (a `.zip`
//! of the two `type_params` lists). When the signatures use the SAME multiset
//! of names in a DIFFERENT order (e.g. `<E,A>` vs `<A,E>`), positional pairing
//! renames the target body onto the wrong source identities and produces a
//! spurious mismatch (TS2322/TS2345). Under `TSZ_ALPHA_NAME_PAIR=1` the pairing
//! is done by NAME instead, so same-named params line up across the reorder.

use crate::types::TypeParamInfo;

/// Gate for name-aware alpha-rename pairing (flag `TSZ_ALPHA_NAME_PAIR=1`).
///
/// Byte-parity-inert when OFF (the default): the pairing falls back to the
/// historical positional `.zip`. The mis-pairing this flag fixes only becomes
/// observable under the dormant `TSZ_TYPEPARAM_DECL_IDENTITY` keystone
/// (#14696), whose distinct authoritative declaration-origin ids stop same-name
/// source params from interning to a single id that masks the reorder.
pub(super) fn alpha_name_pair_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_ALPHA_NAME_PAIR").is_ok_and(|v| v == "1"))
}

/// Compute a name-aware pairing of source/target type parameters.
///
/// When the source and target type-parameter lists carry the SAME multiset of
/// names, return a permutation `perm` such that `target[perm[i]]` has the same
/// name as `source[i]`. This lets the alpha-rename pair same-name params across
/// a reordered declaration (`<E,A>` vs `<A,E>`) instead of by position, which
/// would otherwise rename the target body onto the wrong source identities.
///
/// Returns `None` when the lists differ in length or the name multisets are not
/// equal (caller falls back to positional pairing). The comparison is over
/// `Atom` name identities only -- no surface-text or user-name string
/// inspection.
pub(super) fn name_aware_target_permutation(
    source_type_params: &[TypeParamInfo],
    target_type_params: &[TypeParamInfo],
) -> Option<Vec<usize>> {
    if source_type_params.len() != target_type_params.len() {
        return None;
    }
    let mut used = vec![false; target_type_params.len()];
    let mut perm = Vec::with_capacity(source_type_params.len());
    for source_tp in source_type_params {
        let matched = target_type_params
            .iter()
            .enumerate()
            .find(|(idx, target_tp)| !used[*idx] && target_tp.name == source_tp.name);
        match matched {
            Some((idx, _)) => {
                used[idx] = true;
                perm.push(idx);
            }
            // A source name has no unused same-named target counterpart ->
            // the name multisets are not equal; abort, keep positional.
            None => return None,
        }
    }
    Some(perm)
}

#[cfg(test)]
mod tests {
    use super::name_aware_target_permutation;
    use crate::types::TypeParamInfo;
    use tsz_common::interner::Atom;

    fn tp(name: u32) -> TypeParamInfo {
        TypeParamInfo::simple(Atom(name))
    }

    #[test]
    fn identity_order_yields_identity_permutation() {
        let src = [tp(1), tp(2), tp(3)];
        let tgt = [tp(1), tp(2), tp(3)];
        assert_eq!(
            name_aware_target_permutation(&src, &tgt),
            Some(vec![0, 1, 2])
        );
    }

    #[test]
    fn reordered_same_names_pair_by_name() {
        // source <A,E> (1,2) vs target <E,A> (2,1): source[0]=A pairs target[1],
        // source[1]=E pairs target[0].
        let src = [tp(1), tp(2)];
        let tgt = [tp(2), tp(1)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), Some(vec![1, 0]));
    }

    #[test]
    fn different_names_fall_back_to_positional() {
        // source <A,E> (1,2) vs target <T,U> (3,4): no shared names -> None.
        let src = [tp(1), tp(2)];
        let tgt = [tp(3), tp(4)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }

    #[test]
    fn length_mismatch_is_none() {
        let src = [tp(1), tp(2)];
        let tgt = [tp(1)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }

    #[test]
    fn repeated_names_consume_each_target_once() {
        // multiset {A,A,E} vs {A,E,A}: each source consumes one unused same-named
        // target in order.
        let src = [tp(1), tp(1), tp(2)];
        let tgt = [tp(1), tp(2), tp(1)];
        assert_eq!(
            name_aware_target_permutation(&src, &tgt),
            Some(vec![0, 2, 1])
        );
    }

    #[test]
    fn unequal_multiset_same_len_is_none() {
        // {A,A} vs {A,E}: second source A finds no second target A -> None.
        let src = [tp(1), tp(1)];
        let tgt = [tp(1), tp(2)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }
}
