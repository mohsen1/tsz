//! Signature identity helpers used while merging structural interface types.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tsz_solver::{CallSignature, TypeId};

/// Deduplicate call signatures keeping the LAST occurrence of each unique
/// type-only signature.
///
/// Construct signatures use the solver-owned exact identity through the
/// construct-signature query boundary. This historical call-signature path
/// remains type-only because call overload ordering does not consume the
/// constructor literal-provenance marker.
pub(crate) fn dedup_call_signatures_keep_last(sigs: &mut Vec<CallSignature>) {
    if sigs.len() <= 1 {
        return;
    }
    // Build a signature key from parameter and return types.
    // Walk from the end and record the last index for each unique key.
    // Then retain only those positions.
    type SignatureKey = (SmallVec<[TypeId; 4]>, TypeId);

    let key_of = |sig: &CallSignature| -> SignatureKey {
        let param_types = sig.params.iter().map(|p| p.type_id).collect();
        (param_types, sig.return_type)
    };

    let mut seen: FxHashMap<SignatureKey, usize> = FxHashMap::default();
    // Record the LAST index for each key
    for (i, sig) in sigs.iter().enumerate() {
        seen.insert(key_of(sig), i);
    }
    // Retain only signatures whose index matches their last occurrence
    let mut i = 0;
    sigs.retain(|sig| {
        let idx = i;
        i += 1;
        seen.get(&key_of(sig)).copied() == Some(idx)
    });
}
