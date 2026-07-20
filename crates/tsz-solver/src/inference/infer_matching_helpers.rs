use crate::caches::db::TypeDatabase;
use crate::types::{InferencePriority, TemplateSpan, TypeData, TypeId};

use super::infer::{InferenceContext, InferenceVar};
use super::template_anchor::{find_leftmost_occurrence, find_next_anchor_alternatives};
use super::template_segment_prefix::match_template_segment_prefix;

impl InferenceContext<'_> {
    pub(super) fn add_source_type_param_candidate(
        &mut self,
        var: InferenceVar,
        target: TypeId,
        priority: InferencePriority,
    ) {
        if self.type_is_own_original_type_param(var, target) {
            return;
        }

        if self.in_variance_walk {
            // Contravariant traversal swaps source and target in TSZ. Route the
            // recovered concrete type through the ordinary candidate entrypoint
            // so nested polarity and method/constructor bivariance can select
            // the candidate bucket without changing traversal direction.
            self.add_candidate(var, target, priority);
        } else {
            self.add_upper_bound(var, target);
        }
    }

    /// Match a source string against a template pattern, extracting infer variable bindings.
    ///
    /// # Arguments
    ///
    /// * `source` - The source string to match (e.g., `"user_123"`)
    /// * `spans` - The template spans (e.g., `[Text("user_"), Type(ID), Text("_")]`)
    ///
    /// # Returns
    ///
    /// * `Some(bindings)` - Mapping from inference variables to captured strings
    /// * `None` - The source doesn't match the pattern
    pub(super) fn match_template_pattern(
        &self,
        source: &str,
        spans: &[TemplateSpan],
    ) -> Option<Vec<(InferenceVar, String)>> {
        let mut bindings = Vec::with_capacity(spans.len());
        let mut pos = 0;

        for (i, span) in spans.iter().enumerate() {
            let is_last = i == spans.len() - 1;

            match span {
                TemplateSpan::Text(text_atom) => {
                    // Match literal text at current position
                    let text = self.interner.resolve_atom(*text_atom).to_string();
                    if !source.get(pos..)?.starts_with(&text) {
                        return None; // Text doesn't match
                    }
                    pos += text.len();
                }

                TemplateSpan::Type(type_id) => {
                    // Match both `infer T` (conditional) and generic `T` (type parameter).
                    // Intrinsics are never Infer or TypeParameter.
                    if !type_id.is_intrinsic()
                        && let Some(
                            TypeData::Infer(param_info) | TypeData::TypeParameter(param_info),
                        ) = self.interner.lookup(*type_id)
                        && let Some(var) = self.find_type_param(param_info.name)
                    {
                        if is_last {
                            // Last span: capture all remaining text (greedy)
                            let captured = source[pos..].to_string();
                            bindings.push((var, captured));
                            pos = source.len();
                        } else if let Some(alternatives) =
                            find_next_anchor_alternatives(self.interner, spans, i, |type_id| {
                                if type_id.is_intrinsic() {
                                    return false;
                                }
                                matches!(
                                    self.interner.lookup(type_id),
                                    Some(
                                        TypeData::Infer(param_info)
                                            | TypeData::TypeParameter(param_info)
                                    ) if self.find_type_param(param_info.name).is_some()
                                )
                            })
                        {
                            let capture_end = find_leftmost_occurrence(source, pos, &alternatives)?;
                            let captured = source[pos..capture_end].to_string();
                            bindings.push((var, captured));
                            pos = capture_end;
                        } else {
                            bindings.push((var, String::new()));
                        }
                    } else {
                        let next_pos =
                            match_template_segment_prefix(self.interner, source, pos, *type_id)?;
                        pos = next_pos;
                    }
                }
            }
        }

        // Must have consumed the entire source string
        (pos == source.len()).then_some(bindings)
    }
}

pub(super) fn constraint_is_nullable_union(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    let Some(TypeData::Union(members)) = db.lookup(constraint) else {
        return false;
    };
    db.type_list(members)
        .iter()
        .any(|&member| member.is_nullable())
}
