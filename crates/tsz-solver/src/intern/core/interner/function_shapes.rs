//! Function-shape interning and lookup for [`TypeInterner`], including the
//! per-parameter "arity-only optional" display masks for JS untyped
//! signatures (#17227).
//!
//! A bare, unannotated parameter in a JS file is `optional` in its
//! `FunctionShape` only so call-arity checking stays lenient; `tsc` never
//! displays it with `?`. The mask records which parameters owe their
//! `optional` bit to that rule so the printer can render them as required.
//! Because a structurally identical TS shape — where `?` was actually
//! written — must keep its `?`, masked shapes intern under their own
//! `FunctionShapeId`s, allocated outside the plain dedup map: the mask
//! participates in type identity exactly as a struct field would, without
//! widening `FunctionShape` itself.

use super::TypeInterner;
use crate::types::{FunctionShape, FunctionShapeId, TypeId};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::FxBuildHasher;
use std::sync::Arc;

/// Dedup key for a masked shape: the plain shape plus its display mask.
type MaskedShapeKey = (Arc<FunctionShape>, Box<[bool]>);

/// Storage for JS untyped-signature display masks, held by [`TypeInterner`].
#[derive(Default)]
pub(in crate::intern) struct JsDisplayMasks {
    /// `(shape, mask)` -> the unique `FunctionShapeId` allocated for it.
    by_shape: DashMap<MaskedShapeKey, FunctionShapeId, FxBuildHasher>,
    /// `FunctionShapeId` -> mask, for ids allocated by
    /// `function_with_arity_optional_mask`.
    masks: DashMap<FunctionShapeId, Arc<[bool]>, FxBuildHasher>,
}

impl TypeInterner {
    #[inline]
    pub fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.function_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(FunctionShape {
                type_params: Vec::new(),
                params: Vec::new(),
                this_type: None,
                return_type: TypeId::ERROR,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            })
        })
    }

    pub(in crate::intern::core) fn intern_function_shape(
        &self,
        shape: FunctionShape,
    ) -> FunctionShapeId {
        tsz_common::perf_counters::record_interner_function_shape_intern_call();
        FunctionShapeId(self.function_shapes.intern(shape))
    }

    /// Intern a function type whose flagged parameters are `optional` ONLY
    /// for JS call-arity leniency (a bare, unannotated parameter in a JS
    /// file). `mask[i] == true` marks `shape.params[i]` as arity-only
    /// optional: call-arity checking and subtyping keep reading `optional`
    /// unchanged, but display renders the parameter as required
    /// (`tree: any`, not `tree?: any`), matching tsc. An empty or
    /// length-mismatched mask falls back to the plain function intern.
    pub fn function_with_arity_optional_mask(&self, shape: FunctionShape, mask: &[bool]) -> TypeId {
        if mask.len() != shape.params.len() || !mask.contains(&true) {
            return self.function(shape);
        }
        let key = (Arc::new(shape), Box::<[bool]>::from(mask));
        let shape_id = match self.js_display_masks.by_shape.entry(key) {
            Entry::Occupied(e) => *e.get(),
            Entry::Vacant(e) => {
                let id =
                    FunctionShapeId(self.function_shapes.insert_unique(Arc::clone(&e.key().0)));
                self.js_display_masks.masks.insert(id, Arc::from(mask));
                e.insert(id);
                id
            }
        };
        self.function_type_from_shape_id(shape_id)
    }

    /// The per-parameter arity-only-optional display mask recorded for `id`,
    /// or `None` for ids interned without one. `Some(mask)[i] == true` means
    /// `params[i]`'s `optional` bit exists only for JS call-arity leniency
    /// and the parameter displays as required.
    pub fn function_shape_arity_optional_mask(&self, id: FunctionShapeId) -> Option<Arc<[bool]>> {
        self.js_display_masks
            .masks
            .get(&id)
            .map(|entry| Arc::clone(entry.value()))
    }
}
