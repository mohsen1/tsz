//! Concrete anchor extraction for template-literal inference.

use crate::caches::db::TypeDatabase;
use crate::types::{LiteralValue, TemplateSpan, TypeData, TypeId};

/// Return the byte offset (relative to `source`) of the leftmost occurrence of
/// any string in `alternatives` starting at `pos`.
pub(crate) fn find_leftmost_occurrence(
    source: &str,
    pos: usize,
    alternatives: &[String],
) -> Option<usize> {
    let slice = source.get(pos..)?;
    alternatives
        .iter()
        .filter_map(|alt| slice.find(alt.as_str()).map(|offset| pos + offset))
        .min()
}

/// Find the concrete string alternatives that act as the next anchor for a
/// non-greedy infer-variable capture.
///
/// Scans spans after `start_idx` and returns the first span that can be
/// expressed as a finite set of string literals:
///
/// - `TemplateSpan::Text` -> `Some(vec![text])`
/// - `TemplateSpan::Type` holding a `Union` of string literals or a single
///   string `Literal` -> `Some(alternatives)`
///
/// Returns `None` when:
/// - The next `Type` span is another infer / type-parameter variable.
/// - The next `Type` span is a non-concrete type, such as `string`.
/// - No more spans remain.
pub(crate) fn find_next_anchor_alternatives(
    interner: &dyn TypeDatabase,
    spans: &[TemplateSpan],
    start_idx: usize,
    is_inference_variable: impl Fn(TypeId) -> bool,
) -> Option<Vec<String>> {
    match spans.get(start_idx + 1)? {
        TemplateSpan::Text(text) => {
            let s = interner.resolve_atom(*text).as_str().to_owned();
            Some(vec![s])
        }
        TemplateSpan::Type(type_id) => {
            if is_inference_variable(*type_id) {
                return None;
            }
            collect_string_alternatives_for_anchor(interner, *type_id)
        }
    }
}

/// Collect the concrete string literals from a type that is being used as a
/// fixed separator / anchor in a template pattern.
///
/// Handles:
/// - `Literal(String)` -> single-element vec
/// - `Union` of `Literal(String)` members -> all members
fn collect_string_alternatives_for_anchor(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<String>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match interner.lookup(type_id)? {
        TypeData::Literal(LiteralValue::String(atom)) => {
            Some(vec![interner.resolve_atom(atom).as_str().to_owned()])
        }
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            let mut alternatives = Vec::with_capacity(members.len());
            for &member in members.iter() {
                if member.is_intrinsic() {
                    return None;
                }
                if let Some(TypeData::Literal(LiteralValue::String(atom))) = interner.lookup(member)
                {
                    alternatives.push(interner.resolve_atom(atom).as_str().to_owned());
                } else {
                    return None;
                }
            }
            (!alternatives.is_empty()).then_some(alternatives)
        }
        _ => None,
    }
}
