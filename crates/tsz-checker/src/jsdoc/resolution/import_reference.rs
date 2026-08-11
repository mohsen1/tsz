//! JSDoc `import("./mod")…` reference resolution — the string-parse path.
//!
//! Split out of `name_resolution.rs` to keep each checker source file within
//! the per-file size budget. Owns the two JSDoc import-reference resolvers:
//!
//! - `resolve_jsdoc_import_type_reference` — a bare `import("./mod").Member`
//!   type-position reference, including a qualified `@typedef A.B` member.
//! - `resolve_jsdoc_typeof_import_reference_parts` — the `typeof
//!   import("./mod").a.b` value-position property walk.

use super::name_resolution::JsdocNameMode;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::jsdoc) fn resolve_jsdoc_import_type_reference(
        &mut self,
        type_expr: &str,
    ) -> Option<TypeId> {
        let (module_specifier, member_name) = Self::parse_jsdoc_import_type(type_expr)?;
        let resolution_mode = Self::jsdoc_import_type_resolution_mode(type_expr);

        if let Some(member_name) = member_name {
            // `member_name` may be a qualified path (`A.B[.C…]`). A dotted JSDoc
            // `@typedef {T} A.B` registers under its *full* qualified name, so
            // resolve the whole path against the module's typedef surface
            // first. This is purely additive: a single-segment member contains
            // no dot and skips this branch (leaving the resolution order below
            // byte-for-byte unchanged), while a qualified path that is not a
            // typedef falls through to the head-segment resolution below,
            // preserving prior behavior for genuine namespace-member access.
            if member_name.contains('.')
                && let Some(typedef_type) =
                    self.resolve_import_type_jsdoc_typedef(&module_specifier, &member_name, None)
            {
                return Some(typedef_type);
            }
            // The head segment drives export/class resolution — tsc resolves
            // a qualified path progressively, so a non-typedef qualified path
            // falls back to its head segment for the lookups below. The full
            // path is kept for the TS2694 report, which qualifies its
            // namespace display with any declared dotted-typedef prefix.
            let full_member_path = member_name;
            let member_name: &str = full_member_path
                .split_once('.')
                .map_or(full_member_path.as_str(), |(head, _tail)| head);
            if let Some(sym_id) = self.resolve_jsdoc_import_member_with_mode(
                &module_specifier,
                member_name,
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
                self.resolve_import_type_jsdoc_typedef(&module_specifier, member_name, None)
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
                    member_name,
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
            // the namespace display accordingly (oracle-verified:
            // `Namespace '"m".Dotted' has no exported member 'Missing'`).
            let segments: Vec<&str> = full_member_path.split('.').collect();
            let mut prefix_len = 0usize;
            for candidate_len in (1..segments.len()).rev() {
                if self.import_type_jsdoc_typedef_namespace_prefix_exists(
                    &module_specifier,
                    &segments[..candidate_len].join("."),
                    None,
                ) {
                    prefix_len = candidate_len;
                    break;
                }
            }
            let display_namespace = if prefix_len == 0 {
                format!("\"{namespace_name}\"")
            } else {
                format!("\"{namespace_name}\".{}", segments[..prefix_len].join("."))
            };
            let missing_member = segments[prefix_len];
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
