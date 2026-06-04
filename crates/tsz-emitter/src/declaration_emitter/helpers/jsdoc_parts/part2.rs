impl<'a> DeclarationEmitter<'a> {
    fn normalize_jsdoc_generic_type_reference(type_expr: &str) -> Option<String> {
        let open = type_expr.find('<')?;
        if !type_expr.ends_with('>') {
            return None;
        }

        let base_end = type_expr[..open]
            .strip_suffix('.')
            .map_or(open, |base| base.len());
        let base = type_expr[..base_end].trim();
        if base.is_empty()
            || !base
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        let args = type_expr[open + 1..type_expr.len() - 1].trim();
        if args.is_empty() {
            return Some(match base {
                "Array" => "any[]".to_string(),
                "Promise" => "Promise<any>".to_string(),
                _ => format!("{base}<>"),
            });
        }

        let normalized_args = Self::split_jsdoc_params(args)
            .into_iter()
            .map(Self::normalize_jsdoc_type_expr)
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("{base}<{normalized_args}>"))
    }

    fn normalize_jsdoc_object_index_type(type_expr: &str) -> Option<String> {
        let args = type_expr
            .strip_prefix("Object<")
            .or_else(|| type_expr.strip_prefix("Object.<"))?
            .strip_suffix('>')?;
        let parts = Self::split_jsdoc_params(args);
        if parts.len() != 2 {
            return None;
        }
        let key = Self::normalize_jsdoc_type_expr(parts[0]);
        let value = Self::normalize_jsdoc_type_expr(parts[1]);
        let key = match key.as_str() {
            "string" | "number" | "symbol" => key,
            _ => "string".to_string(),
        };
        Some(format!("{{\n    [x: {key}]: {value};\n}}"))
    }

    fn strip_balanced_parens(text: &str) -> Option<&str> {
        let inner = text.strip_prefix('(')?.strip_suffix(')')?;
        let mut depth = 0usize;
        for (index, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && index != text.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }
        Some(inner)
    }

    fn split_top_level_jsdoc_union(text: &str) -> Vec<&str> {
        let mut result = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (index, ch) in text.char_indices() {
            match ch {
                '(' | '<' | '{' | '[' => depth += 1,
                ')' | '>' | '}' | ']' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => {
                    result.push(text[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            }
        }
        result.push(text[start..].trim());
        result
    }

    /// Returns true when a JSDoc type expression contains syntax that cannot
    /// be emitted verbatim as TypeScript and should instead be resolved through
    /// the checker/solver type cache.
    pub(crate) fn jsdoc_type_needs_checker_resolution(type_text: &str) -> bool {
        let t = type_text.trim();
        t.starts_with("function(") || t.starts_with("function (")
    }

    /// Convert a JSDoc `function(...)` type to TypeScript arrow function syntax.
    /// e.g. `function(this:Object, ...*):*` -> `(this: Object, ...args: any[]) => any`
    pub(crate) fn convert_jsdoc_function_type(type_text: &str) -> Option<String> {
        let t = type_text.trim();
        let rest = t.strip_prefix("function")?.trim();
        let rest = rest.strip_prefix('(')?;

        // Find matching closing paren (handling nested parens)
        let mut depth = 1usize;
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
        let params_str = &rest[..close_idx];
        let after_close = rest[close_idx + 1..].trim();

        // Parse return type (after `:`)
        let return_type = if let Some(ret) = after_close.strip_prefix(':') {
            Self::normalize_jsdoc_type_atom(ret.trim())
        } else {
            "any".to_string()
        };

        // Parse parameters
        let mut ts_params = Vec::new();
        let mut construct_return_type = None;
        if !params_str.trim().is_empty() {
            let raw_params = Self::split_jsdoc_params(params_str);
            let mut unnamed_idx = 0u32;
            for raw in &raw_params {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                let (is_rest, raw) = if let Some(s) = raw.strip_prefix("...") {
                    (true, s.trim())
                } else {
                    (false, raw)
                };

                // Check for `name:Type` or just `Type`
                if let Some(colon) = Self::find_param_colon(raw) {
                    let name = raw[..colon].trim();
                    let ptype = Self::normalize_jsdoc_type_atom(raw[colon + 1..].trim());
                    if name == "new" {
                        construct_return_type = Some(ptype);
                        unnamed_idx = unnamed_idx.max(1);
                        continue;
                    }
                    if is_rest {
                        ts_params.push(format!("...args: {ptype}[]"));
                    } else {
                        ts_params.push(format!("{name}: {ptype}"));
                    }
                } else {
                    let ptype = Self::normalize_jsdoc_type_atom(raw);
                    if is_rest {
                        ts_params.push(format!("...args: {ptype}[]"));
                    } else {
                        let name = format!("arg{unnamed_idx}");
                        unnamed_idx += 1;
                        ts_params.push(format!("{name}: {ptype}"));
                    }
                }
            }
        }

        if let Some(construct_return_type) = construct_return_type {
            Some(format!(
                "new ({}) => {}",
                ts_params.join(", "),
                construct_return_type
            ))
        } else {
            Some(format!("({}) => {}", ts_params.join(", "), return_type))
        }
    }

    /// Normalize a single JSDoc type atom: `*` -> `any`, otherwise pass through.
    fn normalize_jsdoc_type_atom(s: &str) -> String {
        let s = s.trim();
        if Self::jsdoc_module_reference_type_falls_back_to_any(s) {
            return "any".to_string();
        }
        if let Some((base, args)) = Self::split_jsdoc_generic_atom(s) {
            if args.trim().is_empty() {
                return match base {
                    "Array" => "any[]".to_string(),
                    "Promise" => "Promise<any>".to_string(),
                    _ => format!("{base}<>"),
                };
            }
            let args = Self::split_jsdoc_params(args)
                .into_iter()
                .map(Self::normalize_jsdoc_type_expr)
                .collect::<Vec<_>>();
            return format!("{base}<{}>", args.join(", "));
        }
        match s {
            "*" | "?" => "any".to_string(),
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            "Void" => "void".to_string(),
            "Undefined" => "undefined".to_string(),
            "Null" => "null".to_string(),
            "function" => "Function".to_string(),
            "event" => "Event".to_string(),
            // tsc treats empty-args generic JSDoc references as implicit-any
            // (`Array.<>` → `any[]`); without the empty generic arms the DTS
            // surfaces literal `Array<>` tokens that are not valid TypeScript.
            "array" | "Array" | "Array.<>" | "Array<>" => "any[]".to_string(),
            "promise" | "Promise" | "Promise.<>" | "Promise<>" => "Promise<any>".to_string(),
            _ => s.to_string(),
        }
    }

    fn jsdoc_module_reference_type_falls_back_to_any(type_text: &str) -> bool {
        type_text
            .trim()
            .strip_prefix("module:")
            .is_some_and(|rest| !rest.trim().is_empty())
    }

    fn split_jsdoc_generic_atom(s: &str) -> Option<(&str, &str)> {
        let open = s.find('<')?;
        if !s.ends_with('>') {
            return None;
        }
        let base = s[..open].trim();
        if base.is_empty()
            || !base
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in s.char_indices().skip(open) {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && idx != s.len() - 1 {
                        return None;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        Some((base, &s[open + 1..s.len() - 1]))
    }

    /// Split JSDoc function parameters by commas, respecting nested parens.
    fn split_jsdoc_params(s: &str) -> Vec<&str> {
        let mut result = Vec::new();
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        let mut start = 0;
        for (i, ch) in s.char_indices() {
            if let Some(q) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == q {
                    quote = None;
                }
                continue;
            }
            match ch {
                '\'' | '"' | '`' => quote = Some(ch),
                '(' | '<' | '{' | '[' => depth += 1,
                ')' | '>' | '}' | ']' => {
                    depth = depth.saturating_sub(1);
                }
                ',' if depth == 0 => {
                    result.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        result.push(&s[start..]);
        result
    }

    /// Find the colon separating name from type in a JSDoc param like `this:Object`.
    /// Returns None if no colon found (the whole thing is a type).
    fn find_param_colon(s: &str) -> Option<usize> {
        // The name part should be a simple identifier (letters, digits, _, $)
        // If the first `:` appears after such a name, it's a name:type separator.
        let s = s.trim();
        for (i, ch) in s.char_indices() {
            if ch == ':' {
                return Some(i);
            }
            if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$' {
                return None;
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_param_decl(
        line: &str,
    ) -> Option<JsdocParamDecl> {
        let rest = line.strip_prefix("@param")?.trim();
        let (raw_type_expr, raw_name) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let raw_name = raw_name
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        let (name, optional_name) = Self::normalize_jsdoc_param_name(raw_name);

        let mut type_expr = raw_type_expr.trim();
        let optional_type = type_expr.ends_with('=');
        if optional_type {
            type_expr = type_expr[..type_expr.len() - 1].trim();
        }

        let (rest_param, base_type) = if let Some(stripped) = type_expr.strip_prefix("...") {
            (true, stripped.trim())
        } else {
            (false, type_expr)
        };

        Some(JsdocParamDecl {
            name,
            type_text: Self::normalize_jsdoc_type_text(base_type, rest_param),
            optional: optional_name || optional_type,
            rest: rest_param,
        })
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_param_decls(
        jsdoc: &str,
    ) -> Vec<JsdocParamDecl> {
        jsdoc
            .lines()
            .map(|raw_line| raw_line.trim_start_matches('*').trim())
            .filter_map(Self::parse_jsdoc_param_decl)
            .collect()
    }
}
