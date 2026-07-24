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
}

#[cfg(test)]
#[path = "tests/params_qualified_tests.rs"]
mod tests;
