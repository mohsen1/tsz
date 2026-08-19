//! Qualified (dotted) `@param` tag names and tsc's nested-`@param` model.
//!
//! This module owns:
//! - the nested-`@param` absorption model tsc implements in its parser
//! - TS8032 `Qualified name '{0}' is not allowed without a leading '@param {object} {1}'`
//!
//! See also:
//! - `params` — TS8024 `@param` name checking and tag syntax validation
//! - `params_type_strings` — `@param {type}` text extraction and nested types

use super::types::JsdocParamTagInfo;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// TS8032: a qualified `@param` name is only legal when an enclosing
    /// `@param {object}` tag declares its parent.
    pub(super) fn check_jsdoc_qualified_param_tags(
        &mut self,
        jsdoc: &str,
        actual_params: &[(String, bool)],
        func_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // tsc skips the whole unmatched-parameter branch when the body reads
        // `arguments`; the tags are matched against the implicit arguments
        // object instead.
        if self.function_uses_implicit_arguments_object(func_idx) {
            return;
        }

        let unattached = Self::jsdoc_unattached_qualified_param_tags(jsdoc);
        if unattached.is_empty() {
            return;
        }

        let source_info = self
            .get_jsdoc_comment_pos_for_function(func_idx)
            .and_then(|pos| {
                let sf = self.ctx.arena.source_files.first()?;
                Some((pos, sf.text.clone()))
            });

        for (index, name, source_name) in unattached {
            let Some((left, _)) = name.rsplit_once('.') else {
                continue;
            };
            // Destructured parameters accept any tag name at their position.
            if actual_params
                .get(index)
                .is_some_and(|(_, is_pattern)| *is_pattern)
            {
                continue;
            }
            let (name, source_name) = (name.as_str(), source_name.as_str());
            let message = format_message(
                diagnostic_messages::QUALIFIED_NAME_IS_NOT_ALLOWED_WITHOUT_A_LEADING_PARAM_OBJECT,
                &[name, left],
            );
            let code =
                diagnostic_codes::QUALIFIED_NAME_IS_NOT_ALLOWED_WITHOUT_A_LEADING_PARAM_OBJECT;
            let start = source_info.as_ref().and_then(|(comment_pos, source_text)| {
                Self::jsdoc_param_name_source_offset(source_text, *comment_pos, source_name)
            });
            match start {
                Some(start) => {
                    self.ctx
                        .error(start, source_name.len() as u32, message, code);
                }
                None => self.error_at_node(func_idx, &message, code),
            }
        }
    }

    /// Replay tsc's nested-`@param` absorption and return the qualified tags it
    /// leaves unattached — the TS8032 sites.
    ///
    /// tsc models nesting in the parser: `parseNestedTypeLiteral` lets a
    /// `@param` tag whose type is `object`/`Object`/`object[]` absorb the
    /// immediately following tags whose qualified name is exactly
    /// `<parent>.<one segment>`. Absorbed tags become properties of the
    /// parent's synthesized type literal and never reach the
    /// unmatched-parameter check, so only tags that survive absorption are
    /// reportable. tsz has no `JSDoc` AST, so the absorption is replayed as an
    /// ordered scan over a stack of open object-typed parents, popped until one
    /// of them is the direct parent of the current tag.
    ///
    /// Each entry is `(tag index, entity name, name as written in source)`. The
    /// entity name has `[]` segments stripped, matching tsc's
    /// `parseJSDocEntityName`, which accepts `y[]` but discards the brackets —
    /// so `opts[].x` is the qualified name `opts.x` and nests under
    /// `@param {Object[]} opts`. The source spelling is kept for anchoring.
    pub(crate) fn jsdoc_unattached_qualified_param_tags(
        jsdoc: &str,
    ) -> Vec<(usize, String, String)> {
        let tags = Self::extract_jsdoc_param_tag_entries(jsdoc);
        let names: Vec<String> = tags
            .iter()
            .map(|(tag, _)| tag.name.replace("[]", ""))
            .collect();

        let mut open_parents: Vec<&str> = Vec::new();
        let mut unattached = Vec::new();
        for (index, (tag, _)) in tags.iter().enumerate() {
            let name = names[index].as_str();
            let parent = name.rsplit_once('.').map(|(left, _)| left);
            while let Some(open) = open_parents.last() {
                if parent == Some(*open) {
                    break;
                }
                open_parents.pop();
            }
            // Plain identifiers are TS8024's business, not TS8032's.
            if open_parents.is_empty() && parent.is_some() {
                unattached.push((index, name.to_string(), tag.name.clone()));
            }
            if tag
                .type_expr
                .as_deref()
                .is_some_and(Self::jsdoc_type_is_object_or_object_array)
            {
                open_parents.push(name);
            }
        }
        unattached
    }

    /// Extract every `@param` tag from a `JSDoc` comment, keeping qualified
    /// (dotted) names.
    ///
    /// `extract_jsdoc_param_names` drops dotted names because its callers only
    /// care about tags that can name a real parameter. The nesting model needs
    /// the dotted tags too, in source order, together with the tag type
    /// expression, so this returns the parsed tag plus the byte offset of the
    /// `@param` tag within `jsdoc`.
    fn extract_jsdoc_param_tag_entries(jsdoc: &str) -> Vec<(JsdocParamTagInfo, usize)> {
        let mut result = Vec::new();
        let mut in_param = false;
        let mut param_text = String::new();
        let mut param_offset = 0usize;

        let flush = |text: &str, offset: usize, out: &mut Vec<(JsdocParamTagInfo, usize)>| {
            if let Some(tag) = Self::parse_jsdoc_param_tag(text) {
                out.push((tag, offset));
            }
        };

        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    flush(&param_text, param_offset, &mut result);
                    param_text.clear();
                }
                if let Some((param_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) {
                    in_param = true;
                    if let Some(line_start) = jsdoc.find(line)
                        && let Some(effective_pos) = line.find(effective)
                        && let Some(tag_pos) = Self::jsdoc_tag_offset(effective, param_tag)
                    {
                        param_offset = line_start + effective_pos + tag_pos;
                    }
                    param_text = rest.to_string();
                } else {
                    in_param = false;
                }
            } else if in_param {
                param_text.push(' ');
                param_text.push_str(trimmed);
            }
        }
        if in_param {
            flush(&param_text, param_offset, &mut result);
        }
        result
    }

    /// Mirror of tsc's `isObjectOrObjectArrayTypeReference`: the only tag types
    /// that can carry nested `@param` children.
    ///
    /// `object`, `Object` without type arguments, and any array of those.
    fn jsdoc_type_is_object_or_object_array(type_expr: &str) -> bool {
        let trimmed = type_expr.trim();
        if let Some(element) = trimmed.strip_suffix("[]") {
            return Self::jsdoc_type_is_object_or_object_array(element);
        }
        matches!(trimmed, "object" | "Object")
    }

    /// Byte offset of a `@param` tag's name within the source text, searching
    /// from the start of the `JSDoc` comment at `comment_pos`.
    fn jsdoc_param_name_source_offset(
        source_text: &str,
        comment_pos: u32,
        name: &str,
    ) -> Option<u32> {
        let comment_start = comment_pos as usize;
        let region = source_text.get(comment_start..)?;
        let mut search_from = 0usize;
        while let Some((at_param, param_tag)) =
            Self::jsdoc_param_tag_offset(region.get(search_from..)?)
        {
            let after_param = search_from + at_param + Self::jsdoc_tag_source_len(param_tag);
            if let Some(offset) = Self::find_param_name_in_source(region.get(after_param..)?, name)
            {
                return u32::try_from(comment_start + after_param + offset).ok();
            }
            search_from = after_param;
        }
        None
    }

    /// TS2694 for a qualified JSDoc type name `root.a.b(.c…)` whose root is a
    /// namespace-import alias, namespace, or module and one of whose
    /// **non-terminal** qualifier segments resolves to a value-only export
    /// member (or does not resolve as a type-space entity at all).
    ///
    /// Every qualifier segment of a qualified type name must have
    /// namespace/type meaning: for `import * as s from './mod'` where
    /// `exports.n = {}` makes `n` a plain value, the type reference `s.n.K`
    /// cannot qualify `.K` off the value `n`, so tsc reports TS2694
    /// "Namespace '\"mod\"' has no exported member 'n'." anchored at the failing
    /// segment (`n`). This is distinct from
    /// `validate_jsdoc_param_namespace_member_errors`, which only covers a
    /// *missing* member off a namespace root recognized by
    /// `is_jsdoc_namespace_root` (which excludes namespace-import aliases). A
    /// terminal segment used with type meaning (`s.Classic`, a class) stays
    /// clean here — only intermediate qualifiers are checked. Returns true when
    /// it emitted.
    pub(crate) fn report_jsdoc_param_qualified_value_only_qualifier(
        &mut self,
        type_expr: &str,
        comment_start: u32,
        type_expr_offset: usize,
    ) -> bool {
        let trimmed = type_expr.trim();
        if !Self::jsdoc_type_expr_is_plain_qualified_name(trimmed) {
            return false;
        }
        let segments: Vec<&str> = trimmed.split('.').collect();
        // Need a root plus at least one non-terminal qualifier and a terminal
        // segment (`root.a.b`). A two-segment `root.a` has no intermediate
        // qualifier to reject.
        if segments.len() < 3 {
            return false;
        }
        let root = segments[0];
        if !self.jsdoc_qualified_root_is_namespace_or_alias(root) {
            return false;
        }
        // A qualified `@typedef` declares a real dotted type reachable by name
        // even through a value-ish root; it is never the "value used as a
        // qualifier" case.
        if self.resolve_global_jsdoc_typedef_info(trimmed).is_some() {
            return false;
        }

        // Walk the non-terminal qualifier segments left to right. `byte_offset`
        // tracks the position of each segment inside `trimmed` for anchoring.
        let mut byte_offset = root.len();
        for i in 1..segments.len() - 1 {
            byte_offset += 1; // skip the '.' preceding this segment
            let seg = segments[i];
            let seg_start = byte_offset;
            byte_offset += seg.len();

            let prefix = segments[..=i].join(".");
            let resolved = self.resolve_jsdoc_entity_name_symbol(&prefix);
            let is_valid_qualifier = resolved
                .is_some_and(|sym_id| self.jsdoc_symbol_is_type_qualifier_container(sym_id));
            if is_valid_qualifier {
                continue;
            }
            // A resolvable value-only member, or a segment that does not resolve
            // to any type-space entity, cannot host `.{next}` — report the
            // TS2694 anchored at that segment. Requiring a real namespace/alias
            // root above keeps this off shapes some other resolver would
            // legitimately accept. `resolved_qualifier` names the segments
            // resolved before `seg` (empty when `seg` is the first member).
            let resolved_qualifier: Vec<&str> = segments[1..i].to_vec();
            let namespace_display =
                self.jsdoc_qualified_root_namespace_display(root, &resolved_qualifier);
            let message =
                format!("Namespace '{namespace_display}' has no exported member '{seg}'.");
            let start = self
                .ctx
                .arena
                .source_files
                .first()
                .and_then(|source_file| {
                    let source_text = source_file.text.as_ref();
                    source_text
                        .find(&format!("@param {{{trimmed}}}"))
                        .map(|offset| offset + "@param {".len() + seg_start)
                })
                .map(|offset| offset as u32)
                .unwrap_or(comment_start + type_expr_offset as u32 + seg_start as u32);
            let length = seg.len() as u32;
            let already_reported = self.ctx.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == 2694
                    && diagnostic.start == start
                    && diagnostic.length == length
                    && diagnostic.message_text == message
            });
            if !already_reported {
                self.error_at_position(start, length, &message, 2694);
            }
            return true;
        }
        false
    }

    /// Whether a symbol resolved for a qualified JSDoc type segment can host a
    /// further `.member` in type position — a namespace/module, enum, class,
    /// interface, or type alias. A plain value member (property, variable,
    /// object literal) cannot.
    fn jsdoc_symbol_is_type_qualifier_container(&self, sym_id: tsz_binder::SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        let container_flags = symbol_flags::NAMESPACE_MODULE
            | symbol_flags::VALUE_MODULE
            | symbol_flags::ENUM
            | symbol_flags::CLASS
            | symbol_flags::INTERFACE
            | symbol_flags::TYPE_ALIAS;
        self.get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(container_flags))
    }

    /// Display name of the namespace a qualified JSDoc type name roots at, for
    /// the TS2694 message. An import alias (`import * as s from './mod'`) is
    /// named by its resolved module (`"mod"`), matching tsc and the
    /// `import(...)`/`typeof import(...)` JSDoc paths; a plain in-file namespace
    /// is named by its own dotted path.
    fn jsdoc_qualified_root_namespace_display(
        &mut self,
        root: &str,
        resolved_qualifier: &[&str],
    ) -> String {
        if let Some(module_specifier) = self
            .ctx
            .binder
            .file_locals
            .get(root)
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .and_then(|symbol| symbol.import_module().map(str::to_string))
        {
            let module_path = self
                .resolved_import_type_module_path(&module_specifier, None)
                .unwrap_or_else(|| self.imported_namespace_display_module_name(&module_specifier));
            return self.jsdoc_import_namespace_display(
                &module_specifier,
                &module_path,
                resolved_qualifier,
            );
        }
        if resolved_qualifier.is_empty() {
            root.to_string()
        } else {
            format!("{root}.{}", resolved_qualifier.join("."))
        }
    }
}

#[cfg(test)]
#[path = "tests/params_qualified_tests.rs"]
mod tests;
