//! Augmented-interface redirects for index-access evaluation.

use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::TypeId;
use crate::visitors::visitor::{object_shape_id, object_with_index_shape_id};

/// #14344 / #14345: redirect an index access against a frozen pre-merge empty
/// snapshot of a cross-file augmented interface to the merged body published
/// under the home `DefId`.
///
/// Discriminator is structural: channel membership keyed on `shape.symbol` plus
/// empty properties. No name or file-string match participates.
pub(super) fn redirect_empty_augmented_base_index<R: TypeResolver>(
    evaluator: &mut TypeEvaluator<'_, R>,
    index_type: TypeId,
    symbol: Option<tsz_binder::SymbolId>,
) -> Option<TypeId> {
    let symbol = symbol?;
    let merged_body = evaluator
        .resolver()
        .augmented_base_body_for_symbol(symbol.0)
        .or_else(|| {
            evaluator
                .query_db()
                .and_then(|db| db.augmented_base_body_for_symbol(symbol.0))
        })?;

    let evaluated = evaluator.evaluate(merged_body);
    let interner = evaluator.interner();
    let shape_id = object_shape_id(interner, evaluated)
        .or_else(|| object_with_index_shape_id(interner, evaluated))?;
    let shape = interner.object_shape(shape_id);
    if shape.properties.is_empty() {
        return None;
    }
    let result = evaluator.evaluate_object_index(&shape.properties, index_type);
    (result != TypeId::UNDEFINED).then_some(result)
}
