use crate::{LiteralValue, TemplateSpan, TypeData, TypeDatabase, TypeId};

/// Match a non-infer template substitution span, such as `${" " | "\t"}`,
/// against the source string at the current position.
pub(super) fn match_template_segment_prefix(
    interner: &dyn TypeDatabase,
    source: &str,
    pos: usize,
    type_id: TypeId,
) -> Option<usize> {
    match interner.lookup(type_id)? {
        TypeData::Literal(LiteralValue::String(atom)) => {
            let text = interner.resolve_atom(atom);
            source
                .get(pos..)?
                .starts_with(&text)
                .then_some(pos + text.len())
        }
        TypeData::Union(list_id) => interner
            .type_list(list_id)
            .iter()
            .find_map(|member| match_template_segment_prefix(interner, source, pos, *member)),
        TypeData::TemplateLiteral(template_id) => {
            let spans = interner.template_list(template_id);
            let mut text = String::new();
            for span in spans.iter() {
                let TemplateSpan::Text(atom) = span else {
                    return None;
                };
                text.push_str(&interner.resolve_atom(*atom));
            }
            source
                .get(pos..)?
                .starts_with(&text)
                .then_some(pos + text.len())
        }
        _ => None,
    }
}
