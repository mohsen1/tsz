//! JSDoc `import("./mod").Member` reference resolution — the string-parse
//! path counterpart of the TS-syntax import-type resolver in
//! `state/type_resolution/import_type.rs`.
//!
//! Owns resolving a JSDoc import-type reference through the module's export
//! surface, its JSDoc `@typedef`/`@callback` declarations (including dotted
//! *qualified* names such as `@typedef {number} Dotted.Name`), and the
//! CommonJS expando-export fallback, plus the TS2694 report when every route
//! fails.

use super::name_resolution::JsdocNameMode;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::jsdoc) fn resolve_jsdoc_import_type_reference(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let (module_specifier, member_chain) =
            Self::parse_jsdoc_import_type_member_chain(type_expr)?;
        let resolution_mode = Self::jsdoc_import_type_resolution_mode(type_expr);

        if let Some(member_name) = member_chain.first().cloned() {
            if let Some(sym_id) = self.resolve_jsdoc_import_member_with_mode(
                &module_specifier,
                &member_name,
                resolution_mode,
            ) {
                // `import(...).Member` (without a leading `typeof`) is a bare
                // type-position reference: `Member` must be type-eligible
                // (interface/class/enum/type-alias/namespace/typedef), not a
                // plain value export. `BareTypeReference` mode rejects a
                // plain-value symbol (returns `ERROR`) the same way a local
                // bare name reference already does; `ValuePosition` mode would
                // silently hand back the value's own type instead.
                let resolved = self
                    .resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::BareTypeReference);
                if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                    return Some(resolved);
                }
            }
            if let Some(typedef_type) =
                self.resolve_import_type_jsdoc_typedef(&module_specifier, &member_name, None)
            {
                return Some(typedef_type);
            }
            // A dotted reference (`import("./m").Dotted.Name`) may name a
            // *qualified* JSDoc `@typedef`/`@callback` declaration
            // (`@typedef {number} Dotted.Name`), which registers under its
            // full dotted name rather than as a member `Dotted`.
            if member_chain.len() > 1
                && let Some(typedef_type) = self.resolve_import_type_jsdoc_typedef(
                    &module_specifier,
                    &member_chain.join("."),
                    None,
                )
            {
                return Some(typedef_type);
            }
            // A CommonJS expando export (`module.exports.Member = Member` /
            // `exports.Member = Member`) records no SymbolId in the binder's
            // export tables — those only track ES `export` syntax — so the
            // plain symbol lookup above never sees it. `commonjs_named_export_class_symbol_for_file`
            // (the synthesized-export-surface query boundary) recognizes the
            // expando assignment and, when its RHS is a class declaration's own
            // identifier, hands back that class's real SymbolId. This mirrors
            // the same fallback already used for the `const { X } = require(...)`
            // binding-element path in `resolve_jsdoc_commonjs_binding_element_type`.
            if let Some((export_sym_id, export_file_idx)) = self
                .resolve_js_export_named_class_symbol(
                    &module_specifier,
                    &member_name,
                    Some(self.ctx.current_file_idx),
                )
            {
                self.ctx
                    .register_symbol_file_target(export_sym_id, export_file_idx);
                let resolved = self.resolve_jsdoc_symbol_type_with_mode(
                    export_sym_id,
                    JsdocNameMode::BareTypeReference,
                );
                if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                    return Some(resolved);
                }
            }
            // Neither a type-eligible export nor a JSDoc `@typedef` named
            // `member_name` exists on the module: tsc reports TS2694
            // ("Namespace has no exported member") the same way the
            // TS-syntax `import(...).Member` resolver does. This mirrors
            // that resolver's `report_missing_import_type_member`, which the
            // string-based JSDoc parse path cannot reach directly.
            let namespace_name = self.imported_namespace_display_module_name(&module_specifier);
            // When a leading portion of a dotted reference names a namespace
            // synthesized by dotted `@typedef`/`@callback` declarations
            // (`Dotted.Missing` with a declared `Dotted.Name`), tsc reports
            // the first segment missing *under* that namespace and qualifies
            // the namespace display accordingly.
            let mut prefix_len = 0usize;
            for candidate_len in (1..member_chain.len()).rev() {
                if self.import_type_jsdoc_typedef_namespace_prefix_exists(
                    &module_specifier,
                    &member_chain[..candidate_len].join("."),
                    None,
                ) {
                    prefix_len = candidate_len;
                    break;
                }
            }
            let display_namespace = if prefix_len == 0 {
                format!("\"{namespace_name}\"")
            } else {
                format!(
                    "\"{namespace_name}\".{}",
                    member_chain[..prefix_len].join(".")
                )
            };
            let missing_member = &member_chain[prefix_len];
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
                &[&display_namespace, missing_member],
            );
            let anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
            self.ctx.error(
                anchor,
                type_expr.len() as u32,
                message,
                crate::diagnostics::diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
            );
            return None;
        }

        self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx))
            .and_then(|module_type| {
                self.instance_type_from_constructor_type(module_type)
                    .or(Some(module_type))
            })
    }

    pub(crate) fn resolve_jsdoc_typeof_import_reference_parts(
        &mut self,
        module_specifier: &str,
        segments: &[(usize, String)],
    ) -> Result<TypeId, (usize, String)> {
        let mut current = self
            .commonjs_module_value_type(module_specifier, Some(self.ctx.current_file_idx))
            .or_else(|| self.build_typeof_import_namespace_type(module_specifier, None))
            .ok_or_else(|| {
                segments
                    .first()
                    .cloned()
                    .unwrap_or((0, String::from("import")))
            })?;

        if segments.is_empty() {
            return Ok(current);
        }

        for (offset, segment) in segments {
            let access = self.resolve_property_access_with_env(current, segment);
            current = match access {
                crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id, ..
                } => self.resolve_type_query_type(type_id),
                _ => return Err((*offset, segment.clone())),
            };
        }

        Ok(current)
    }
}
