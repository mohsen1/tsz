use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Format a single property fragment "name: type" used inside the synthesized
    /// JSX-attributes source-type display. Mirrors tsc's per-property display:
    /// shorthand attrs render as `name: true`, others use the formatted value type.
    pub(super) fn format_jsx_synthesized_prop_fragment(
        &mut self,
        name: &str,
        type_id: TypeId,
    ) -> String {
        let display_name = {
            let mut chars = name.chars();
            let is_ident = chars.next().is_some_and(|first| {
                (first == '_' || first == '$' || first.is_ascii_alphabetic())
                    && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            });
            if is_ident {
                name.to_string()
            } else {
                format!("\"{name}\"")
            }
        };
        let type_str = if type_id == TypeId::BOOLEAN_TRUE {
            "true".to_string()
        } else {
            self.format_type(type_id)
        };
        format!("{display_name}: {type_str}")
    }
}
