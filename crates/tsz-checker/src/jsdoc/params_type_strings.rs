//! JSDoc param type string extraction, nested params, and `@type` tag analysis.
//!
//! This module owns:
//! - `@param {type}` text-level extraction
//! - Required vs. optional param tag detection
//! - Nested `@param` object type construction
//! - `@type` tag callable detection and broad function checks
//! - `JsdocParamTagInfo` assembly helpers

use super::types::JsdocParamTagInfo;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Check if a `JSDoc` comment has a `@param {type}` annotation for the given parameter name.
    ///
    /// Returns true if the `JSDoc` contains `@param {someType} paramName`.
    pub(crate) fn jsdoc_has_param_type(jsdoc: &str, param_name: &str) -> bool {
        Self::extract_jsdoc_param_type_string(jsdoc, param_name).is_some()
    }

    /// Returns true if the JSDoc contains a `@param` tag for `param_name` that
    /// makes the parameter required (not optional).
    ///
    /// JSDoc optional param syntax (returns false for these):
    /// - `@param {Type=} name` — optional type suffix
    /// - `@param {Type} [name]` — brackets around name
    /// - `@param {Type} [name=default]` — brackets with default
    ///
    /// Non-optional `@param` tags (returns true):
    /// - `@param name` — name-only, no type
    /// - `@param {Type} name` — standard typed param
    pub(crate) fn jsdoc_has_required_param_tag(jsdoc: &str, param_name: &str) -> bool {
        for chunk in jsdoc.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches('\n').trim();

            let effective = Self::skip_backtick_quoted(trimmed);

            if let Some((_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective)
                && let Some(param) = Self::parse_jsdoc_param_tag(rest)
                && param.name == param_name
                && !param.optional
            {
                return true;
            }
        }
        false
    }

    /// Extract the type expression string from a `@param {type} name` JSDoc tag.
    ///
    /// Returns the type expression (e.g., "Object.<string, boolean>") for the given
    /// parameter name, or None if no matching `@param` tag is found.
    pub(crate) fn extract_jsdoc_param_type_string(jsdoc: &str, param_name: &str) -> Option<String> {
        let mut in_param = false;
        let mut param_text = String::new();
        for chunk in jsdoc.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches('\n').trim();

            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some((type_expr, _)) =
                        Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
                    {
                        return Some(type_expr);
                    }
                    param_text.clear();
                }
                if let Some((_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) {
                    in_param = true;
                    param_text = rest.to_string();
                } else {
                    in_param = false;
                }
            } else if in_param {
                // Continuation line for multi-line @param
                param_text.push(' ');
                param_text.push_str(trimmed);
            }
        }

        if in_param
            && let Some((type_expr, _)) =
                Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
        {
            return Some(type_expr);
        }

        None
    }

    /// Resolve the type from a JSDoc `@param {Type} name` annotation for a specific parameter.
    ///
    /// Extracts the type expression string from the `@param` tag matching `param_name`,
    /// then resolves it to a `TypeId` using the JSDoc type expression parser.
    ///
    /// Handles JSDoc optional parameter syntax:
    pub(crate) fn resolve_jsdoc_param_type_with_pos(
        &mut self,
        jsdoc: &str,
        param_name: &str,
        jsdoc_comment_start: Option<u32>,
    ) -> Option<tsz_solver::TypeId> {
        let (type_expr, type_expr_offset) =
            Self::extract_jsdoc_param_type_expr_with_span(jsdoc, param_name)?;
        // Handle {Type=} suffix which means optional (Type | undefined)
        let is_optional_type = type_expr.ends_with('=');
        let effective_type_expr = if is_optional_type {
            let mut expr = type_expr;
            expr.pop();
            expr
        } else {
            type_expr
        };
        // Handle {...Type} rest parameter prefix
        let is_rest = effective_type_expr.starts_with("...");
        let effective_type_expr = if is_rest {
            effective_type_expr[3..].to_string()
        } else {
            effective_type_expr
        };
        if let Some(comment_start) = jsdoc_comment_start {
            self.validate_jsdoc_param_namespace_member_errors(
                &effective_type_expr,
                comment_start,
                type_expr_offset,
            );
        }

        // Empty generic type parameter list inside the braces, e.g.
        // `@param {<} x`. tsc reports TS1098 at the `<` and TS1139 at the
        // `}`, and typechecks as if the type were unknown. Position
        // conversion uses `+ 4` to account for `/** ` (3 + leading space),
        // matching the convention used below for TS2314 emission.
        if effective_type_expr.trim() == "<"
            && let Some(comment_start) = jsdoc_comment_start
        {
            let lt_pos = comment_start + type_expr_offset as u32 + 4;
            let close_brace_pos = lt_pos + 1;
            self.error_at_position(
                lt_pos,
                1,
                crate::diagnostics::diagnostic_messages::TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY,
                crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY,
            );
            self.error_at_position(
                close_brace_pos,
                1,
                crate::diagnostics::diagnostic_messages::TYPE_PARAMETER_DECLARATION_EXPECTED,
                crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_DECLARATION_EXPECTED,
            );
            return Some(tsz_solver::TypeId::ERROR);
        }

        // Generic JSDoc type references like {C} should emit TS2314 when C
        // requires type arguments and none were provided.
        let base_type_expr = effective_type_expr.as_str();
        if let Some(comment_start) = jsdoc_comment_start
            && let Some((display_name, required_count)) =
                self.required_generic_count_for_jsdoc_type_name(base_type_expr)
        {
            let diag_start = comment_start + type_expr_offset as u32 + 4;
            self.error_generic_type_requires_type_arguments_at_span(
                &display_name,
                required_count,
                diag_start,
                base_type_expr.len() as u32,
            );
            return Some(tsz_solver::TypeId::ERROR);
        }

        let mut base_type = if let Some((module_specifier, segments)) =
            Self::parse_jsdoc_typeof_import_query(&effective_type_expr)
        {
            match self.resolve_jsdoc_typeof_import_reference_parts(&module_specifier, &segments) {
                Ok(resolved) => resolved,
                Err((member_offset, member_name)) => {
                    if let Some(comment_start) = jsdoc_comment_start {
                        let display_name =
                            self.imported_namespace_display_module_name(&module_specifier);
                        let resolved_qualifier = segments
                            .iter()
                            .filter_map(|(offset, segment)| {
                                (*offset < member_offset).then_some(segment.as_str())
                            })
                            .collect::<Vec<_>>();
                        let namespace_qualifier = if resolved_qualifier.is_empty() {
                            format!("\"{display_name}\"")
                        } else {
                            format!("\"{display_name}\".{}", resolved_qualifier.join("."))
                        };
                        let anchored_member_offset = effective_type_expr
                            .rfind(&format!(".{member_name}"))
                            .map(|offset| offset + 1)
                            .unwrap_or(member_offset);
                        let message = format!(
                            "Namespace '{namespace_qualifier}.export=' has no exported member '{member_name}'."
                        );
                        let source_start = self
                            .ctx
                            .arena
                            .source_files
                            .first()
                            .and_then(|source_file| {
                                let source_text = source_file.text.as_ref();
                                let exact =
                                    format!("@param {{{effective_type_expr}}} {param_name}");
                                let optional =
                                    format!("@param {{{effective_type_expr}}} [{param_name}]");
                                source_text
                                    .find(&exact)
                                    .or_else(|| source_text.find(&optional))
                            })
                            .map(|offset| offset + "@param {".len() + anchored_member_offset);
                        let start = source_start.map(|offset| offset as u32).unwrap_or(
                            comment_start + type_expr_offset as u32 + anchored_member_offset as u32,
                        );
                        let length = member_name.len() as u32;
                        let already_reported = self.ctx.diagnostics.iter().any(|diagnostic| {
                            diagnostic.code == 2694
                                && diagnostic.start == start
                                && diagnostic.length == length
                                && diagnostic.message_text == message
                        });
                        if !already_reported {
                            self.error_at_position(start, length, &message, 2694);
                        }
                    }
                    tsz_solver::TypeId::ANY
                }
            }
        } else {
            if let Some(comment_start) = jsdoc_comment_start {
                // Keep JSDoc @param generic-instantiation diagnostics anchored to the
                // same source offsets as conformance baselines.
                let type_expr_start = comment_start + type_expr_offset as u32 + 7;
                if self.report_jsdoc_param_generic_instantiation_errors(
                    &effective_type_expr,
                    type_expr_start,
                ) {
                    return Some(tsz_solver::TypeId::ERROR);
                }
            }
            self.resolve_jsdoc_type_str(&effective_type_expr)?
        };

        // Handle JSDoc destructured parameter type literals.
        // When the base type is Object/object (possibly with []), nested @param tags
        // like `@param {string} opts.x` define the actual object shape.
        let trimmed_expr = effective_type_expr.trim();
        let is_object_base = trimmed_expr == "Object" || trimmed_expr == "object";
        let is_array_object_base = trimmed_expr == "Object[]"
            || trimmed_expr == "object[]"
            || trimmed_expr == "Array.<Object>"
            || trimmed_expr == "Array.<object>"
            || trimmed_expr == "Array<Object>"
            || trimmed_expr == "Array<object>";

        if (is_object_base || is_array_object_base)
            && let Some(built) =
                self.build_nested_param_object_type(jsdoc, param_name, is_array_object_base)
        {
            base_type = built;
        }

        // For rest params ({...Type}), wrap in array
        if is_rest {
            base_type = self.ctx.types.factory().array(base_type);
        }

        // Check if parameter is optional via bracket syntax [name] or [name=default]
        let is_optional_name = Self::is_jsdoc_param_optional_by_brackets(jsdoc, param_name);
        if (is_optional_type || is_optional_name)
            && self.ctx.strict_null_checks()
            && base_type != tsz_solver::TypeId::ANY
            && base_type != tsz_solver::TypeId::UNDEFINED
        {
            Some(
                self.ctx
                    .types
                    .factory()
                    .union2(base_type, tsz_solver::TypeId::UNDEFINED),
            )
        } else {
            Some(base_type)
        }
    }

    /// Check if a JSDoc `@param` tag has a rest type prefix (`{...Type}`).
    pub(crate) fn jsdoc_param_is_rest(jsdoc: &str, param_name: &str) -> bool {
        Self::extract_jsdoc_param_type_expr_with_span(jsdoc, param_name)
            .is_some_and(|(expr, _)| expr.starts_with("..."))
    }

    pub(crate) fn validate_jsdoc_param_namespace_member_errors(
        &mut self,
        type_expr: &str,
        comment_start: u32,
        type_expr_offset: usize,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let bytes = type_expr.as_bytes();
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            if !Self::is_jsdoc_identifier_start(bytes[cursor]) {
                cursor += 1;
                continue;
            }
            let root_start = cursor;
            cursor += 1;
            while cursor < bytes.len() && Self::is_jsdoc_identifier_part(bytes[cursor]) {
                cursor += 1;
            }
            let root_end = cursor;
            if bytes.get(cursor) != Some(&b'.')
                || !bytes
                    .get(cursor + 1)
                    .is_some_and(|b| Self::is_jsdoc_identifier_start(*b))
            {
                continue;
            }
            let member_start = cursor + 1;
            cursor = member_start + 1;
            while cursor < bytes.len() && Self::is_jsdoc_identifier_part(bytes[cursor]) {
                cursor += 1;
            }
            let member_end = cursor;
            let root = &type_expr[root_start..root_end];
            let member = &type_expr[member_start..member_end];

            if !self.is_jsdoc_namespace_root(root) {
                continue;
            }
            if self
                .resolve_namespace_member_from_all_binders(root, member)
                .is_some()
                || self.ctx.binder.file_locals.get(root).is_some_and(|sym_id| {
                    self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        symbol
                            .exports
                            .as_ref()
                            .is_some_and(|exports| exports.get(member).is_some())
                            || symbol
                                .members
                                .as_ref()
                                .is_some_and(|members| members.get(member).is_some())
                    })
                })
            {
                continue;
            }

            let message = format_message(
                diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
                &[root, member],
            );
            let start = self
                .ctx
                .arena
                .source_files
                .first()
                .and_then(|source_file| {
                    let source_text = source_file.text.as_ref();
                    source_text
                        .find(&format!("@param {{{type_expr}}}"))
                        .map(|offset| offset + "@param {".len() + member_start)
                })
                .map(|offset| offset as u32)
                .unwrap_or(comment_start + type_expr_offset as u32 + member_start as u32);
            let length = member.len() as u32;
            let already_reported = self.ctx.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER
                    && diagnostic.start == start
                    && diagnostic.length == length
                    && diagnostic.message_text == message
            });
            if !already_reported {
                self.error_at_position(
                    start,
                    length,
                    &message,
                    diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
                );
            }
            return;
        }
    }

    fn is_jsdoc_namespace_root(&self, root: &str) -> bool {
        use tsz_binder::symbol_flags;
        if let Some(sym_id) = self.ctx.binder.file_locals.get(root)
            && self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::MODULE)
            })
        {
            return true;
        }
        self.resolve_identifier_symbol_from_all_binders(root, |_, symbol| {
            symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::MODULE)
        })
        .is_some()
    }

    /// Whether the root segment of a qualified JSDoc type expression refers
    /// to a (possibly aliased) namespace, module, or import alias visible in
    /// the current file. Used by the JSDoc typedef-base-type diagnostic loop
    /// to suppress the generic "Cannot find name" emitter for qualified names
    /// whose validity is owned by namespace-member resolution rather than by
    /// simple-identifier name lookup.
    ///
    /// `import * as s from './m'` binds `s` as an ALIAS symbol with
    /// `import_module = Some("./m")` but no `NAMESPACE_MODULE` flag on the
    /// alias itself, so `is_jsdoc_namespace_root` returns false. References
    /// like `@param {s.X}` are namespace-member accesses; emitting TS2304
    /// "Cannot find name 's.X'" for them conflicts with tsc, which either
    /// accepts them silently (when `X` is a valid export) or emits the
    /// namespace-member-specific TS2694 ("Namespace 's' has no exported
    /// member 'X'"). Both outcomes are owned by namespace-member resolution
    /// — not by the generic identifier-not-found emitter.
    pub(crate) fn jsdoc_qualified_root_is_namespace_or_alias(&self, root_name: &str) -> bool {
        use tsz_binder::symbol_flags;
        if self.is_jsdoc_namespace_root(root_name) {
            return true;
        }
        let Some(sym_id) = self.ctx.binder.file_locals.get(root_name) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.has_any_flags(
            symbol_flags::ALIAS
                | symbol_flags::NAMESPACE_MODULE
                | symbol_flags::VALUE_MODULE
                | symbol_flags::MODULE_EXPORTS,
        ) || symbol.import_module.is_some()
    }

    const fn is_jsdoc_identifier_start(byte: u8) -> bool {
        byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic()
    }

    const fn is_jsdoc_identifier_part(byte: u8) -> bool {
        Self::is_jsdoc_identifier_start(byte) || byte.is_ascii_digit()
    }

    fn required_generic_count_for_jsdoc_type_name(
        &mut self,
        type_expr: &str,
    ) -> Option<(String, usize)> {
        use tsz_binder::symbol_flags;

        if !Self::is_plain_jsdoc_type_name(type_expr) {
            return None;
        }
        if self
            .resolve_jsdoc_implicit_any_builtin_type(type_expr)
            .is_some()
        {
            return None;
        }

        let sym_id = self.ctx.binder.file_locals.get(type_expr)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags
            & (symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM)
            == 0
        {
            return None;
        }

        let type_params = self.get_type_params_for_symbol(sym_id);
        let required_count = type_params.iter().filter(|p| p.default.is_none()).count();
        if required_count == 0 {
            return None;
        }

        Some((
            Self::format_generic_display_name_with_interner(
                type_expr,
                &type_params,
                self.ctx.types,
            ),
            required_count,
        ))
    }

    pub(crate) fn is_plain_jsdoc_type_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '$' || first == '_' || first.is_ascii_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '$' || ch == '_' || ch.is_ascii_alphanumeric())
    }

    /// Extract nested `@param` properties for a destructured parameter.
    ///
    /// Given a parent parameter name like `opts`, extracts entries like:
    /// - `@param {string} opts.x` → ("x", "string", false)
    /// - `@param {string=} opts.y` → ("y", "string", true)  (= suffix)
    /// - `@param {string} [opts.z]` → ("z", "string", true)  (bracket syntax)
    /// - `@param {string} [opts.w="hi"]` → ("w", "string", true) (bracket + default)
    ///
    /// Build an object type from nested `@param` properties, handling arbitrary nesting depth.
    ///
    /// For `@param {object} opts` with nested `@param {string} opts.x` and
    /// `@param {object} opts.nested` with `@param {number} opts.nested.y`,
    /// this builds `{ x: string; nested: { y: number } }`.
    ///
    /// When `is_array` is true, wraps the result in an array type.
    fn build_nested_param_object_type(
        &mut self,
        jsdoc: &str,
        parent_name: &str,
        is_array: bool,
    ) -> Option<tsz_solver::TypeId> {
        let entries = Self::collect_jsdoc_nested_param_entries(jsdoc);
        self.build_nested_param_object_type_from_entries(&entries, parent_name, is_array)
    }

    pub(crate) fn build_nested_param_object_type_from_entries(
        &mut self,
        entries: &[(String, String, bool)],
        parent_name: &str,
        is_array: bool,
    ) -> Option<tsz_solver::TypeId> {
        let nested = Self::extract_jsdoc_nested_param_properties_from_entries(entries, parent_name);
        if nested.is_empty() {
            return None;
        }
        let mut properties = Vec::new();
        for (prop_name, prop_type_expr, is_prop_optional) in &nested {
            let (eff_type, opt_from_type) = if prop_type_expr.ends_with('=') {
                (&prop_type_expr[..prop_type_expr.len() - 1], true)
            } else {
                (prop_type_expr.as_str(), false)
            };

            // Check if this property itself is an object/Object with sub-properties
            let eff_trimmed = eff_type.trim();
            let is_sub_object = eff_trimmed == "Object" || eff_trimmed == "object";
            let is_sub_array_object = eff_trimmed == "Object[]"
                || eff_trimmed == "object[]"
                || eff_trimmed == "Array.<Object>"
                || eff_trimmed == "Array.<object>"
                || eff_trimmed == "Array<Object>"
                || eff_trimmed == "Array<object>";

            let prop_type_id = if is_sub_object || is_sub_array_object {
                // Build the full dotted parent name for recursive lookup
                let sub_parent = if is_array {
                    format!("{parent_name}[].{prop_name}")
                } else {
                    format!("{parent_name}.{prop_name}")
                };
                // Recursively build the nested object type
                self.build_nested_param_object_type_from_entries(
                    entries,
                    &sub_parent,
                    is_sub_array_object,
                )
                .or_else(|| self.jsdoc_type_from_expression(eff_type))
            } else {
                self.jsdoc_type_from_expression(eff_type)
            };

            if let Some(mut prop_type_id) = prop_type_id {
                let is_optional = *is_prop_optional || opt_from_type;
                if is_optional
                    && self.ctx.strict_null_checks()
                    && prop_type_id != tsz_solver::TypeId::ANY
                    && prop_type_id != tsz_solver::TypeId::UNDEFINED
                {
                    prop_type_id = self
                        .ctx
                        .types
                        .factory()
                        .union2(prop_type_id, tsz_solver::TypeId::UNDEFINED);
                }
                let name_atom = self.ctx.types.intern_string(prop_name);
                properties.push(tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: prop_type_id,
                    write_type: prop_type_id,
                    optional: is_optional,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: None,
                    declaration_order: (properties.len() + 1) as u32,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                });
            }
        }
        if properties.is_empty() {
            return None;
        }
        let obj_type = self.ctx.types.factory().object(properties);
        if is_array {
            Some(self.ctx.types.factory().array(obj_type))
        } else {
            Some(obj_type)
        }
    }

    /// - `@param {string} opts[].x` → ("x", "string", false) (array element property)
    ///
    /// Only extracts immediate child properties (one level of nesting).
    #[cfg(test)]
    pub(crate) fn extract_jsdoc_nested_param_properties(
        jsdoc: &str,
        parent_name: &str,
    ) -> Vec<(String, String, bool)> {
        let entries = Self::collect_jsdoc_nested_param_entries(jsdoc);
        Self::extract_jsdoc_nested_param_properties_from_entries(&entries, parent_name)
    }

    fn collect_jsdoc_nested_param_entries(jsdoc: &str) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();

        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            let Some((_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) else {
                continue;
            };
            let rest = rest.trim();

            // Parse {type} name pattern
            if !rest.starts_with('{') {
                continue;
            }
            let Some((type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest) else {
                continue;
            };
            let name_part = after_type.split_whitespace().next().unwrap_or("");

            // Check for bracket syntax [opts.x] or [opts.x=default]
            let (bare_name, is_bracket_optional) = if name_part.starts_with('[') {
                let inner = name_part.trim_start_matches('[');
                let bare = inner.split('=').next().unwrap_or(inner);
                let bare = bare.trim_end_matches(']');
                (bare, true)
            } else {
                (name_part, false)
            };

            if !bare_name.contains('.') && !bare_name.contains("[]") {
                continue;
            }

            result.push((
                bare_name.to_string(),
                type_expr.trim().to_string(),
                is_bracket_optional,
            ));
        }
        result
    }

    fn extract_jsdoc_nested_param_properties_from_entries(
        entries: &[(String, String, bool)],
        parent_name: &str,
    ) -> Vec<(String, String, bool)> {
        let mut result = Vec::new();
        let dot_prefix = format!("{parent_name}.");
        let array_dot_prefix = format!("{parent_name}[].");

        for (full_name, type_expr, is_bracket_optional) in entries {
            let prop_name = if let Some(prop) = full_name.strip_prefix(&dot_prefix) {
                if prop.contains('.') || prop.contains("[]") {
                    continue;
                }
                prop
            } else if let Some(prop) = full_name.strip_prefix(&array_dot_prefix) {
                if prop.contains('.') || prop.contains("[]") {
                    continue;
                }
                prop
            } else {
                continue;
            };

            if prop_name.is_empty() {
                continue;
            }

            result.push((
                prop_name.to_string(),
                type_expr.clone(),
                *is_bracket_optional,
            ));
        }

        result
    }

    /// Check if a JSDoc `@param` uses bracket syntax indicating optionality.
    ///
    /// Returns `true` for `@param {Type} [name]` or `@param {Type} [name=default]`.
    pub(crate) fn is_jsdoc_param_optional_by_brackets(jsdoc: &str, param_name: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);
            if let Some((_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) {
                let rest = rest.trim();
                // Check the name part after optional {type}
                let name_part_str = if rest.starts_with('{') {
                    // @param {type} [name] or @param {type} [name=default]
                    if let Some((_type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest)
                    {
                        after_type.split_whitespace().next().unwrap_or("")
                    } else {
                        continue;
                    }
                } else {
                    // @param [name] — no type, just bracket-optional name
                    rest.split_whitespace().next().unwrap_or("")
                };
                if name_part_str.starts_with('[') {
                    // Extract the bare name from [name] or [name=default]
                    let inner = name_part_str.trim_start_matches('[');
                    let bare = inner.split('=').next().unwrap_or(inner);
                    let bare = bare.trim_end_matches(']');
                    if bare == param_name {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Extract all `@param` tag names from a JSDoc comment.
    ///
    /// Returns a list of `(name, byte_offset)` pairs where `byte_offset` is the
    /// offset of the `@param` tag within the JSDoc text (used for error positioning).
    /// Handles `@param {type} name`, `@param name {type}`, and nested/dotted names
    /// like `opts.x` (only returns the top-level portion before the dot).
    pub(crate) fn extract_jsdoc_param_names(jsdoc: &str) -> Vec<(String, usize)> {
        let mut result = Vec::new();
        let mut in_param = false;
        let mut param_text = String::new();
        let mut param_offset = 0usize;

        for line in jsdoc.lines() {
            let trimmed = line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some(name) = Self::extract_param_name_from_tag(&param_text) {
                        result.push((name, param_offset));
                    }
                    param_text.clear();
                }
                if let Some((param_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) {
                    in_param = true;
                    // Calculate offset of this @param in the original JSDoc string
                    // Find this line in the original to get byte offset
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
        // Process the last @param if any
        if in_param && let Some(name) = Self::extract_param_name_from_tag(&param_text) {
            result.push((name, param_offset));
        }
        result
    }

    /// Extract the parameter name from a `@param` tag body (the text after `@param`).
    ///
    /// Handles:
    /// - `{type} name` → "name"
    /// - `{type} name description` → "name"
    /// - `{type} [name]` → "name"
    /// - `{type} [name=default]` → "name"
    /// - `{type} opts.x` → "opts" (nested/dotted → top-level only, skipped)
    /// - `{type} opts[].x` → "opts" (array dotted → skipped)
    /// - `name {type}` → "name"
    fn extract_param_name_from_tag(tag_body: &str) -> Option<String> {
        let parsed = Self::parse_jsdoc_param_tag(tag_body)?;
        if parsed.name.contains('.') || parsed.name.contains("[]") {
            return None;
        }
        let decoded = Self::decode_unicode_escapes(&parsed.name);
        if decoded.is_empty() {
            return Some(String::new()); // Empty name — still a @param tag
        }
        Some(decoded)
    }

    pub(crate) fn parse_jsdoc_param_tag(tag_body: &str) -> Option<JsdocParamTagInfo> {
        let rest = tag_body.trim();
        if rest.is_empty() {
            return None;
        }

        let (type_expr, name_token) = if rest.starts_with('{') {
            let (expr, after_type) = Self::parse_jsdoc_curly_type_expr(rest)?;
            if Self::jsdoc_param_type_syntax_error_offset(expr).is_some() {
                return None;
            }
            (
                Some(expr.trim().to_string()),
                after_type.split_whitespace().next().unwrap_or(""),
            )
        } else {
            let first = rest.split_whitespace().next().unwrap_or("");
            let inline_type = rest.find('{').and_then(|idx| {
                Self::parse_jsdoc_curly_type_expr(&rest[idx..]).and_then(|(expr, _)| {
                    Self::jsdoc_param_type_syntax_error_offset(expr)
                        .is_none()
                        .then(|| expr.trim().to_string())
                })
            });
            (inline_type, first)
        };

        let bracket_optional = name_token.starts_with('[');
        let mut name = name_token.trim_start_matches('[');
        name = name.split('=').next().unwrap_or(name);
        name = name.trim_end_matches(']');
        name = name.trim_matches('`');
        if name == "*" {
            if rest.starts_with('{') {
                return Some(JsdocParamTagInfo {
                    name: String::new(),
                    type_expr,
                    optional: false,
                    rest: false,
                });
            }
            return None;
        }
        let name = Self::decode_unicode_escapes(name.trim_start_matches("..."));
        if name.is_empty() {
            return None;
        }

        let type_optional = type_expr
            .as_deref()
            .is_some_and(|expr| expr.trim_end().ends_with('='));
        let rest = type_expr
            .as_deref()
            .is_some_and(|expr| expr.trim_start().starts_with("..."));

        Some(JsdocParamTagInfo {
            name,
            type_expr,
            optional: bracket_optional || type_optional,
            rest,
        })
    }

    pub(crate) fn jsdoc_param_type_syntax_error_offset(type_expr: &str) -> Option<usize> {
        let trimmed = type_expr.trim_start();
        let leading_ws = type_expr.len() - trimmed.len();
        let body = trimmed.strip_prefix("...").unwrap_or(trimmed);
        let body_offset = leading_ws + trimmed.len().saturating_sub(body.len());
        body.find("?[]")
            .or_else(|| body.ends_with("?!").then(|| body.len() - 2))
            .map(|offset| body_offset + offset)
    }

    /// Skip leading JSDoc decoration and backtick-quoted sections in a `JSDoc` line.
    ///
    /// Lines like `* @param {string} z` or `` `@param` @param {string} z ``
    /// contain comment decoration or backtick-quoted text before the real
    /// `@param` tag. This function strips those leading sections so the real
    /// tag can be detected.
    pub(crate) fn skip_backtick_quoted(s: &str) -> &str {
        let mut rest = s;
        loop {
            rest = rest.trim_start();
            if let Some(after_star) = rest.strip_prefix('*') {
                let is_jsdoc_decoration = after_star.is_empty()
                    || after_star
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_whitespace() || ch == '@');
                if is_jsdoc_decoration {
                    rest = after_star;
                    continue;
                }
            }
            if rest.starts_with('`') {
                // Find matching closing backtick
                if let Some(end) = rest[1..].find('`') {
                    rest = &rest[end + 2..];
                    continue;
                }
            }
            break;
        }
        rest
    }

    /// Strip `@<tag_name>` from the start of `text` if present and immediately
    /// followed by a JSDoc tag boundary. Use instead of `text.strip_prefix("@tag")`
    /// when the text may also start with longer `@tagx` identifiers — the
    /// boundary check rejects such longer names.
    pub(crate) fn strip_jsdoc_tag_prefix<'s>(text: &'s str, tag_name: &str) -> Option<&'s str> {
        let needle = format!("@{tag_name}");
        let rest = text.strip_prefix(&needle)?;
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return None;
        }
        Some(rest)
    }

    /// Like `extract_jsdoc_param_type_expr_from_param_tag`, but returns the matching type expression
    /// and its byte offset within a full JSDoc block.
    pub(crate) fn extract_jsdoc_param_type_expr_with_span(
        jsdoc: &str,
        param_name: &str,
    ) -> Option<(String, usize)> {
        let mut in_param = false;
        let mut param_text = String::new();
        let mut text_offset = 0usize;
        let mut line_start = 0usize;

        for chunk in jsdoc.split_inclusive('\n') {
            let raw_line = chunk.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = raw_line.trim();
            let effective = Self::skip_backtick_quoted(trimmed);

            if effective.starts_with('@') {
                if in_param {
                    if let Some((expr, local_offset)) =
                        Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
                    {
                        return Some((expr, text_offset + local_offset));
                    }
                    param_text.clear();
                }
                if let Some((param_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(effective) {
                    in_param = true;
                    param_text = rest.to_string();
                    let at_pos = raw_line.find(effective).unwrap_or(0)
                        + Self::jsdoc_tag_offset(effective, param_tag).unwrap_or(0);
                    text_offset = line_start + at_pos + Self::jsdoc_tag_source_len(param_tag);
                } else {
                    in_param = false;
                }
            } else if in_param {
                param_text.push(' ');
                param_text.push_str(trimmed);
            }

            line_start += chunk.len();
        }
        if in_param
            && let Some((expr, local_offset)) =
                Self::extract_jsdoc_param_type_expr_from_param_tag(&param_text, param_name)
        {
            return Some((expr, text_offset + local_offset));
        }
        None
    }

    /// Extract a @param type expression (inside {}) matching a parameter name,
    /// returning the expression and its byte offset within the JSDoc tag body.
    fn extract_jsdoc_param_type_expr_from_param_tag(
        text: &str,
        param_name: &str,
    ) -> Option<(String, usize)> {
        let rest = text.trim();
        if rest.is_empty() {
            return None;
        }
        let text_ptr = text.as_ptr() as usize;
        let rest_ptr = rest.as_ptr() as usize;
        let rest_offset = rest_ptr.saturating_sub(text_ptr);

        // Handle alternate syntax: @param `name` {type} or @param name {type}
        if !rest.starts_with('{') {
            let name_part = rest.split_whitespace().next().unwrap_or("");
            let name_part_stripped = name_part.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name_part_stripped);
            if decoded == param_name {
                let after_name = rest[name_part.len()..].trim();
                if let Some((type_expr, _)) = Self::parse_jsdoc_curly_type_expr(after_name) {
                    if Self::jsdoc_param_type_syntax_error_offset(type_expr).is_some() {
                        return None;
                    }
                    let type_expr = type_expr.trim();
                    let type_expr_start_offset = type_expr.len() - type_expr.trim_start().len();
                    let type_expr_ptr = type_expr.as_ptr() as usize;
                    let offset = if type_expr.is_empty() {
                        0
                    } else {
                        let raw_offset = type_expr_ptr.saturating_sub(rest_ptr);
                        raw_offset + type_expr_start_offset + rest_offset
                    };
                    return Some((type_expr.to_string(), offset));
                }
            }
            return None;
        }

        // Standard syntax: @param {type} name
        if let Some((type_expr, after_type)) = Self::parse_jsdoc_curly_type_expr(rest) {
            if Self::jsdoc_param_type_syntax_error_offset(type_expr).is_some() {
                return None;
            }
            let name = after_type.split_whitespace().next().unwrap_or("");
            let name = name.trim_start_matches('[');
            let name = name.split('=').next().unwrap_or(name);
            let name = name.trim_end_matches(']');
            let name = name.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name);
            if decoded == param_name {
                let type_expr = type_expr.trim();
                let type_expr_start_offset = type_expr.len() - type_expr.trim_start().len();
                let type_expr_ptr = type_expr.as_ptr() as usize;
                let offset = if type_expr.is_empty() {
                    0
                } else {
                    let raw_offset = type_expr_ptr.saturating_sub(rest_ptr);
                    raw_offset + type_expr_start_offset + rest_offset
                };
                return Some((type_expr.to_string(), offset));
            }
        }
        None
    }

    /// Decode unicode escapes (`\uXXXX` and `\u{XXXX}`) in a string.
    fn decode_unicode_escapes(s: &str) -> String {
        if !s.contains("\\u") {
            return s.to_string();
        }
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && chars.peek() == Some(&'u') {
                chars.next(); // consume 'u'
                if chars.peek() == Some(&'{') {
                    // \u{XXXX} form
                    chars.next(); // consume '{'
                    let mut hex = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        hex.push(c);
                        chars.next();
                    }
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u{");
                    result.push_str(&hex);
                    result.push('}');
                } else {
                    // \uXXXX form (exactly 4 hex digits)
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4
                        && let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u");
                    result.push_str(&hex);
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    /// Check if a `JSDoc` comment has any type annotations (`@param {type}`, `@returns {type}`,
    /// `@type {type}`, or `@template`).
    ///
    /// In tsc, when a function has `JSDoc` type annotations, implicit any errors (TS7010/TS7011)
    /// are suppressed even without explicit `@returns`, because the developer is providing
    /// type information through `JSDoc`.
    pub(crate) fn jsdoc_has_type_annotations(jsdoc: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            // @param {type} name
            if let Some((_tag, rest)) = Self::strip_jsdoc_param_tag_prefix(trimmed)
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @returns {type} or @return {type}
            if let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed)
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @type {type}
            if let Some(rest) = trimmed.strip_prefix("@type")
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @template T
            if Self::jsdoc_line_starts_with_tag(trimmed, "template") {
                return true;
            }
        }
        false
    }

    pub(crate) fn jsdoc_type_tag_declares_callable(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        if expr.eq_ignore_ascii_case("function") || expr.eq_ignore_ascii_case("Function") {
            return false;
        }
        expr.contains("=>")
            || expr
                .strip_prefix("function")
                .is_some_and(|rest| rest.trim_start().starts_with('('))
    }

    pub(crate) fn jsdoc_type_tag_is_broad_function(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        expr.eq_ignore_ascii_case("function") || expr.eq_ignore_ascii_case("Function")
    }

    pub(crate) fn jsdoc_type_tag_function_missing_return(jsdoc: &str) -> bool {
        let Some(expr) = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc) else {
            return false;
        };
        let expr = expr.trim();
        let Some(rest) = expr.strip_prefix("function") else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            return false;
        }
        let rest = &rest[1..];
        // Closure-style constructor types like `function(new: object, ...)` have
        // an implied return type (the type after `new:`).  They never need a
        // separate `:returnType` suffix, so they should not trigger TS7014.
        if rest.trim_start().starts_with("new:") || rest.trim_start().starts_with("new :") {
            return false;
        }
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close_idx) = close_idx else {
            return false;
        };
        !rest[close_idx + 1..].trim_start().starts_with(':')
    }

    pub(crate) fn jsdoc_type_tag_function_keyword_pos_in_source(
        source_text: &str,
        comment_pos: u32,
    ) -> Option<u32> {
        let comment_start = comment_pos as usize;
        let comment_text = &source_text[comment_start..];
        let comment_end = comment_text.find("*/")?;
        let comment_text = &comment_text[..comment_end];
        let tag_pos = comment_text.find("@type")?;
        let rest = &comment_text[tag_pos + "@type".len()..];
        let fn_rel = rest.find("function")?;
        Some(comment_pos + (tag_pos + "@type".len() + fn_rel) as u32)
    }

    /// Extract the return type string from `@type {function(): ReturnType}`.
    /// Returns `Some(return_type_str)` if the JSDoc `@type` is a function type
    /// with an explicit return type annotation.
    pub(crate) fn jsdoc_type_tag_function_return_type(jsdoc: &str) -> Option<String> {
        let expr = Self::jsdoc_extract_type_tag_expr_braceless(jsdoc)?;
        let expr = expr.trim();
        let rest = expr.strip_prefix("function")?;
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            return None;
        }
        let rest = &rest[1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_idx = close_idx?;
        let after_close = rest[close_idx + 1..].trim_start();
        let ret_str = after_close.strip_prefix(':')?;
        let ret_str = ret_str.trim();
        if ret_str.is_empty() {
            return None;
        }
        Some(ret_str.to_string())
    }

    /// Find the source position and length of the return type in
    /// `@type {function(): ReturnType}` within the comment starting at `comment_pos`.
    pub(crate) fn jsdoc_type_tag_function_return_type_span_in_source(
        source_text: &str,
        comment_pos: u32,
    ) -> Option<(u32, u32)> {
        let comment_start = comment_pos as usize;
        let comment_text = source_text.get(comment_start..)?;
        let comment_end = comment_text.find("*/")?;
        let comment_text = &comment_text[..comment_end];
        let tag_pos = comment_text.find("@type")?;
        let rest = &comment_text[tag_pos + "@type".len()..];
        let fn_rel = rest.find("function")?;
        let after_fn = &rest[fn_rel + "function".len()..];
        let paren_start = after_fn.find('(')?;
        let after_paren = &after_fn[paren_start + 1..];
        let mut depth = 1u32;
        let mut close_idx = None;
        for (i, ch) in after_paren.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_idx = close_idx?;
        let after_close = &after_paren[close_idx + 1..];
        let colon_rel = after_close.find(':')?;
        let ret_part = &after_close[colon_rel + 1..];
        let leading_ws = ret_part.len() - ret_part.trim_start().len();
        let ret_trimmed = ret_part.trim();
        let ret_end = ret_trimmed.find('}').unwrap_or(ret_trimmed.len());
        let ret_type_str = ret_trimmed[..ret_end].trim_end();
        let abs_start = comment_start
            + tag_pos
            + "@type".len()
            + fn_rel
            + "function".len()
            + paren_start
            + 1
            + close_idx
            + 1
            + colon_rel
            + 1
            + leading_ws;
        Some((abs_start as u32, ret_type_str.len() as u32))
    }

    pub(crate) fn jsdoc_extract_type_tag_expr_braceless(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            if let Some(rest) = trimmed.strip_prefix("@type") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    return Some(rest[1..1 + end].trim().to_string());
                }
                if !rest.is_empty() && !rest.starts_with('@') {
                    return Some(rest.to_string());
                }
            }
        }
        None
    }

    /// Extract the type expression from a `@type {X}` JSDoc tag.
    /// Returns the inner type expression string (e.g., "Cb" from `@type {Cb}`).
    pub(crate) fn jsdoc_extract_type_tag_expr(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            if let Some(rest) = trimmed.strip_prefix("@type") {
                let rest = rest.trim();
                if let Some(after_open) = rest.strip_prefix('{') {
                    // Balance nested braces so `{{ a: T }}` (object literal
                    // type wrapped in `@type {...}`) extracts the full
                    // `{ a: T }` body, not just `{ a: T`.
                    let mut depth = 1usize;
                    for (i, ch) in after_open.char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    return Some(after_open[..i].trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        None
    }

    pub(crate) fn jsdoc_type_tag_expr_span_for_node_direct(
        &self,
        idx: NodeIndex,
    ) -> Option<(u32, u32)> {
        let sf = self.source_file_data_for_node(idx)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let pos = self.effective_jsdoc_pos_for_node(idx, &comments, &source_text)?;
        let (jsdoc, comment_pos) = self.try_leading_jsdoc_with_pos(&comments, pos, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let comment = comments.iter().find(|comment| comment.pos == comment_pos)?;
        let raw_comment = comment.get_text(&source_text);
        let type_tag_offset = raw_comment.find("@type")?;
        let after_tag = &raw_comment[type_tag_offset + "@type".len()..];
        let open_brace_offset = after_tag.find('{')?;
        let after_open_brace = &after_tag[open_brace_offset + 1..];
        let trimmed = after_open_brace.trim_start();
        let leading_ws = after_open_brace.len().saturating_sub(trimmed.len());
        let expr_start = comment_pos
            + (type_tag_offset + "@type".len() + open_brace_offset + 1 + leading_ws) as u32;
        Some((expr_start, type_expr.len() as u32))
    }

    /// Check if a JSDoc type expression is syntactically a callable/function type.
    /// Returns true for arrow types (`(x: T) => R`), function types (`function(x): R`),
    /// and generic signatures (`<T>(x: T) => R`).
    pub(crate) fn is_syntactically_callable_type(type_expr: &str) -> bool {
        let trimmed = type_expr.trim();
        // Arrow function type: contains `=>`
        if trimmed.contains("=>") {
            return true;
        }
        // function(...): ... type
        if trimmed.starts_with("function") {
            return true;
        }
        // Generic signature: <T>(...) => ...
        if trimmed.starts_with('<') {
            return true;
        }
        // Parenthesized callable: (x: number) => void
        if trimmed.starts_with('(') {
            return true;
        }
        false
    }

    /// Extract a type predicate from a `@type {CallbackType}` JSDoc annotation.
    /// Resolves the referenced type and checks both Function and Callable shapes.
    pub(crate) fn extract_type_predicate_from_jsdoc_type_tag(
        &mut self,
        jsdoc: &str,
    ) -> Option<tsz_solver::TypePredicate> {
        let type_expr = Self::jsdoc_extract_type_tag_expr(jsdoc)?;
        let resolved = self.resolve_jsdoc_type_str(&type_expr)?;
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, resolved)
        {
            return shape.type_predicate;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, resolved)
            && let Some(sig) = sigs.first()
        {
            return sig.type_predicate;
        }
        None
    }

    /// Extract `@template` type parameter names from a `JSDoc` comment.
    ///
    /// Returns `(name, is_const, default_type_str)` triples. `is_const` is true when
    /// the `const` modifier precedes the type parameter name (e.g. `@template const T`).
    /// `default_type_str` is `Some(expr)` when the bracket-default form `[T=expr]` is
    /// used (e.g. `@template [T=string]` → `Some("string")`); otherwise `None`.
    ///
    /// Supports:
    /// - `@template T`
    /// - `@template T,U`
    /// - `@template const T`
    /// - `@template const T, U` (both T and U are const per tsc)
    /// - `@template [T=string]` (T with default `string`)
    pub(crate) fn jsdoc_template_type_params(jsdoc: &str) -> Vec<(String, bool, Option<String>)> {
        let mut out = Vec::new();
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_tag_prefix(trimmed, "template") else {
                continue;
            };
            // Track whether `const` modifier was seen on this @template line.
            // In tsc, `@template const T, U` makes ALL type params on
            // that line const.
            let mut saw_const = false;
            let mut segment_start = 0usize;
            let mut depth = 0usize;
            let mut push_segment = |segment: &str, saw_const: &mut bool| {
                let bytes = segment.as_bytes();
                let mut cursor = 0usize;
                while cursor < bytes.len() {
                    while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_whitespace() {
                        cursor += 1;
                    }
                    if cursor >= bytes.len() {
                        break;
                    }
                    if bytes[cursor] as char == '{' {
                        let mut brace_depth = 1usize;
                        cursor += 1;
                        while cursor < bytes.len() && brace_depth > 0 {
                            match bytes[cursor] as char {
                                '{' => brace_depth += 1,
                                '}' => brace_depth = brace_depth.saturating_sub(1),
                                _ => {}
                            }
                            cursor += 1;
                        }
                        continue;
                    }

                    // Bracket-default form: `@template [T=string]` declares
                    // type parameter `T` with default `string`. tsc accepts
                    // this form; without unwrapping the `[`, the identifier
                    // scan below sees `[` as a non-identifier byte and skips
                    // the segment entirely (issue #4005).
                    let in_bracket = bytes[cursor] as char == '[';
                    if in_bracket {
                        cursor += 1;
                        while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_whitespace()
                        {
                            cursor += 1;
                        }
                    }

                    let start = cursor;
                    while cursor < bytes.len() {
                        let ch = bytes[cursor] as char;
                        if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                            cursor += 1;
                        } else {
                            break;
                        }
                    }
                    if start == cursor {
                        break;
                    }

                    let name = &segment[start..cursor];
                    // Track `const` modifier keyword (e.g., `@template const T`).
                    // tsc treats `const` as a type parameter modifier, not a name.
                    if name == "const" {
                        *saw_const = true;
                        continue;
                    }
                    // Skip variance modifier keywords (e.g., `@template in T`,
                    // `@template out T`, `@template in out T`). tsc treats `in`
                    // and `out` as type-parameter modifiers, not names. Without
                    // this skip, downstream consumers see an extra unbound name
                    // like `in` and emit cascading TS2314/TS7006 false positives.
                    // (TS1274 — `'in' modifier can only appear on a type
                    // parameter of a class, interface or type alias` — is
                    // emitted by a separate validator and is not in scope here.)
                    if name == "in" || name == "out" {
                        continue;
                    }
                    // Extract default type string from bracket form `[T=default]`.
                    let default_str = if in_bracket {
                        // After the identifier, skip whitespace and look for `=`
                        let mut pos = cursor;
                        while pos < bytes.len() && (bytes[pos] as char).is_ascii_whitespace() {
                            pos += 1;
                        }
                        if pos < bytes.len() && bytes[pos] as char == '=' {
                            pos += 1; // skip '='
                            // Find `]` or end of segment
                            let default_start = pos;
                            while pos < bytes.len() && bytes[pos] as char != ']' {
                                pos += 1;
                            }
                            let raw = &segment[default_start..pos];
                            let trimmed_default = raw.trim();
                            if trimmed_default.is_empty() {
                                None
                            } else {
                                Some(trimmed_default.to_string())
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if !out.iter().any(|(existing, _, _)| existing == name) {
                        out.push((name.to_string(), *saw_const, default_str));
                    }
                    break;
                }
            };

            for (idx, ch) in rest.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        push_segment(&rest[segment_start..idx], &mut saw_const);
                        segment_start = idx + ch.len_utf8();
                    }
                    _ => {}
                }
            }
            push_segment(&rest[segment_start..], &mut saw_const);
        }
        out
    }

    /// Map of `@template` parameter name -> its constraint type-expression
    /// string for the `@template {Constraint} T` form.
    ///
    /// Only names that carry an explicit constraint appear in the map; a bare
    /// `@template T` contributes no entry. As in tsc, a single brace clause
    /// (`@template {C} A, B`) constrains only the first listed name.
    ///
    /// Callers resolve each string to a `TypeId` via `resolve_jsdoc_reference`
    /// at the point where the type parameter is lowered. As with `@template`
    /// defaults, a constraint that references a sibling `@template` parameter
    /// resolves only at sites that register each parameter in scope before
    /// resolving the next.
    pub(crate) fn jsdoc_template_constraint_strings(
        jsdoc: &str,
    ) -> std::collections::HashMap<String, String> {
        Self::jsdoc_template_constraints(jsdoc)
            .into_iter()
            .filter_map(|(name, constraint)| constraint.map(|c| (name, c)))
            .collect()
    }

    /// Emit JSDoc `@template` syntax diagnostics for invalid brace forms like
    /// `@template {T}`. tsc reports both TS1069 at `{` and TS2304 at `T`.
    pub(crate) fn validate_jsdoc_template_tag_syntax_at_decl(&mut self, decl_idx: NodeIndex) {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some((_, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return;
        };
        let comment_end = node.pos.min(source_text.len() as u32);
        let comment_range = &source_text[comment_pos as usize..comment_end as usize];

        let mut scan_start = 0usize;
        while let Some(template_offset) =
            Self::jsdoc_tag_offset(&comment_range[scan_start..], "template")
        {
            let template_start = scan_start + template_offset;
            let rest = &comment_range[template_start + "@template".len()..];
            let trimmed = rest.trim_start();
            if !trimmed.starts_with('{') {
                scan_start = template_start + "@template".len();
                continue;
            }

            let leading_ws = rest.len() - trimmed.len();
            let brace_rel = template_start + "@template".len() + leading_ws;
            let after_brace = &trimmed[1..];

            // tsc accepts `@template {Constraint} Name` as a type parameter
            // with a constraint (equivalent to `Name extends Constraint`).
            // Detect that form by finding the matching close-brace and
            // checking whether an identifier follows it (after whitespace).
            // If so, this is valid JSDoc syntax — skip.
            let balanced_close_brace_offset = |s: &str| -> Option<usize> {
                let mut depth: i32 = 1;
                for (i, ch) in s.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(i);
                            }
                        }
                        _ => {}
                    }
                }
                None
            };
            if let Some(close_rel) = balanced_close_brace_offset(after_brace) {
                let after_close = &after_brace[close_rel + 1..];
                let ws_len = after_close
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(|c| c.len_utf8())
                    .sum::<usize>();
                let ident_rest = &after_close[ws_len..];
                let has_ident = ident_rest
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c == '$' || c.is_ascii_alphabetic());
                if has_ident {
                    scan_start = brace_rel + 1 + close_rel + 1;
                    continue;
                }
            }

            let name_len = after_brace
                .chars()
                .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                .count();
            let error_rel = brace_rel
                + 1
                + name_len
                + usize::from(
                    after_brace
                        .get(name_len..)
                        .is_some_and(|rest| rest.starts_with('}')),
                );
            let brace_pos = comment_pos + error_rel as u32;
            self.ctx.error(
                brace_pos,
                1,
                crate::diagnostics::diagnostic_messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES.to_string(),
                crate::diagnostics::diagnostic_codes::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
            );

            if name_len > 0 {
                let name = &after_brace[..name_len];
                self.emit_jsdoc_cannot_find_name(name, comment_pos, comment_end, source_text);
            }

            scan_start = brace_rel + 1;
        }
    }

    /// Extract a simple identifier from `@returns {T}` / `@return {T}`.
    ///
    /// Returns `None` for complex type expressions.
    pub(crate) fn jsdoc_returns_type_name(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };
            if !type_expr.is_empty()
                && type_expr
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    /// Extract the raw type expression from `@returns {Type}` / `@return {Type}`.
    pub(crate) fn jsdoc_returns_type_expression(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };
            if !type_expr.is_empty() {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    pub(crate) fn jsdoc_type_expression_is_type_predicate(type_expr: &str) -> bool {
        let (is_asserts, remainder) = Self::split_jsdoc_asserts_prefix(type_expr);
        is_asserts || Self::find_jsdoc_type_predicate_is(remainder).is_some()
    }

    /// Extract a type predicate from `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Returns `Some((is_asserts, param_name, type_str))` if the `@returns` tag
    /// contains a type predicate pattern like `{x is string}` or `{this is Entry}`.
    /// Also handles `{asserts x is Type}` and `{asserts x}` patterns.
    pub(crate) fn jsdoc_returns_type_predicate(
        jsdoc: &str,
    ) -> Option<(bool, String, Option<String>)> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };

            let (is_asserts, remainder) = Self::split_jsdoc_asserts_prefix(type_expr);

            if let Some((is_pos, is_end)) = Self::find_jsdoc_type_predicate_is(remainder) {
                let param_name = remainder[..is_pos].trim();
                let type_str = remainder[is_end..].trim();
                // Validate param_name is a simple identifier or "this"
                if !param_name.is_empty()
                    && (param_name == "this"
                        || param_name
                            .chars()
                            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                    && !type_str.is_empty()
                {
                    return Some((
                        is_asserts,
                        param_name.to_string(),
                        Some(type_str.to_string()),
                    ));
                }
            } else if is_asserts {
                // "asserts x" without " is Type" — assertion without narrowing type
                let param_name = remainder;
                if !param_name.is_empty()
                    && (param_name == "this"
                        || param_name
                            .chars()
                            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                {
                    return Some((true, param_name.to_string(), None));
                }
            }
        }
        None
    }
}
