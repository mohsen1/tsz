//! Declaration-boundary projection for Sound Mode (issue #8533).
//!
//! When sound user code observes a type owned by an external declaration file
//! (`.d.ts`, a default lib, or a `node_modules` package), `any` in
//! read/covariant positions is projected to `unknown`, so an unvalidated
//! boundary value cannot silently act as a bottom type inside sound code.
//! Write/contravariant positions stay permissive: parameters the user passes
//! *into* the library keep `any`.
//!
//! This is a semantic *view*. Projected `TypeId`s are built on demand through
//! the interner; no interned definition is mutated in place, and ordinary mode
//! (and `--sound` without the projection flag) never invokes it. The decision
//! of *when* to project — i.e. that an observed value is declaration-owned — is
//! a checker-side trust-boundary policy; this module owns only the
//! polarity-aware type transform.
//!
//! Scope is intentionally small (the issue's "tiny opt-in prototype"): the
//! projection descends only observable value shapes — objects, functions,
//! callables, unions, intersections, and `readonly` wrappers — and rewrites
//! `any` with correct variance through them. Polarity flips at each function
//! parameter position, so a library-supplied callback parameter
//! (`(cb: (value: any) => void) => void`) is itself a read position for user
//! code and is projected, exactly as `SOUND_MODE.md` describes.
//!
//! Deliberately left unchanged (per `SOUND_MODE.md` "positions that should not be
//! naively rewritten"):
//! - type-level plumbing: `Lazy`/`Application`/`Conditional`/`Mapped`/`infer`/
//!   `IndexAccess`/`KeyOf`/template-literal nodes;
//! - mutable element containers: `Array`/`Tuple` (their element is both read and
//!   written through one `TypeId`, so a covariant-only rewrite would be
//!   unsound);
//! - index signatures (same read/write aliasing concern as mutable containers).

use rustc_hash::FxHashMap;

use crate::construction::TypeDatabase;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, ObjectShape, ParamInfo, PropertyInfo, TypeData,
    TypeId,
};

/// Variance of the position currently being projected.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Polarity {
    /// Read position (return types, readable properties): project `any` here.
    Covariant,
    /// Write position (parameters, setter inputs): keep `any` permissive.
    Contravariant,
    /// Both read and written through one handle: leave unchanged.
    Invariant,
}

impl Polarity {
    const fn flip(self) -> Self {
        match self {
            Self::Covariant => Self::Contravariant,
            Self::Contravariant => Self::Covariant,
            Self::Invariant => Self::Invariant,
        }
    }
}

/// Maximum structural depth the projector descends before leaving a type
/// unchanged. Recursive object graphs are additionally guarded by the
/// `visiting` memo; this cap only bounds pathological non-cyclic nesting.
const MAX_PROJECTION_DEPTH: u32 = 64;

/// Projected pieces of a function or call signature: the new return type, the
/// new `this` type, and the new parameter list (`None` when the parameters were
/// unchanged so the caller can reuse the originals).
type ProjectedSignatureParts = (TypeId, Option<TypeId>, Option<Vec<ParamInfo>>);

/// Project `any` to `unknown` in the read/covariant positions of `ty` as
/// observed across a declaration trust boundary. See the module docs for the
/// scope and variance rules.
///
/// The result is interned through `db`; the original type is returned unchanged
/// when nothing in a projectable position was `any`.
pub fn project_declaration_boundary(
    db: &dyn TypeDatabase,
    ty: TypeId,
    polarity: Polarity,
) -> TypeId {
    Projector {
        db,
        visiting: FxHashMap::default(),
        depth: 0,
    }
    .project(ty, polarity)
}

struct Projector<'a> {
    db: &'a dyn TypeDatabase,
    visiting: FxHashMap<(TypeId, Polarity), TypeId>,
    depth: u32,
}

impl Projector<'_> {
    fn project(&mut self, ty: TypeId, polarity: Polarity) -> TypeId {
        // The single rewrite: `any` becomes `unknown` only when read (covariant).
        if ty == TypeId::ANY {
            return if polarity == Polarity::Covariant {
                TypeId::UNKNOWN
            } else {
                ty
            };
        }
        if self.depth >= MAX_PROJECTION_DEPTH {
            return ty;
        }
        if let Some(&cached) = self.visiting.get(&(ty, polarity)) {
            return cached;
        }
        let Some(key) = self.db.lookup(ty) else {
            return ty;
        };
        // Only descend into observable value shapes; everything else
        // (type-level plumbing, mutable containers, leaves) is unchanged.
        if !Self::is_projectable(&key) {
            return ty;
        }
        // Seed the cycle guard with the original id so a self-referential shape
        // resolves to itself rather than recursing forever.
        self.visiting.insert((ty, polarity), ty);
        self.depth += 1;
        let result = self.project_key(ty, &key, polarity);
        self.depth -= 1;
        self.visiting.insert((ty, polarity), result);
        result
    }

    const fn is_projectable(key: &TypeData) -> bool {
        matches!(
            key,
            TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Union(_)
                | TypeData::Intersection(_)
                | TypeData::ReadonlyType(_)
        )
    }

    fn project_key(&mut self, ty: TypeId, key: &TypeData, polarity: Polarity) -> TypeId {
        match key {
            TypeData::Union(members) => {
                let members = self.db.type_list(*members);
                match self.project_list(&members, polarity) {
                    Some(projected) => self.db.union(projected),
                    None => ty,
                }
            }
            TypeData::Intersection(members) => {
                let members = self.db.type_list(*members);
                match self.project_list(&members, polarity) {
                    Some(projected) => self.db.intersection(projected),
                    None => ty,
                }
            }
            TypeData::ReadonlyType(inner) => {
                let projected = self.project(*inner, polarity);
                if projected == *inner {
                    ty
                } else {
                    self.db.readonly_type(projected)
                }
            }
            TypeData::Object(shape_id) => {
                let shape = self.db.object_shape(*shape_id);
                match self.project_object_shape(&shape, polarity) {
                    Some(new_shape) => self.db.object_with_flags_and_symbol(
                        new_shape.properties,
                        new_shape.flags,
                        new_shape.symbol,
                    ),
                    None => ty,
                }
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(*shape_id);
                match self.project_object_shape(&shape, polarity) {
                    Some(new_shape) => self.db.object_with_index(new_shape),
                    None => ty,
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.db.function_shape(*shape_id);
                match self.project_function_shape(&shape, polarity) {
                    Some(new_shape) => self.db.function(new_shape),
                    None => ty,
                }
            }
            TypeData::Callable(shape_id) => {
                let shape = self.db.callable_shape(*shape_id);
                match self.project_callable_shape(&shape, polarity) {
                    Some(new_shape) => self.db.callable(new_shape),
                    None => ty,
                }
            }
            // `is_projectable` admits no other variants.
            _ => ty,
        }
    }

    /// Project each member with the surrounding polarity, returning `None` when
    /// nothing changed (so the caller can keep the original `TypeId`).
    fn project_list(&mut self, members: &[TypeId], polarity: Polarity) -> Option<Vec<TypeId>> {
        let mut out: Option<Vec<TypeId>> = None;
        for (index, &member) in members.iter().enumerate() {
            let projected = self.project(member, polarity);
            if let Some(out) = &mut out {
                out.push(projected);
            } else if projected != member {
                let mut changed = Vec::with_capacity(members.len());
                changed.extend_from_slice(&members[..index]);
                changed.push(projected);
                out = Some(changed);
            }
        }
        out
    }

    fn project_params(
        &mut self,
        params: &[ParamInfo],
        polarity: Polarity,
    ) -> Option<Vec<ParamInfo>> {
        let mut out: Option<Vec<ParamInfo>> = None;
        for (index, param) in params.iter().enumerate() {
            let projected = self.project(param.type_id, polarity);
            if let Some(out) = &mut out {
                out.push(ParamInfo {
                    type_id: projected,
                    ..*param
                });
            } else if projected != param.type_id {
                let mut changed = Vec::with_capacity(params.len());
                changed.extend_from_slice(&params[..index]);
                changed.push(ParamInfo {
                    type_id: projected,
                    ..*param
                });
                out = Some(changed);
            }
        }
        out
    }

    fn project_property(
        &mut self,
        prop: &PropertyInfo,
        polarity: Polarity,
    ) -> Option<PropertyInfo> {
        let new_read = self.project(prop.type_id, polarity);
        // A readonly property has no write surface: project its read side only.
        if prop.readonly {
            if new_read == prop.type_id {
                return None;
            }
            let mut projected = prop.clone();
            projected.type_id = new_read;
            return Some(projected);
        }
        // A mutable property keeps a permissive write surface. `write_type ==
        // NONE` means "same as read"; make that aliasing explicit so projecting
        // the read side to `unknown` does not also tighten writes.
        let effective_write = if prop.write_type == TypeId::NONE {
            prop.type_id
        } else {
            prop.write_type
        };
        let new_write = self.project(effective_write, polarity.flip());
        if new_read == prop.type_id && new_write == effective_write {
            return None;
        }
        let mut projected = prop.clone();
        projected.type_id = new_read;
        projected.write_type = if new_write == new_read {
            TypeId::NONE
        } else {
            new_write
        };
        Some(projected)
    }

    /// Project a property list, returning `None` when nothing changed so the
    /// caller can keep the original (unchanged properties are only cloned once a
    /// later property forces a new vector).
    fn project_property_list(
        &mut self,
        properties: &[PropertyInfo],
        polarity: Polarity,
    ) -> Option<Vec<PropertyInfo>> {
        let mut out: Option<Vec<PropertyInfo>> = None;
        for (index, prop) in properties.iter().enumerate() {
            let projected = self.project_property(prop, polarity);
            if let Some(out) = &mut out {
                out.push(projected.unwrap_or_else(|| prop.clone()));
            } else if let Some(projected) = projected {
                let mut changed = Vec::with_capacity(properties.len());
                changed.extend(properties[..index].iter().cloned());
                changed.push(projected);
                out = Some(changed);
            }
        }
        out
    }

    fn project_object_shape(
        &mut self,
        shape: &ObjectShape,
        polarity: Polarity,
    ) -> Option<ObjectShape> {
        let properties = self.project_property_list(&shape.properties, polarity)?;
        // Index signatures are left as-is: their value is both read and written
        // through one handle, so a covariant-only rewrite would be unsound.
        Some(ObjectShape {
            flags: shape.flags,
            properties,
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol_index: shape.symbol_index,
            symbol: shape.symbol,
        })
    }

    /// Shared variance walk for both standalone functions and call signatures:
    /// the return type keeps the surrounding polarity, while `this` and every
    /// parameter flip (they are write positions). Returns `None` when nothing
    /// changed; the inner `params` option is `None` when the parameter list is
    /// unchanged so the caller can reuse it.
    fn project_signature_parts(
        &mut self,
        return_type: TypeId,
        this_type: Option<TypeId>,
        params: &[ParamInfo],
        polarity: Polarity,
    ) -> Option<ProjectedSignatureParts> {
        let new_return = self.project(return_type, polarity);
        let new_this = this_type.map(|this| self.project(this, polarity.flip()));
        let new_params = self.project_params(params, polarity.flip());
        if new_return == return_type && new_this == this_type && new_params.is_none() {
            return None;
        }
        Some((new_return, new_this, new_params))
    }

    fn project_function_shape(
        &mut self,
        shape: &FunctionShape,
        polarity: Polarity,
    ) -> Option<FunctionShape> {
        let (return_type, this_type, params) = self.project_signature_parts(
            shape.return_type,
            shape.this_type,
            &shape.params,
            polarity,
        )?;
        Some(FunctionShape {
            type_params: shape.type_params.clone(),
            params: params.unwrap_or_else(|| shape.params.clone()),
            this_type,
            return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    fn project_signature(
        &mut self,
        sig: &CallSignature,
        polarity: Polarity,
    ) -> Option<CallSignature> {
        let (return_type, this_type, params) =
            self.project_signature_parts(sig.return_type, sig.this_type, &sig.params, polarity)?;
        Some(CallSignature {
            type_params: sig.type_params.clone(),
            params: params.unwrap_or_else(|| sig.params.clone()),
            this_type,
            return_type,
            type_predicate: sig.type_predicate,
            is_method: sig.is_method,
            declaration_group: sig.declaration_group,
        })
    }

    fn project_signatures(
        &mut self,
        signatures: &[CallSignature],
        polarity: Polarity,
    ) -> Option<Vec<CallSignature>> {
        let mut out: Option<Vec<CallSignature>> = None;
        for (index, sig) in signatures.iter().enumerate() {
            let projected = self.project_signature(sig, polarity);
            if let Some(out) = &mut out {
                out.push(projected.unwrap_or_else(|| sig.clone()));
            } else if let Some(projected) = projected {
                let mut changed = Vec::with_capacity(signatures.len());
                changed.extend(signatures[..index].iter().cloned());
                changed.push(projected);
                out = Some(changed);
            }
        }
        out
    }

    fn project_callable_shape(
        &mut self,
        shape: &CallableShape,
        polarity: Polarity,
    ) -> Option<CallableShape> {
        let call_signatures = self.project_signatures(&shape.call_signatures, polarity);
        let construct_signatures = self.project_signatures(&shape.construct_signatures, polarity);
        let properties = self.project_property_list(&shape.properties, polarity);
        if call_signatures.is_none() && construct_signatures.is_none() && properties.is_none() {
            return None;
        }
        Some(CallableShape {
            call_signatures: call_signatures.unwrap_or_else(|| shape.call_signatures.clone()),
            construct_signatures: construct_signatures
                .unwrap_or_else(|| shape.construct_signatures.clone()),
            properties: properties.unwrap_or_else(|| shape.properties.clone()),
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: shape.symbol,
            is_abstract: shape.is_abstract,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Polarity, project_declaration_boundary};
    use crate::construction::TypeInterner;
    use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TypeData, TypeId};

    fn param(type_id: TypeId) -> ParamInfo {
        ParamInfo {
            name: None,
            type_id,
            optional: false,
            rest: false,
        }
    }

    fn func(db: &TypeInterner, params: Vec<ParamInfo>, return_type: TypeId) -> TypeId {
        db.function(FunctionShape::new(params, return_type))
    }

    fn function_return(db: &TypeInterner, ty: TypeId) -> TypeId {
        match db.lookup(ty) {
            Some(TypeData::Function(shape_id)) => db.function_shape(shape_id).return_type,
            other => panic!("expected function, got {other:?}"),
        }
    }

    fn function_param(db: &TypeInterner, ty: TypeId, index: usize) -> TypeId {
        match db.lookup(ty) {
            Some(TypeData::Function(shape_id)) => db.function_shape(shape_id).params[index].type_id,
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn bare_any_projects_only_when_read() {
        let db = TypeInterner::new();
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Covariant),
            TypeId::UNKNOWN,
            "read position projects any -> unknown"
        );
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Contravariant),
            TypeId::ANY,
            "write position keeps any permissive"
        );
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Invariant),
            TypeId::ANY,
            "invariant position is left unchanged"
        );
    }

    #[test]
    fn non_any_leaf_is_unchanged() {
        let db = TypeInterner::new();
        assert_eq!(
            project_declaration_boundary(&db, TypeId::STRING, Polarity::Covariant),
            TypeId::STRING
        );
        let clean = func(&db, vec![param(TypeId::STRING)], TypeId::NUMBER);
        assert_eq!(
            project_declaration_boundary(&db, clean, Polarity::Covariant),
            clean,
            "a function with no any returns the identical TypeId"
        );
    }

    #[test]
    fn function_return_is_read_position() {
        let db = TypeInterner::new();
        // `(x: any) => any`: return projects, parameter stays permissive.
        let f = func(&db, vec![param(TypeId::ANY)], TypeId::ANY);
        let projected = project_declaration_boundary(&db, f, Polarity::Covariant);
        assert_eq!(function_return(&db, projected), TypeId::UNKNOWN);
        assert_eq!(
            function_param(&db, projected, 0),
            TypeId::ANY,
            "parameter is a write position and keeps any"
        );
    }

    #[test]
    fn function_projected_contravariantly_keeps_return_any() {
        let db = TypeInterner::new();
        let f = func(&db, vec![], TypeId::ANY);
        // As a write position the whole function flips: its return is now
        // contravariant, so the any stays.
        assert_eq!(
            project_declaration_boundary(&db, f, Polarity::Contravariant),
            f
        );
    }

    #[test]
    fn library_supplied_callback_parameter_is_a_read_position() {
        let db = TypeInterner::new();
        // `(cb: (value: any) => void) => void`. The callback parameter is a
        // value the library pushes into user code, so `value` must project to
        // `unknown` via the double polarity flip.
        let callback = func(&db, vec![param(TypeId::ANY)], TypeId::VOID);
        let outer = func(&db, vec![param(callback)], TypeId::VOID);
        let projected = project_declaration_boundary(&db, outer, Polarity::Covariant);
        let projected_callback = function_param(&db, projected, 0);
        assert_eq!(
            function_param(&db, projected_callback, 0),
            TypeId::UNKNOWN,
            "callback parameter supplied to user code projects any -> unknown"
        );
    }

    #[test]
    fn object_property_splits_read_and_write() {
        let db = TypeInterner::new();
        let value = db.intern_string("value");
        let obj = db.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
        let projected = project_declaration_boundary(&db, obj, Polarity::Covariant);
        let shape = match db.lookup(projected) {
            Some(TypeData::Object(shape_id)) => db.object_shape(shape_id),
            other => panic!("expected object, got {other:?}"),
        };
        let prop = &shape.properties[0];
        assert_eq!(
            prop.type_id,
            TypeId::UNKNOWN,
            "read side projects to unknown"
        );
        assert_eq!(
            prop.write_type,
            TypeId::ANY,
            "write side stays permissive (any)"
        );
    }

    #[test]
    fn readonly_property_projects_read_side() {
        let db = TypeInterner::new();
        let value = db.intern_string("value");
        let mut prop = PropertyInfo::new(value, TypeId::ANY);
        prop.readonly = true;
        let obj = db.object(vec![prop]);
        let projected = project_declaration_boundary(&db, obj, Polarity::Covariant);
        let shape = match db.lookup(projected) {
            Some(TypeData::Object(shape_id)) => db.object_shape(shape_id),
            other => panic!("expected object, got {other:?}"),
        };
        assert_eq!(shape.properties[0].type_id, TypeId::UNKNOWN);
    }
}
