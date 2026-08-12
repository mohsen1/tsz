//! JSDoc `@typedef` base-type validation helpers for `CheckerState`.
//!
//! This module owns TS2304 "Cannot find name" emission for unresolvable
//! simple-name base types referenced by `@typedef`, `@param`, and `@return`
//! JSDoc tags, along with the supporting visibility and name-resolution
//! helpers. Split out of `jsdoc/diagnostics.rs` to keep that file under the
//! checker line-count boundary; this is a pure code move with no behavior
//! change.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Eagerly validate base types of all `@typedef` declarations in the file.
    /// Emits TS2304 "Cannot find name" for unresolvable simple-name base types.
    pub(crate) fn check_jsdoc_typedef_base_types(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        if sf.comments.is_empty() {
            return;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_text);
            let end = (comment.end as usize).min(source_text.len());
            let comment_text = &source_text[comment.pos as usize..end];
            for (type_expr, offset_in_comment) in Self::jsdoc_param_return_type_spans(comment_text)
            {
                self.validate_jsdoc_param_namespace_member_errors(
                    &type_expr,
                    comment.pos,
                    offset_in_comment,
                );
                let line_start = comment_text[..offset_in_comment]
                    .rfind('\n')
                    .map_or(0, |idx| idx + 1);
                // Single-line JSDoc (`/** @param {T} y */`) needs the
                // `/**` prefix stripped before tag detection works;
                // multi-line JSDoc lines start with `* …` and need the
                // existing `*` strip. Apply both unconditionally —
                // `trim_start_matches` is a no-op when the prefix is
                // absent.
                let line = comment_text[line_start..]
                    .trim_start()
                    .trim_start_matches("/**")
                    .trim_start_matches('*')
                    .trim_start();
                let is_param_tag = Self::strip_jsdoc_tag_prefix(line, "param").is_some();
                let is_return_tag = Self::strip_jsdoc_return_tag_prefix(line).is_some();
                if !is_param_tag && !is_return_tag {
                    continue;
                }
                // A bare `import('mod')` names the module's `export =` type. The
                // module's own namespace is not a type, so a module exporting only
                // values — or one with named type exports and no `export =` — is
                // TS1340 even though the specifier resolves. The TypeScript
                // import-type resolver reports this already; JSDoc reaches the
                // question by a separate path, so it asks the shared predicate
                // here, where the type expression's offset is known.
                // `import('mod').Member` is unaffected: it is not a bare import.
                {
                    let raw = type_expr.trim();
                    if let Some((module_specifier, None)) = Self::parse_jsdoc_import_type(raw)
                        && !self.bare_import_type_names_a_type(
                            &module_specifier,
                            Self::jsdoc_import_type_resolution_mode(raw),
                        )
                    {
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
                            &[&module_specifier],
                        );
                        self.error_at_position(
                            comment.pos + offset_in_comment as u32,
                            raw.len() as u32,
                            &message,
                            crate::diagnostics::diagnostic_codes::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
                        );
                        continue;
                    }
                }
                let simple_expr = type_expr
                    .trim()
                    .trim_start_matches('!')
                    .trim_end_matches('=')
                    .trim();
                if self
                    .resolve_jsdoc_implicit_any_builtin_type(simple_expr)
                    .is_some()
                    || crate::types_domain::queries::lib_resolution::keyword_name_to_type_id(
                        simple_expr,
                    )
                    .is_some()
                    || self.jsdoc_template_in_scope_for_reference(
                        simple_expr,
                        &content,
                        comment.pos,
                    )
                    || Self::parse_jsdoc_typedefs(&source_text)
                        .iter()
                        .any(|(name, _)| name == simple_expr)
                {
                    continue;
                }
                let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                // Anchor at this tag's own type-expression start (not the shared
                // comment start) so an `import(...).Member` TS2694 lands on the
                // member token of *this* tag, even when another tag in the same
                // comment carries the identical type expression (issue #17176).
                self.ctx
                    .jsdoc_typedef_anchor_pos
                    .set(comment.pos + offset_in_comment as u32);
                // The comment-scan validation pass owns the single import-type
                // member TS2694 (anchored above at this tag's member token); the
                // lazy type computations resolve the same string silently.
                let unresolved_type = {
                    let _diag = crate::jsdoc::resolution::import_type_member_diag::ImportTypeMemberDiagGuard::active();
                    self.resolve_jsdoc_type_str(simple_expr)
                        .is_none_or(|ty| self.jsdoc_resolved_type_is_unresolved(simple_expr, ty))
                };
                self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
                if Self::is_simple_type_name(simple_expr) && unresolved_type {
                    if let Some(angle_idx) = Self::find_top_level_char(simple_expr, '<')
                        && simple_expr.ends_with('>')
                    {
                        let raw_base_name = simple_expr[..angle_idx].trim();
                        let base_name = raw_base_name.strip_suffix('.').unwrap_or(raw_base_name);
                        let base_is_known = self
                            .jsdoc_generic_base_suppresses_full_name_error(base_name)
                            || Self::parse_jsdoc_typedefs(&source_text)
                                .iter()
                                .any(|(name, _)| name == base_name);
                        if base_is_known {
                            continue;
                        }
                    }
                    // Suppress the generic "Cannot find name" emitter for
                    // qualified names whose root is a (possibly aliased)
                    // namespace, module, or import alias. tsc owns these
                    // diagnostics through namespace-member resolution — it
                    // either accepts the reference silently (valid export) or
                    // emits the namespace-specific TS2694 ("Namespace 'X' has
                    // no exported member 'Y'"). Emitting TS2304/TS2552
                    // "Cannot find name 's.X'" here would conflict with both
                    // outcomes.
                    if let Some(dot_idx) = simple_expr.find('.') {
                        let root_name = simple_expr[..dot_idx].trim();
                        if self.jsdoc_qualified_root_is_namespace_or_alias(root_name) {
                            continue;
                        }
                    }
                    self.emit_jsdoc_cannot_find_name(
                        simple_expr,
                        comment.pos,
                        comment.end,
                        &source_text,
                    );
                } else if !Self::is_simple_type_name(simple_expr) {
                    let template_params: Vec<String> = Self::jsdoc_template_type_params(&content)
                        .into_iter()
                        .map(|(name, _is_const, _default)| name)
                        .collect();
                    let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                    self.ctx.jsdoc_typedef_anchor_pos.set(comment.pos);
                    self.report_jsdoc_unresolved_inner_type_leaves(
                        simple_expr,
                        comment.pos,
                        comment.end,
                        &source_text,
                        &template_params,
                    );
                    self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
                }
            }

            // TS1109: Check for malformed @import tags (bare @import or missing module specifier)
            {
                let mut search_from = 0;
                while let Some(idx) = Self::jsdoc_tag_offset(&comment_text[search_from..], "import")
                {
                    let abs_idx = search_from + idx;
                    let after_import = abs_idx + "@import".len();
                    let rest_full = &comment_text[after_import..];
                    let next_tag = rest_full
                        .lines()
                        .skip(1)
                        .enumerate()
                        .find_map(|(i, line)| {
                            let trimmed = line.trim_start().trim_start_matches('*').trim();
                            if trimmed.starts_with('@') {
                                let offset: usize =
                                    rest_full.lines().take(i + 1).map(|l| l.len() + 1).sum();
                                Some(offset)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(rest_full.len());
                    let raw_slice = rest_full[..next_tag]
                        .trim_end()
                        .trim_end_matches("*/")
                        .trim_end();
                    let joined: String = raw_slice
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let joined = joined.trim();

                    // JSDoc `@import` attribute diagnostics:
                    // - TS2823/TS1464/TS1005 for malformed `with` clauses (e.g. `with` without `{...}`)
                    // - TS2857 when an attribute object is present but invalid for type-only import tags
                    let raw_import_clause = &rest_full[..next_tag];
                    if let Some(with_off) = raw_import_clause.find("with") {
                        let attr_part = raw_import_clause[with_off + 4..]
                            .trim()
                            .trim_end_matches("*/")
                            .trim();
                        let attr_trimmed = attr_part.trim_start();
                        if !attr_trimmed.starts_with('{') {
                            let with_pos = comment.pos + after_import as u32 + with_off as u32;
                            self.error_at_position(
                                with_pos,
                                4,
                                crate::diagnostics::diagnostic_messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                                crate::diagnostics::diagnostic_codes::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                            );
                            self.error_at_position(
                                with_pos,
                                4,
                                crate::diagnostics::diagnostic_messages::TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                                crate::diagnostics::diagnostic_codes::TYPE_IMPORT_ATTRIBUTES_SHOULD_HAVE_EXACTLY_ONE_KEY_RESOLUTION_MODE_WITH_VALUE_IM,
                            );

                            // TS1005: after `with`, parser expects `{`.
                            let after_with = &raw_import_clause[with_off + 4..];
                            let ws_len = after_with.len() - after_with.trim_start().len();
                            let expected_pos =
                                comment.pos + after_import as u32 + (with_off + 4 + ws_len) as u32;
                            self.error_at_position(
                                expected_pos,
                                1,
                                "'{' expected.",
                                crate::diagnostics::diagnostic_codes::EXPECTED,
                            );

                            // TS2306 at the module specifier location when target isn't a module.
                            if let Some((_local, specifier, _import_name)) =
                                Self::parse_jsdoc_import_tag(raw_import_clause)
                                    .into_iter()
                                    .next()
                            {
                                let quoted_spec = format!("\"{specifier}\"");
                                let single_quoted_spec = format!("'{specifier}'");
                                let spec_off = raw_import_clause
                                    .find(&quoted_spec)
                                    .or_else(|| raw_import_clause.find(&single_quoted_spec))
                                    .unwrap_or(with_off);
                                let spec_pos = comment.pos + after_import as u32 + spec_off as u32;
                                let spec_len = (specifier.len() + 2) as u32;

                                if let Some(target_idx) = self.ctx.resolve_import_target(&specifier)
                                    && let Some(target_binder) =
                                        self.ctx.get_binder_for_file(target_idx)
                                    && !target_binder.is_external_module
                                {
                                    let target_arena =
                                        self.ctx.get_arena_for_file(target_idx as u32);
                                    if let Some(target_sf) = target_arena.source_files.first() {
                                        let display_name = target_sf
                                            .file_name
                                            .rsplit('/')
                                            .next()
                                            .unwrap_or(&target_sf.file_name)
                                            .rsplit('\\')
                                            .next()
                                            .unwrap_or(&target_sf.file_name)
                                            .to_string();
                                        let message = crate::diagnostics::format_message(
                                            crate::diagnostics::diagnostic_messages::FILE_IS_NOT_A_MODULE,
                                            &[&display_name],
                                        );
                                        self.error_at_position(
                                            spec_pos,
                                            spec_len,
                                            &message,
                                            crate::diagnostics::diagnostic_codes::FILE_IS_NOT_A_MODULE,
                                        );
                                    }
                                }
                            }
                        } else {
                            let has_resolution_mode = attr_part.contains("resolution-mode");
                            let is_resolution_mode_only = has_resolution_mode
                                && !attr_part.contains(',')
                                && !attr_part.contains(" type ")
                                && !attr_part.contains(" type:")
                                && !attr_part.contains("{type")
                                && !attr_part.contains("{ type");
                            if !is_resolution_mode_only {
                                self.error_at_position(
                                    comment.pos + after_import as u32 + with_off as u32,
                                    4,
                                    crate::diagnostics::diagnostic_messages::IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS,
                                    crate::diagnostics::diagnostic_codes::IMPORT_ATTRIBUTES_CANNOT_BE_USED_WITH_TYPE_ONLY_IMPORTS_OR_EXPORTS,
                                );
                            }
                        }
                    }

                    if let Some(from_idx) = joined.rfind("from") {
                        let before_from = joined[..from_idx].trim();
                        if matches!(
                            before_from.split_whitespace().next(),
                            Some("type" | "defer")
                        ) && before_from.contains(char::is_whitespace)
                        {
                            let modifier =
                                before_from.split_whitespace().next().unwrap_or_default();
                            if let Some(modifier_idx) = rest_full[..next_tag].find(modifier) {
                                let error_pos = comment.pos
                                    + after_import as u32
                                    + modifier_idx as u32
                                    + modifier.len() as u32
                                    + 1;
                                self.error_at_position(
                                    error_pos,
                                    1,
                                    "'from' expected.",
                                    crate::diagnostics::diagnostic_codes::EXPECTED,
                                );
                                // tsc also expects a string-literal module
                                // specifier at the same spot the clause breaks
                                // (`@import defer * as ns from …`).
                                self.error_at_position(
                                    error_pos,
                                    1,
                                    crate::diagnostics::diagnostic_messages::STRING_LITERAL_EXPECTED,
                                    crate::diagnostics::diagnostic_codes::STRING_LITERAL_EXPECTED,
                                );
                                search_from = after_import;
                                continue;
                            }
                        }
                    }

                    if joined.is_empty() {
                        self.error_expression_expected_at_position(
                            comment.pos + after_import as u32,
                            1,
                        );
                    } else if joined.contains("from")
                        && !joined.contains('"')
                        && !joined.contains('\'')
                        && let Some(from_off) = rest_full[..next_tag].rfind("from")
                    {
                        self.error_expression_expected_at_position(
                            comment.pos + after_import as u32 + from_off as u32 + 4,
                            1,
                        );
                    } else if !joined.is_empty() && !joined.contains("from") {
                        // TS1005: @import clause without 'from' keyword, e.g.:
                        //   @import x = require("types")  — should be: @import { x } from "types"
                        //   @import Foo                    — missing 'from "module"'
                        // Find the position after the import clause (first identifier)
                        // where 'from' is expected.
                        let rest_trimmed = rest_full.trim_start();
                        let skip_ws = rest_full.len() - rest_trimmed.len();
                        // Skip past the first identifier-like characters
                        let clause_end = rest_trimmed
                            .find(|c: char| {
                                !c.is_alphanumeric()
                                    && c != '_'
                                    && c != '{'
                                    && c != '}'
                                    && c != '*'
                                    && c != ' '
                                    && c != ','
                            })
                            .unwrap_or(rest_trimmed.len());
                        let expr_pos =
                            comment.pos + after_import as u32 + skip_ws as u32 + clause_end as u32;
                        // Check if the rest after the import clause is just whitespace/comment
                        // markers (bare import like `@import foo`), or has additional content
                        // like `@import x = require("types")`.
                        let after_clause = rest_trimmed[clause_end..].trim();
                        let after_clause_clean = after_clause
                            .trim_end_matches("*/")
                            .trim()
                            .trim_start_matches('*')
                            .trim();
                        let is_bare_import = after_clause_clean.is_empty();
                        if is_bare_import {
                            // TS1109: Expression expected — at the position after the bare
                            // import clause where the module specifier was expected.
                            // tsc only emits this for bare imports (e.g., `@import foo`).
                            self.error_expression_expected_at_position(expr_pos, 1);
                            // TS1005: 'from' expected — tsc emits this at the closing `*/`
                            // of the JSDoc comment for bare imports.
                            let from_error_pos = if comment.end >= 2 {
                                comment.end - 2
                            } else {
                                expr_pos
                            };
                            self.error_at_position(
                                from_error_pos,
                                1,
                                "'from' expected.",
                                crate::diagnostics::diagnostic_codes::EXPECTED,
                            );
                        } else {
                            // For imports with additional content (e.g., `@import x = require("types")`),
                            // emit TS1005 'from' expected at the import clause
                            // position (not TS1109), plus TS1141 for the missing
                            // string-literal specifier tsc reports at the same spot.
                            self.error_at_position(
                                expr_pos,
                                1,
                                "'from' expected.",
                                crate::diagnostics::diagnostic_codes::EXPECTED,
                            );
                            self.error_at_position(
                                expr_pos,
                                1,
                                crate::diagnostics::diagnostic_messages::STRING_LITERAL_EXPECTED,
                                crate::diagnostics::diagnostic_codes::STRING_LITERAL_EXPECTED,
                            );
                        }
                    }
                    search_from = after_import;
                }
            }

            for (_name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                let template_names: Vec<String> = typedef_info
                    .template_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect();
                if let Some(ref cb) = typedef_info.callback {
                    // Check callback param types for unresolvable references (TS2304)
                    for param in &cb.params {
                        let Some(type_expr) = param.type_expr.as_deref() else {
                            continue;
                        };
                        let expr = type_expr.trim();
                        let expr = expr.strip_prefix("...").unwrap_or(expr);
                        if expr.is_empty() {
                            continue;
                        }
                        if !Self::is_simple_type_name(expr) {
                            continue;
                        }
                        // Skip template params defined in this same comment
                        if template_names.iter().any(|t| t == expr) {
                            continue;
                        }
                        if self.resolve_jsdoc_type_str(expr).is_none() {
                            self.emit_jsdoc_cannot_find_name(
                                expr,
                                comment.pos,
                                comment.end,
                                &source_text,
                            );
                        }
                    }
                    continue;
                }
                for prop in &typedef_info.properties {
                    let expr = prop.type_expr.trim().trim_end_matches('=').trim();
                    if expr.is_empty() || expr == "Object" || expr == "object" {
                        continue;
                    }
                    if !Self::is_simple_type_name(expr) {
                        continue;
                    }
                    if template_names.iter().any(|t| t == expr) {
                        continue;
                    }
                    if self.resolve_jsdoc_type_str(expr).is_none() {
                        self.emit_jsdoc_cannot_find_name(
                            expr,
                            comment.pos,
                            comment.end,
                            &source_text,
                        );
                    }
                }
                if let Some(ref base_type) = typedef_info.base_type {
                    let expr = base_type.trim();
                    if expr == "Object" || expr == "object" || expr.is_empty() {
                        continue;
                    }
                    // A bare `import('mod')` as a typedef base follows the same
                    // rule as in `@param`/`@return`: it names the module's
                    // exported type, and a module without one is TS1340.
                    // Witness: jsdocTypeReferenceToImportOfFunctionExpression,
                    // whose module exports a plain function.
                    let typedef_comment_text = comment.get_text(&source_text);
                    // Only a real `@typedef` base. An `@import` tag also carries a
                    // module specifier but is a value import, not a type
                    // reference, and must not be flagged (witness: importTag2/3/9/…).
                    if Self::jsdoc_tag_offset(typedef_comment_text, "typedef").is_some()
                        && let Some(offset_in_comment) = typedef_comment_text.find(expr)
                        && let Some((module_specifier, None)) = Self::parse_jsdoc_import_type(expr)
                        && !self.bare_import_type_names_a_type(
                            &module_specifier,
                            Self::jsdoc_import_type_resolution_mode(expr),
                        )
                    {
                        let anchor = comment.pos + offset_in_comment as u32;
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
                            &[&module_specifier],
                        );
                        self.error_at_position(
                            anchor,
                            expr.len() as u32,
                            &message,
                            crate::diagnostics::diagnostic_codes::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
                        );
                        continue;
                    }
                    // TS2344: Check constraint satisfaction for import type refs with generics.
                    // e.g., @typedef {import('./file1').Foo<T>} Bar
                    if expr.starts_with("import(")
                        && let Some(angle_idx) = Self::find_top_level_char(expr, '<')
                        && expr.ends_with('>')
                    {
                        let import_base = expr[..angle_idx].trim();
                        let args_str = &expr[angle_idx + 1..expr.len() - 1];
                        let arg_strs = Self::split_type_args_respecting_nesting(args_str);
                        if !arg_strs.is_empty()
                            && let Some((module_specifier, Some(member_name))) =
                                Self::parse_jsdoc_import_type(import_base)
                        {
                            self.report_jsdoc_import_type_constraint_error(
                                crate::jsdoc::diagnostics_import_type_constraints::JsdocImportTypeConstraintDiagnostic {
                                    expr,
                                    angle_idx,
                                    arg_strs: &arg_strs,
                                    module_specifier: &module_specifier,
                                    member_name: &member_name,
                                    typedef_info: &typedef_info,
                                    comment_pos: comment.pos,
                                    comment_end: comment.end,
                                    source_text: &source_text,
                                },
                            );
                        }
                        continue;
                    }
                    // Only validate simple identifier names — complex type expressions
                    // like `function(string): boolean` or `{num: number}` will naturally
                    // fail resolution and produce false TS2304 errors.
                    if !Self::is_simple_type_name(expr) {
                        continue;
                    }
                    self.validate_jsdoc_typedef_body_expr(
                        expr,
                        &template_names,
                        comment.pos,
                        comment.end,
                        &source_text,
                    );
                }
            }
        }

        // Also check @type tag references for unresolvable simple names (TS2304).
        // Only for JSDoc comments that are actually attached to top-level statements.
        // Inline expression-body casts like `value => /** @type {T} */(...)` should not
        // be treated as file-level tags; those are validated in the normal checker flow
        // where function-scoped `@template` params are available.
        // A `@type` on a variable statement nested in a function body is just as
        // much a real annotation as a top-level one, and `tsc` validates it —
        // `function f() { /** @type {Missing} */ var y }` reports TS2304, and the
        // same shape with a value name reports TS2749. Collect those leading
        // comments so the scan below covers them too. Inline expression casts
        // still fall through, because they lead no statement.
        let nested_statement_jsdoc: rustc_hash::FxHashSet<u32> = {
            let mut positions = rustc_hash::FxHashSet::default();
            for raw_idx in 0..self.ctx.arena.len() {
                let idx = tsz_parser::parser::NodeIndex(raw_idx as u32);
                let Some((kind, pos)) = self.ctx.arena.get(idx).map(|node| (node.kind, node.pos))
                else {
                    continue;
                };
                // Variable statements carry most nested annotations, but a JS
                // class routinely declares its fields as `/** @type {T} */
                // this.x = ...` inside the constructor — an *expression*
                // statement. tsc validates that annotation too (witness:
                // jsDeclarationsReferenceToClassInstanceCrossFile).
                if !matches!(
                    kind,
                    tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT
                        | tsz_parser::parser::syntax_kind_ext::EXPRESSION_STATEMENT
                ) {
                    continue;
                }
                if let Some((_, comment_pos)) =
                    self.try_leading_jsdoc_with_pos(&comments, pos, &source_text)
                {
                    positions.insert(comment_pos);
                }
            }
            positions
        };

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }

            let is_top_level_leading_jsdoc = sf.statements.nodes.iter().any(|&stmt_idx| {
                self.ctx
                    .arena
                    .get(stmt_idx)
                    .and_then(|stmt| {
                        self.try_leading_jsdoc_with_pos(&comments, stmt.pos, &source_text)
                    })
                    .is_some_and(|(_, comment_pos)| comment_pos == comment.pos)
            });
            let is_nested_statement_jsdoc =
                !is_top_level_leading_jsdoc && nested_statement_jsdoc.contains(&comment.pos);
            if !is_top_level_leading_jsdoc && !is_nested_statement_jsdoc {
                continue;
            }

            let content = get_jsdoc_content(comment, &source_text);
            // Check for @type {Name} where Name is a simple identifier
            if let Some(type_expr) = Self::jsdoc_extract_type_tag_expr(&content) {
                let expr = type_expr.trim();
                let comment_text = comment.get_text(&source_text);
                let type_expr_start = comment_text
                    .find(expr)
                    .map(|offset| comment.pos + offset as u32)
                    .unwrap_or(comment.pos);
                if self.report_jsdoc_backtick_import_type_error(expr, type_expr_start) {
                    continue;
                }
                self.report_jsdoc_param_generic_instantiation_errors(expr, type_expr_start);

                let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                // Anchor at the `@type` expression start so an
                // `import(...).Member` TS2694 lands on the member token
                // (issue #17176), not at the `/**` comment start.
                self.ctx.jsdoc_typedef_anchor_pos.set(type_expr_start);
                // The comment-scan validation pass owns the single import-type
                // member TS2694 (anchored above at the member token); the lazy
                // declaration-type computation resolves the same string silently.
                let resolved = {
                    let _diag = crate::jsdoc::resolution::import_type_member_diag::ImportTypeMemberDiagGuard::active();
                    self.resolve_jsdoc_type_str(expr)
                };
                self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
                let unresolved = resolved.is_none()
                    || resolved.is_some_and(|ty| self.jsdoc_resolved_type_is_unresolved(expr, ty));

                if let Some((module_specifier, _segments)) =
                    Self::parse_jsdoc_typeof_import_query(expr)
                {
                    let has_ambient_module = self
                        .ctx
                        .declared_modules_contains(self.ctx.binder, &module_specifier)
                        || self
                            .ctx
                            .binder
                            .shorthand_ambient_modules
                            .contains(&module_specifier);
                    let rooted_specifier = module_specifier.starts_with('/');
                    let resolves = self
                        .ctx
                        .resolve_import_target_from_file(
                            self.ctx.current_file_idx,
                            &module_specifier,
                        )
                        .is_some()
                        || self.ctx.resolve_import_target(&module_specifier).is_some();
                    let should_emit_module_not_found = if rooted_specifier {
                        !has_ambient_module
                    } else {
                        !resolves && !has_ambient_module
                    };

                    if should_emit_module_not_found {
                        let end = (comment.end as usize).min(source_text.len());
                        let comment_range = &source_text[comment.pos as usize..end];
                        let (start, length) =
                            if let Some(import_offset) = comment_range.find("import(") {
                                let mut cursor = import_offset + "import(".len();
                                while cursor < comment_range.len()
                                    && comment_range.as_bytes()[cursor].is_ascii_whitespace()
                                {
                                    cursor += 1;
                                }
                                (
                                    comment.pos + cursor as u32,
                                    (module_specifier.len() as u32).saturating_add(2),
                                )
                            } else {
                                (
                                    comment.pos,
                                    (module_specifier.len() as u32).saturating_add(2),
                                )
                            };
                        let (message, code) = self.module_not_found_diagnostic_for_site(
                            &module_specifier,
                            crate::import::core::ModuleNotFoundSite::ImportType,
                        );
                        self.error_at_position(start, length, &message, code);
                        continue;
                    }
                }

                if unresolved && let Some(dot_idx) = expr.find('.') {
                    let root_name = expr[..dot_idx].trim();
                    if !root_name.is_empty()
                        && Self::is_simple_type_name(root_name)
                        && root_name != "globalThis"
                        && self.resolve_jsdoc_entity_name_symbol(root_name).is_none()
                    {
                        let end = (comment.end as usize).min(source_text.len());
                        let comment_range = &source_text[comment.pos as usize..end];
                        let (start, length) = if let Some(offset) = comment_range.find(root_name) {
                            (comment.pos + offset as u32, root_name.len() as u32)
                        } else {
                            (comment.pos, root_name.len() as u32)
                        };
                        self.ctx.error(
                            start,
                            length,
                            crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::CANNOT_FIND_NAMESPACE,
                                &[root_name],
                            ),
                            crate::diagnostics::diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        );
                        continue;
                    }
                }
                let skip_cannot_find_name = expr.is_empty()
                    || !Self::is_simple_type_name(expr)
                    || expr.contains('<')
                    || expr.contains('.')
                    || !unresolved
                    // A `@typedef` written inside a function body is in scope for
                    // that function's own `@type` tags (witness: typedefScope1,
                    // where `B` is declared and used within `B1`). This scan does
                    // not carry function scopes, so for a nested tag defer to any
                    // `@typedef` of that name rather than report a false TS2304.
                    // The top-level path is unaffected and still reports a
                    // top-level use of a function-scoped typedef.
                    || (is_nested_statement_jsdoc
                        && Self::parse_jsdoc_typedefs(&source_text)
                            .iter()
                            .any(|(name, _)| name == expr))
                    // In JS files, `exports`, `module`, `require`, `global`
                    // are CommonJS built-ins that always resolve at runtime
                    // even if the checker's type system doesn't create a
                    // user-land binding for them.  tsc does not flag them
                    // as "Cannot find name" in JSDoc @type contexts.
                    // `exports`, `module`, `require`, `global` are CommonJS
                    // built-ins that resolve at runtime even without a
                    // user-land binding, so tsc does not flag them as
                    // "Cannot find name". It does report `exports`/`module`
                    // as TS2749 once the file assigns to them — they are the
                    // module object, a value — so let those reach the emitter,
                    // which picks TS2749 for a value used as a type.
                    || (self.ctx.is_js_file()
                        && (matches!(expr, "require" | "global")
                            || (matches!(expr, "exports" | "module")
                                && !self.current_file_has_commonjs_export_assignment())));
                if !skip_cannot_find_name {
                    self.emit_jsdoc_cannot_find_name(expr, comment.pos, comment.end, &source_text);
                } else if !Self::is_simple_type_name(expr) && !expr.is_empty() {
                    let template_params: Vec<String> = Self::jsdoc_template_type_params(&content)
                        .into_iter()
                        .map(|(name, _is_const, _default)| name)
                        .collect();
                    let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
                    self.ctx.jsdoc_typedef_anchor_pos.set(comment.pos);
                    self.report_jsdoc_unresolved_inner_type_leaves(
                        expr,
                        comment.pos,
                        comment.end,
                        &source_text,
                        &template_params,
                    );
                    self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
                }
            }
        }

        // Check @param and @return/@returns type references for missing required type arguments
        // (TS2314) when noImplicitAny is enabled. This covers bare generic lib types like
        // `Array` and `Promise` that resolve to `X<any>` under normal rules but require a
        // type argument when strict/noImplicitAny is active. Only lib-provided types are
        // checked here; user-defined generic types in file_locals are already handled by
        // resolve_jsdoc_param_type_with_pos during function type construction.
        if self.ctx.no_implicit_any() {
            for comment in &comments {
                if !is_jsdoc_comment(comment, &source_text) {
                    continue;
                }
                let end = (comment.end as usize).min(source_text.len());
                let comment_text = &source_text[comment.pos as usize..end];
                let type_spans = Self::jsdoc_bare_param_return_type_spans(comment_text);
                for (type_expr, offset_in_comment) in type_spans {
                    // Only check lib builtin types that are skipped by the @param path.
                    // resolve_jsdoc_implicit_any_builtin_type returns Some for types like
                    // Array and Promise, causing required_generic_count_for_jsdoc_type_name
                    // to early-return None (no TS2314 from the params path). This pass fills
                    // that gap when noImplicitAny is enabled.
                    // User-defined generic types are handled by required_generic_count_for_jsdoc_type_name
                    // directly (they get None from resolve_jsdoc_implicit_any_builtin_type,
                    // so the params path does check them).
                    if self
                        .resolve_jsdoc_implicit_any_builtin_type(type_expr.as_str())
                        .is_none()
                    {
                        continue;
                    }
                    // Look up the type in global lib/ambient symbols.
                    let sym_id = {
                        use tsz_binder::symbol_flags;
                        self.ctx
                            .binder
                            .get_symbols()
                            .find_all_by_name(type_expr.as_str())
                            .iter()
                            .copied()
                            .find(|&s| {
                                self.ctx.binder.get_symbol(s).is_some_and(|sym| {
                                    (sym.flags
                                        & (symbol_flags::TYPE_ALIAS
                                            | symbol_flags::CLASS
                                            | symbol_flags::INTERFACE
                                            | symbol_flags::ENUM))
                                        != 0
                                })
                            })
                    };
                    let Some(sym_id) = sym_id else {
                        continue;
                    };
                    let type_params = self.type_reference_symbol_type_with_params(sym_id).1;
                    if type_params.is_empty() {
                        continue;
                    }
                    let min_required = type_params.iter().filter(|p| p.default.is_none()).count();
                    if min_required == 0 {
                        continue;
                    }
                    let max_expected = type_params.len();
                    let display_name = Self::format_generic_display_name_with_interner(
                        type_expr.as_str(),
                        &type_params,
                        self.ctx.types,
                    );
                    let abs_pos = comment.pos + offset_in_comment as u32;
                    use crate::diagnostics::{
                        diagnostic_codes as dc, diagnostic_messages as dm, format_message,
                    };
                    let (message, code) = if min_required < max_expected {
                        (
                            format_message(
                                dm::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                                &[
                                    &display_name,
                                    &min_required.to_string(),
                                    &max_expected.to_string(),
                                ],
                            ),
                            dc::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                        )
                    } else {
                        (
                            format_message(
                                dm::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                                &[&display_name, &max_expected.to_string()],
                            ),
                            dc::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                        )
                    };
                    self.error_at_position(abs_pos, type_expr.len() as u32, &message, code);
                }
            }
        }
    }

    pub(crate) fn report_jsdoc_backtick_import_type_error(
        &mut self,
        type_expr: &str,
        type_expr_start: u32,
    ) -> bool {
        let Some(offset) = Self::jsdoc_backtick_import_argument_offset(type_expr) else {
            return false;
        };
        let start = type_expr_start + offset as u32;
        if self.ctx.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == crate::diagnostics::diagnostic_codes::STRING_LITERAL_EXPECTED
                && diagnostic.file == self.ctx.file_name
                && diagnostic.start == start
        }) {
            return true;
        }
        self.error_at_position(
            start,
            1,
            crate::diagnostics::diagnostic_messages::STRING_LITERAL_EXPECTED,
            crate::diagnostics::diagnostic_codes::STRING_LITERAL_EXPECTED,
        );
        true
    }

    /// Emit TS2304 "Cannot find name 'X'" or TS2552 "Did you mean 'Y'?" for an
    /// unresolvable JSDoc type reference. Locates the name within the comment text
    /// range for precise error positioning, then attempts spelling suggestions
    /// to match tsc's behavior of upgrading to TS2552 when a close match exists.
    pub(crate) fn emit_jsdoc_cannot_find_name(
        &mut self,
        name: &str,
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Suppress TS2304 when `name` matches an `@template` declaration on
        // any JSDoc comment in the source file. tsc accepts class/function/
        // typedef-level `@template T` as an in-scope type-parameter name for
        // any JSDoc reference within the file, so we cannot flag a class
        // method's `@param {T}` as "Cannot find name 'T'" when an earlier
        // class-level `/** @template T */` declares it. This guard centralizes
        // the cross-comment scope at the diagnostic emitter rather than
        // threading template-name sets through every caller.
        if self.is_js_file()
            && Self::is_simple_type_name(name)
            && self.source_file_declares_jsdoc_template_at(name, comment_pos)
        {
            return;
        }

        let end = (comment_end as usize).min(source_text.len());
        let comment_range = &source_text[comment_pos as usize..end];
        let (start, length) = if let Some(offset) = comment_range.find(name) {
            (comment_pos + offset as u32, name.len() as u32)
        } else {
            (comment_pos, 0)
        };

        // A name that resolves to a value (function, variable, …) but not a type
        // is a "value used as a type" error (TS2749), not a missing name (TS2304).
        //
        // `exports`/`module` are values in a file that assigns to them — the
        // module object. Their symbol carries the MODULE flag, which
        // `jsdoc_name_refers_to_value_only` deliberately excludes, so name them
        // here rather than loosening that predicate for real namespaces.
        let is_commonjs_module_value = self.is_js_file()
            && matches!(name, "exports" | "module")
            && self.current_file_has_commonjs_export_assignment();
        if is_commonjs_module_value || self.jsdoc_name_refers_to_value_only(name) {
            let code = diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF;
            let template = tsz_common::diagnostics::get_message_template(code)
                .unwrap_or("'{0}' refers to a value, but is being used as a type here. Did you mean 'typeof {0}'?");
            let message = crate::diagnostics::format_message(template, &[name]);
            self.error_at_position(start, length, &message, code);
            return;
        }

        // Try spelling suggestions (e.g. "sting" → "string") to emit TS2552
        // instead of plain TS2304, matching tsc behavior.
        if let Some(suggestion) = self.find_jsdoc_type_spelling_suggestion(name) {
            let message = format!("Cannot find name '{name}'. Did you mean '{suggestion}'?");
            self.error_at_position(
                start,
                length,
                &message,
                diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN,
            );
            return;
        }

        self.error_cannot_find_name_at_position(name, start, length);
    }

    /// Validate a JSDoc `@typedef` body type expression for unresolvable
    /// names (TS2304). Recurses into nested generic arguments so that
    /// `@typedef {Record<string, Array<Missing>>} T` reports `Missing`,
    /// matching tsc's per-identifier diagnostic instead of treating
    /// `Array<Missing>` as one opaque missing name.
    ///
    /// `expr` is expected to already be trimmed and to satisfy
    /// `is_simple_type_name` — the caller filters out function types,
    /// object literals, and unions before reaching here.
    fn validate_jsdoc_typedef_body_expr(
        &mut self,
        expr: &str,
        template_names: &[String],
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
    ) {
        if expr.is_empty() || !Self::is_simple_type_name(expr) {
            return;
        }
        if template_names.iter().any(|t| t == expr) {
            return;
        }
        // Forward / recursive references like
        // `@typedef {ReadonlyArray<Json>} JsonArray` declared above the
        // `@typedef Json` itself can fail point-in-time resolution while
        // still being valid. Skip when a visible `@typedef Name` matches
        // — text-based, so it is immune to typedef resolution
        // re-entrancy. Cross-file typedefs are visible only from global
        // scripts; typedefs inside external modules require imports.
        if self.jsdoc_typedef_named_visible(expr) {
            return;
        }

        // Generic shape: `Name<arg, arg, ...>`. Recurse into each inner
        // arg when the base resolves; otherwise the unresolvable name is
        // the base, not the whole expression. Issue #3137.
        if let Some(angle_idx) = Self::find_top_level_char(expr, '<')
            && expr.ends_with('>')
        {
            // JSDoc allows `Object.<K, V>` / `Array.<T>` (dot-generic
            // form) — the trailing `.` is part of the syntax, not part
            // of the base name. Strip it before resolution so the base
            // looks up `Object`, `Array`, etc.
            let raw_base = expr[..angle_idx].trim();
            let base_name = raw_base.strip_suffix('.').unwrap_or(raw_base);
            let args_str = &expr[angle_idx + 1..expr.len() - 1];
            if self.jsdoc_generic_base_suppresses_full_name_error(base_name) {
                for arg in Self::split_type_args_respecting_nesting(args_str) {
                    self.validate_jsdoc_typedef_body_expr(
                        arg.trim(),
                        template_names,
                        comment_pos,
                        comment_end,
                        source_text,
                    );
                }
                return;
            }
            // Base name does not resolve. tsc reports the missing
            // identifier at the base, not the whole generic application.
            if !template_names.iter().any(|t| t == base_name)
                && !self.jsdoc_typedef_named_visible(base_name)
            {
                self.emit_jsdoc_cannot_find_name(base_name, comment_pos, comment_end, source_text);
            }
            // Still recurse into args — `Bogus<Missing>` should also
            // surface `Missing` if it would have, matching tsc's
            // multi-error JSDoc diagnostics.
            for arg in Self::split_type_args_respecting_nesting(args_str) {
                self.validate_jsdoc_typedef_body_expr(
                    arg.trim(),
                    template_names,
                    comment_pos,
                    comment_end,
                    source_text,
                );
            }
            return;
        }

        if self.resolve_jsdoc_type_str(expr).is_some() {
            return;
        }
        self.emit_jsdoc_cannot_find_name(expr, comment_pos, comment_end, source_text);
    }

    /// Whether a JSDoc `@typedef Name` matching `name` is visible from
    /// the current file's resolution context. Used to skip TS2304 for
    /// forward/recursive references that resolve correctly once all
    /// JSDoc is processed.
    fn jsdoc_typedef_named_visible(&self, name: &str) -> bool {
        if let Some(arenas) = self.ctx.all_arenas.as_ref() {
            for (file_idx, arena) in arenas.iter().enumerate() {
                if file_idx != self.ctx.current_file_idx
                    && !self.jsdoc_file_is_global_script(file_idx)
                {
                    continue;
                }
                for sf in arena.source_files.iter() {
                    if Self::source_file_has_jsdoc_typedef_named(sf, name) {
                        return true;
                    }
                }
            }
        }
        for sf in self.ctx.arena.source_files.iter() {
            if Self::source_file_has_jsdoc_typedef_named(sf, name) {
                return true;
            }
        }
        false
    }

    /// Check whether a JSDoc type expression is a simple identifier name
    /// (possibly with dots and angle brackets for generics).
    /// Returns false for complex expressions like function types, object literals, unions.
    fn is_simple_type_name(expr: &str) -> bool {
        if expr.is_empty() {
            return false;
        }
        let first = expr.chars().next().unwrap_or('\0');
        if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
            return false;
        }
        let mut angle_depth = 0u32;
        for ch in expr.chars() {
            match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$' | '.' => {}
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                ',' | ' ' if angle_depth > 0 => {}
                _ => return false,
            }
        }
        true
    }

    fn jsdoc_generic_base_suppresses_full_name_error(&mut self, base_name: &str) -> bool {
        if !Self::is_simple_type_name(base_name) {
            return false;
        }
        if self.resolve_jsdoc_type_str(base_name).is_some() {
            return true;
        }
        self.jsdoc_generic_base_is_known_function_value(base_name)
    }

    fn jsdoc_generic_base_is_known_function_value(&self, base_name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(base_name)
            && self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::FUNCTION))
        {
            return true;
        }

        self.resolve_identifier_symbol_from_all_binders(base_name, |_, symbol| {
            symbol.has_any_flags(symbol_flags::FUNCTION)
        })
        .is_some()
    }
}
