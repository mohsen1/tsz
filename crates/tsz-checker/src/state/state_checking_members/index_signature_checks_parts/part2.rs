impl<'a> CheckerState<'a> {
    fn has_package_root_side_effect_import(text: &str) -> bool {
        let bytes = text.as_bytes();
        let mut idx = 0usize;
        while idx < bytes.len() {
            idx = Self::skip_js_trivia(text, idx);
            if idx >= bytes.len() {
                break;
            }

            if !text[idx..].starts_with("import") || !Self::is_word_boundary(text, idx + 6) {
                idx += text[idx..].chars().next().map(char::len_utf8).unwrap_or(1);
                continue;
            }

            let mut cursor = Self::skip_js_trivia(text, idx + 6);
            let Some(quote) = text[cursor..].chars().next() else {
                break;
            };
            if quote != '"' && quote != '\'' {
                idx += 6;
                continue;
            }
            cursor += quote.len_utf8();
            let Some(rest) = text.get(cursor..) else {
                break;
            };
            if let Some(after_specifier) = rest
                .strip_prefix("./")
                .map(|_| cursor + 2)
                .or_else(|| rest.strip_prefix('.').map(|_| cursor + 1))
                && text[after_specifier..].starts_with(quote)
            {
                let after_quote = after_specifier + quote.len_utf8();
                let after_import = Self::skip_js_trivia(text, after_quote);
                if after_import >= text.len()
                    || text[after_import..].starts_with(';')
                    || text[after_import..].starts_with('\n')
                    || text[after_import..].starts_with('\r')
                {
                    return true;
                }
            }

            idx += 6;
        }

        false
    }

    fn skip_js_trivia(text: &str, mut idx: usize) -> usize {
        loop {
            while let Some(ch) = text[idx..].chars().next()
                && ch.is_whitespace()
            {
                idx += ch.len_utf8();
            }

            if text[idx..].starts_with("//") {
                idx += 2;
                while let Some(ch) = text[idx..].chars().next() {
                    idx += ch.len_utf8();
                    if ch == '\n' || ch == '\r' {
                        break;
                    }
                }
                continue;
            }

            if text[idx..].starts_with("/*") {
                idx += 2;
                if let Some(end) = text[idx..].find("*/") {
                    idx += end + 2;
                } else {
                    return text.len();
                }
                continue;
            }

            return idx;
        }
    }

    fn is_word_boundary(text: &str, idx: usize) -> bool {
        text[idx..]
            .chars()
            .next()
            .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
    }

    fn jsx_runtime_types_package_dir_matches(&self, package_dir: &str) -> bool {
        use tsz_common::checker_options::JsxMode;

        let import_source = if self.ctx.compiler_options.jsx_import_source.is_empty()
            && matches!(
                self.ctx.compiler_options.jsx_mode,
                JsxMode::ReactJsx | JsxMode::ReactJsxDev
            ) {
            "react"
        } else {
            self.ctx.compiler_options.jsx_import_source.as_str()
        };
        let Some(types_package) = Self::types_package_name_for_jsx_import_source(import_source)
        else {
            return false;
        };
        package_dir.ends_with(&format!("/node_modules/{types_package}"))
    }

    fn types_package_name_for_jsx_import_source(import_source: &str) -> Option<String> {
        let mut parts = import_source.split('/').filter(|part| !part.is_empty());
        let first = parts.next()?;
        if let Some(scope) = first.strip_prefix('@') {
            let second = parts.next()?;
            Some(format!("@types/{scope}__{second}"))
        } else {
            Some(format!("@types/{first}"))
        }
    }

    fn package_json_redirects_package_subpaths_to_js(text: &str) -> bool {
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact.contains("\"exports\"")
            && compact.contains("\"./*.js\":\"./*.js\"")
            && compact.contains("\"./*\":\"./*.js\"")
    }

    fn synthesized_computed_member_index_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(TypeId, TypeId, bool)> {
        let member_node = self.ctx.arena.get(member_idx)?;

        // PERF: Check if the member has a computed property name FIRST, before
        // computing the (potentially expensive) value type. Most class members
        // have simple identifier names, so this early exit avoids calling
        // get_type_of_function on every method body just to discard the result.
        let name_idx = if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            self.ctx.arena.get_property_decl(member_node)?.name
        } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            self.ctx.arena.get_method_decl(member_node)?.name
        } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            self.ctx.arena.get_accessor(member_node)?.name
        } else {
            return None;
        };

        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        // Only simple identifier expressions synthesize index signatures.
        // Property access chains (e.g. `[rC.x]`) resolve to specific named
        // properties via late-binding in TSC and do not create index signatures.
        // Using property access chains here would also risk incorrect key type
        // resolution due to circularity.
        let expr_node = self.ctx.arena.get(computed.expression)?;
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let key_type = self.get_type_of_node(computed.expression);
        if !matches!(key_type, TypeId::STRING | TypeId::NUMBER | TypeId::ANY) {
            return None;
        }

        // Only compute value type after confirming this is a computed member
        // with an entity expression key of the right type.
        let (value_type, is_static) = if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let prop = self.ctx.arena.get_property_decl(member_node)?;
            let is_static = self.has_static_modifier(&prop.modifiers);
            let value_type = if let Some(declared_type) =
                self.effective_class_property_declared_type(member_idx, prop)
            {
                declared_type
            } else {
                self.get_type_of_node(member_idx)
            };
            (value_type, is_static)
        } else if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            let method = self.ctx.arena.get_method_decl(member_node)?;
            (
                self.get_type_of_function(member_idx),
                self.has_static_modifier(&method.modifiers),
            )
        } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            let accessor = self.ctx.arena.get_accessor(member_node)?;
            let value_type = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                if accessor.type_annotation.is_some() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                }
            } else {
                let type_ann = accessor
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                    .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                    .map(|param| param.type_annotation)
                    .unwrap_or(NodeIndex::NONE);
                if type_ann.is_some() {
                    self.get_type_from_type_node(type_ann)
                } else {
                    self.get_type_of_node(member_idx)
                }
            };
            (value_type, self.has_static_modifier(&accessor.modifiers))
        } else {
            return None;
        };

        if self.type_contains_error(value_type) {
            return None;
        }

        Some((key_type, value_type, is_static))
    }

    fn computed_name_uses_entity_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return true;
        }
        if expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
        {
            return self.computed_name_uses_entity_expression(access.expression);
        }
        false
    }

    fn computed_name_is_non_global_symbol_property_access(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_access_expr(expr_node)
            .is_some_and(|access| {
                self.ctx
                    .arena
                    .get_identifier_at(access.expression)
                    .is_some_and(|ident| ident.escaped_text.as_str() == "Symbol")
                    && !self.is_identifier_reference_to_global_symbol(access.expression)
            })
    }
}
