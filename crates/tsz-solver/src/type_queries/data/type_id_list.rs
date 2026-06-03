//! `TypeIdList`: a shared, zero-copy view over an interned list of `TypeId`s.
//!
//! Used as the return type of the union/intersection member queries
//! (`get_union_members` / `get_intersection_members`). It wraps the
//! `Arc<[TypeId]>` the solver already owns so member inspection — one of the
//! hottest checker-boundary queries in alias- and union-heavy code — is an
//! O(1) refcount bump instead of a per-call heap allocation + element copy.

use crate::types::TypeId;
use std::sync::Arc;

/// A shared, read-only view over a list of `TypeId`s — the members of a
/// union or intersection type.
///
/// This wraps the interned `Arc<[TypeId]>` that the solver already owns, so
/// constructing one is an O(1) refcount bump rather than the per-call heap
/// allocation + element copy that returning a fresh `Vec<TypeId>` would
/// impose. Union/intersection member inspection is one of the hottest
/// checker-boundary queries in alias- and union-heavy code, so eliminating
/// that allocation removes a large source of heap churn that compounds
/// across every relation, narrowing, and display pass.
///
/// `TypeIdList` is a drop-in replacement for `Vec<TypeId>` in read-only
/// contexts: it `Deref`s to `[TypeId]` (so `.iter()`, `.len()`,
/// `.is_empty()`, indexing, slicing, `.contains()`, `.to_vec()`, etc. all
/// work), and it iterates exactly like `Vec<TypeId>` — `for m in list`
/// yields owned `TypeId`, `for m in &list` yields `&TypeId`. Callers that
/// need an owned, mutable buffer call `.to_vec()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeIdList(Arc<[TypeId]>);

impl TypeIdList {
    /// Wrap an already-interned member list (O(1) refcount bump).
    #[inline]
    pub const fn new(members: Arc<[TypeId]>) -> Self {
        Self(members)
    }

    /// The backing shared `Arc`. Exposed so callers can prove zero-copy
    /// sharing (e.g. `Arc::ptr_eq`) without copying the slice.
    #[inline]
    pub const fn as_arc(&self) -> &Arc<[TypeId]> {
        &self.0
    }

    /// Borrow the members as a slice. Mirrors `Vec<TypeId>::as_slice`.
    #[inline]
    pub fn as_slice(&self) -> &[TypeId] {
        &self.0
    }
}

impl std::ops::Deref for TypeIdList {
    type Target = [TypeId];

    #[inline]
    fn deref(&self) -> &[TypeId] {
        &self.0
    }
}

impl AsRef<[TypeId]> for TypeIdList {
    #[inline]
    fn as_ref(&self) -> &[TypeId] {
        &self.0
    }
}

impl From<Arc<[TypeId]>> for TypeIdList {
    #[inline]
    fn from(members: Arc<[TypeId]>) -> Self {
        Self(members)
    }
}

impl From<Vec<TypeId>> for TypeIdList {
    #[inline]
    fn from(members: Vec<TypeId>) -> Self {
        Self(members.into())
    }
}

// Element-wise equality with `Vec<TypeId>` from either side, so a
// `TypeIdList` compares like the `Vec<TypeId>` it replaced regardless of
// which operand of `==` it is.
impl PartialEq<Vec<TypeId>> for TypeIdList {
    #[inline]
    fn eq(&self, other: &Vec<TypeId>) -> bool {
        self.0.as_ref() == other.as_slice()
    }
}

impl PartialEq<TypeIdList> for Vec<TypeId> {
    #[inline]
    fn eq(&self, other: &TypeIdList) -> bool {
        self.as_slice() == other.0.as_ref()
    }
}

/// Owning iterator that yields each `TypeId` by value without allocating —
/// it walks the shared slice by index while holding the `Arc` alive. This
/// mirrors `Vec<TypeId>::into_iter()` so `for m in list { /* m: TypeId */ }`
/// behaves identically to the previous `Vec`-returning API, including
/// double-ended consumption (`.rev()`, `.next_back()`).
pub struct TypeIdListIter {
    members: Arc<[TypeId]>,
    /// Inclusive front cursor.
    idx: usize,
    /// Exclusive back cursor; the live window is `[idx, end)`.
    end: usize,
}

impl Iterator for TypeIdListIter {
    type Item = TypeId;

    #[inline]
    fn next(&mut self) -> Option<TypeId> {
        if self.idx < self.end {
            let value = self.members[self.idx];
            self.idx += 1;
            Some(value)
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.idx;
        (remaining, Some(remaining))
    }
}

impl DoubleEndedIterator for TypeIdListIter {
    #[inline]
    fn next_back(&mut self) -> Option<TypeId> {
        if self.idx < self.end {
            self.end -= 1;
            Some(self.members[self.end])
        } else {
            None
        }
    }
}

impl ExactSizeIterator for TypeIdListIter {
    #[inline]
    fn len(&self) -> usize {
        self.end - self.idx
    }
}

impl IntoIterator for TypeIdList {
    type Item = TypeId;
    type IntoIter = TypeIdListIter;

    #[inline]
    fn into_iter(self) -> TypeIdListIter {
        let end = self.0.len();
        TypeIdListIter {
            members: self.0,
            idx: 0,
            end,
        }
    }
}

impl<'a> IntoIterator for &'a TypeIdList {
    type Item = &'a TypeId;
    type IntoIter = std::slice::Iter<'a, TypeId>;

    #[inline]
    fn into_iter(self) -> std::slice::Iter<'a, TypeId> {
        self.0.iter()
    }
}
