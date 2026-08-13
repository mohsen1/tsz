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
        if let Some(result) = self.resolve_jsdoc_import_type_member_result(type_expr) {
            return match result {
                Ok(ty) => Some(ty),
                Err((namespace_display, member_name)) => {
                    // Neither a type-eligible export nor a JSDoc `@typedef`
                    // named `member_name` exists on the module: tsc reports
                    // TS2694 ("Namespace has no exported member") the same
                    // way the TS-syntax `import(...).Member` resolver does.
                    // This mirrors that resolver's
                    // `report_missing_import_type_member`, which the
                    // string-based JSDoc parse path cannot reach directly.
                    //
                    // `namespace_display` is already fully quoted-and-dotted
                    // (`"mod"` or `"mod".NS`) by the producer below — a
                    // qualified-chain failure needs the dotted-namespace-prefix
                    // segments to appear *outside* the quotes, which a blanket
                    // wrap here cannot express.
                    //
                    // This coarse anchor (caller-supplied, typically the
                    // enclosing comment or declaration start) and full-length
                    // span are a fallback for callers that have not adopted
                    // the precise-anchor path below. `jsdoc_type_annotation_for_node`
                    // and `resolve_jsdoc_param_type_with_pos` — the two
                    // authoritative per-declaration/per-parameter resolvers —
                    // call `resolve_jsdoc_import_type_member_result` directly
                    // and anchor at the member-name token themselves (#17176).
                    let message = crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
                        &[&namespace_display, &member_name],
                    );
                    let anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                    self.ctx.error(
                        anchor,
                        type_expr.len() as u32,
                        message,
                        crate::diagnostics::diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
                    );
                    None
                }
            };
        }

        let (module_specifier, _member_name) = Self::parse_jsdoc_import_type(type_expr)?;
        self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx))
            .and_then(|module_type| {
                self.instance_type_from_constructor_type(module_type)
                    .or(Some(module_type))
            })
    }

    /// Resolve a bare (non-`typeof`) `import("./mod").Member[.tail…]`
    /// JSDoc type-position reference without emitting a diagnostic.
    ///
    /// Returns `None` when `type_expr` is not this shape at all (including
    /// the memberless `import("./mod")` form, which names the module's own
    /// `export =` type and never fails this way). Returns `Some(Ok(ty))` when
    /// the head member resolves, or `Some(Err((namespace_display, member)))`
    /// when it does not — the caller decides where and how to anchor the
    /// resulting TS2694, since the source span of `type_expr` is not visible
    /// from here (different JSDoc tags carry the type expression at
    /// different offsets from their comment).
    ///
    /// `namespace_display` is the fully formatted namespace for the message —
    /// the quoted module name (`"m"`), qualified with any declared
    /// dotted-typedef prefix (`"m".Dotted`) or resolved namespace-symbol
    /// prefix (`"m".NS`) — and `member` is the first segment that failed
    /// under it; callers substitute both verbatim.
    pub(crate) fn resolve_jsdoc_import_type_member_result(
        &mut self,
        type_expr: &str,
    ) -> Option<Result<TypeId, (String, String)>> {
        let (module_specifier, member_name) = Self::parse_jsdoc_import_type(type_expr)?;
        let member_name = member_name?;
        let resolution_mode = Self::jsdoc_import_type_resolution_mode(type_expr);

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
            return Some(Ok(typedef_type));
        }
        let full_member_path = member_name;
        // A qualified reference (`A.B[.C…]`) that isn't a declared dotted
        // `@typedef` needs *namespace*-meaning resolution for every segment
        // but the last: tsc only walks past a qualifier when it names a
        // real namespace/module symbol, not any type-eligible export. A
        // class/interface/type-alias head is not eligible as a further
        // qualifier and fails immediately, blaming the head segment —
        // oracle-verified (typescript@7.0.2) against the single-segment
        // failure shape, not a `Head.Tail`-qualified one. Delegated to a
        // dedicated resolver so the (already correct, more common)
        // single-segment path below is untouched. (#17181)
        if full_member_path.contains('.') {
            return Some(self.resolve_jsdoc_import_type_qualified_chain(
                &module_specifier,
                &full_member_path,
                resolution_mode,
            ));
        }
        let member_name: &str = full_member_path.as_str();
        if let Some(sym_id) = self.resolve_jsdoc_import_member_with_mode(
            &module_specifier,
            member_name,
            resolution_mode,
        ) {
            // `import(...).Member` (without a leading `typeof`) first tries
            // `Member` as a type-eligible export (interface/class/enum/
            // type-alias/namespace/typedef) via `BareTypeReference` mode, the
            // same way a local bare name reference resolves.
            let resolved =
                self.resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::BareTypeReference);
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(Ok(resolved));
            }
            // Unlike a local bare identifier (which stays a TS2749
            // value-used-as-type error) and unlike the TS-syntax `type T =
            // import("./m").Member` walk (`import_type.rs`, which keeps
            // TS2694 for a plain value), tsc's JSDoc `import("./m").Member`
            // type-position query falls back to the exported value's own
            // type when `Member` has no type meaning — oracle-verified
            // (tsc 6.0.2): `@type {import("./dep").value}` resolves to
            // `number` for `export declare const value: number`, and to a
            // function export's call-signature type. `Member` is already
            // confirmed to exist (`sym_id` resolved above), so tsc never
            // reports TS2694 here; only the type-vs-value fallback differs.
            let value_resolved =
                self.resolve_jsdoc_symbol_type_with_mode(sym_id, JsdocNameMode::ValuePosition);
            if value_resolved != TypeId::ERROR && value_resolved != TypeId::UNKNOWN {
                return Some(Ok(value_resolved));
            }
        }
        if let Some(typedef_type) =
            self.resolve_import_type_jsdoc_typedef(&module_specifier, member_name, None)
        {
            return Some(Ok(typedef_type));
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
        if let Some((export_sym_id, export_file_idx)) = self.resolve_js_export_named_class_symbol(
            &module_specifier,
            member_name,
            Some(self.ctx.current_file_idx),
        ) {
            self.ctx
                .register_symbol_file_target(export_sym_id, export_file_idx);
            let resolved = self.resolve_jsdoc_symbol_type_with_mode(
                export_sym_id,
                JsdocNameMode::BareTypeReference,
            );
            if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                return Some(Ok(resolved));
            }
        }
        Some(Err(self.jsdoc_import_type_missing_member_display(
            &module_specifier,
            full_member_path.as_str(),
        )))
    }

    /// Build the `(namespace_display, missing_member)` pair for a JSDoc
    /// import-type reference whose (single-segment) member failed to
    /// resolve as any type-eligible export, CommonJS expando, or `@typedef`.
    ///
    /// tsc names the module by its resolved file path, not the written
    /// specifier (#17177), and when a leading portion of `full_member_path`
    /// names a namespace synthesized by dotted `@typedef`/`@callback`
    /// declarations, qualifies the namespace display with that declared
    /// prefix (#17162): `Dotted.Missing` with a declared `Dotted.Name`
    /// reports `Namespace '"m".Dotted' has no exported member 'Missing'`,
    /// not the unqualified root. When no declared name starts with the
    /// reference's head segment, the display stays unqualified.
    /// The TS2694 namespace text for a JSDoc `import(...)` reference, applying
    /// the export= naming rule on top of the JSDoc walk's already-resolved
    /// `module_path`. `resolved_segments` are the qualifier segments that
    /// resolved before the missing member (empty for a head/single-segment
    /// miss).
    ///
    /// Mirrors the TS-syntax `import_type_namespace_name{,_with_segments}`
    /// naming (#17208) so every TS2694 walk agrees: a named `export = <target>`
    /// roots at the target's own symbol name (`shape` / `shape.Bar`); an
    /// anonymous export= target uses `"<path>".export=` at the root and
    /// `"<path>".Bar` once segments traverse into it; a module without an
    /// export assignment uses `"<path>"` / `"<path>".Bar`.
    pub(crate) fn jsdoc_import_namespace_display(
        &self,
        module_specifier: &str,
        module_path: &str,
        resolved_segments: &[&str],
    ) -> String {
        let base = if self.target_module_has_export_equals(module_specifier) {
            self.export_equals_target_named_display(module_specifier)
                .unwrap_or_else(|| {
                    if resolved_segments.is_empty() {
                        format!("\"{module_path}\".export=")
                    } else {
                        format!("\"{module_path}\"")
                    }
                })
        } else {
            format!("\"{module_path}\"")
        };
        if resolved_segments.is_empty() {
            base
        } else {
            format!("{base}.{}", resolved_segments.join("."))
        }
    }

    fn jsdoc_import_type_missing_member_display(
        &mut self,
        module_specifier: &str,
        full_member_path: &str,
    ) -> (String, String) {
        let is_relative = module_specifier.starts_with("./") || module_specifier.starts_with("../");
        let namespace_name = is_relative
            .then(|| self.resolved_import_type_module_path(module_specifier, None))
            .flatten()
            .unwrap_or_else(|| self.imported_namespace_display_module_name(module_specifier));
        let segments: Vec<&str> = full_member_path.split('.').collect();

        // A module whose export surface is an export assignment is named by its
        // export= target (named target's own name, else `"<path>".export=`),
        // shared with the TS import-type walks (#17208). The dotted-`@typedef`
        // prefix qualification below is specific to modules without an export
        // assignment, so short-circuit here on the head/single-segment miss.
        if self.target_module_has_export_equals(module_specifier) {
            return (
                self.jsdoc_import_namespace_display(module_specifier, &namespace_name, &[]),
                segments[0].to_string(),
            );
        }
        let mut prefix_len = 0usize;
        for candidate_len in (1..segments.len()).rev() {
            if self.import_type_jsdoc_typedef_namespace_prefix_exists(
                module_specifier,
                &segments[..candidate_len].join("."),
                None,
            ) {
                prefix_len = candidate_len;
                break;
            }
        }
        let namespace_display = if prefix_len == 0 {
            format!("\"{namespace_name}\"")
        } else {
            format!("\"{namespace_name}\".{}", segments[..prefix_len].join("."))
        };
        (namespace_display, segments[prefix_len].to_string())
    }

    /// Resolve a qualified (multi-segment, non-typedef) JSDoc
    /// `import("./mod").A.B[.C…]` member reference once the whole-chain
    /// `@typedef` lookup has already missed.
    ///
    /// tsc requires every segment but the last to resolve in *namespace*
    /// meaning: the head (and each intermediate segment) must be a real
    /// namespace/module symbol whose own export table supplies the next
    /// segment. A class/interface/type-alias/enum head has no such meaning,
    /// so the reference fails at the head — falling through to the same
    /// resolved-path/declared-prefix-qualified display every other missing
    /// member uses, exactly like a single-segment miss (oracle:
    /// typescript@7.0.2, `import("./mod").SomeClass.Member` reports
    /// `Namespace '"mod"' has no exported member 'SomeClass'`, never a
    /// `'SomeClass.Member'`-qualified failure on `Member`). When the head
    /// genuinely is a namespace, tsc keeps walking and, on a failure deeper
    /// in the chain, qualifies the namespace display with the segments that
    /// did resolve (`Namespace '"mod".NS' has no exported member
    /// 'Missing'`).
    fn resolve_jsdoc_import_type_qualified_chain(
        &mut self,
        module_specifier: &str,
        qualified_member_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Result<TypeId, (String, String)> {
        let segments: Vec<&str> = qualified_member_name.split('.').collect();
        let head = segments[0];

        let Some(mut current_sym) =
            self.resolve_jsdoc_import_member_with_mode(module_specifier, head, resolution_mode)
        else {
            return Err(self.jsdoc_import_type_missing_member_display(
                module_specifier,
                qualified_member_name,
            ));
        };
        let is_namespace_like = |checker: &Self, sym| {
            checker
                .get_cross_file_symbol(sym)
                .or_else(|| checker.ctx.binder.get_symbol(sym))
                .is_some_and(|symbol| {
                    symbol.flags
                        & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE)
                        != 0
                })
        };
        if !is_namespace_like(&*self, current_sym) {
            return Err(self.jsdoc_import_type_missing_member_display(
                module_specifier,
                qualified_member_name,
            ));
        }

        let is_relative = module_specifier.starts_with("./") || module_specifier.starts_with("../");
        let namespace_name = is_relative
            .then(|| self.resolved_import_type_module_path(module_specifier, None))
            .flatten()
            .unwrap_or_else(|| self.imported_namespace_display_module_name(module_specifier));

        // The traversed-path root follows the same export= naming rule as
        // every other TS2694 walk (#17208), via `jsdoc_import_namespace_display`
        // — a named `export = <target>` roots at the target's own symbol name
        // (`shape.Bar`), a plain module at the quoted `"<path>"`. The head
        // reached here is namespace-like, so an anonymous export= target (whose
        // object-literal members are not namespace-like) never lands here.
        let mut resolved_segments = vec![head];
        for segment in &segments[1..] {
            let next = self
                .get_cross_file_symbol(current_sym)
                .or_else(|| self.ctx.binder.get_symbol(current_sym))
                .and_then(|symbol| {
                    symbol
                        .exports
                        .as_ref()
                        .and_then(|exports| exports.get(segment))
                        .or_else(|| {
                            symbol
                                .members
                                .as_ref()
                                .and_then(|members| members.get(segment))
                        })
                });
            let Some(next_sym) = next else {
                let display = self.jsdoc_import_namespace_display(
                    module_specifier,
                    &namespace_name,
                    &resolved_segments,
                );
                return Err((display, (*segment).to_string()));
            };
            current_sym = next_sym;
            resolved_segments.push(segment);
        }

        let resolved =
            self.resolve_jsdoc_symbol_type_with_mode(current_sym, JsdocNameMode::BareTypeReference);
        if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
            Ok(resolved)
        } else {
            let last = resolved_segments.pop().unwrap_or(head);
            let display = self.jsdoc_import_namespace_display(
                module_specifier,
                &namespace_name,
                &resolved_segments,
            );
            Err((display, last.to_string()))
        }
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
