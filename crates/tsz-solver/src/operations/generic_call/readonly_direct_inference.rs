use crate::construction::TypeDatabase;
use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::types::{TypeData, TypeId};

fn source_needs_readonly_annotation_wrapper(db: &dyn TypeDatabase, source: TypeId) -> bool {
    !matches!(db.lookup(source), Some(TypeData::ReadonlyType(_)))
        && (crate::type_queries::get_array_element_type(db, source).is_some()
            || crate::type_queries::get_tuple_elements(db, source).is_some())
}

pub(super) fn wrap_readonly_annotation_source(
    db: &dyn TypeDatabase,
    source: TypeId,
    is_readonly_annotation: bool,
) -> TypeId {
    if is_readonly_annotation && source_needs_readonly_annotation_wrapper(db, source) {
        db.readonly_type(source)
    } else {
        source
    }
}

pub(super) fn restore_direct_inference(
    db: &dyn TypeDatabase,
    infer_ctx: &mut InferenceContext,
    var: InferenceVar,
    ty: TypeId,
) -> TypeId {
    if infer_ctx.has_readonly_source_candidate_for(var, ty)
        && source_needs_readonly_annotation_wrapper(db, ty)
    {
        let readonly_ty = db.readonly_type(ty);
        infer_ctx.set_resolved_type(var, readonly_ty);
        readonly_ty
    } else {
        ty
    }
}
