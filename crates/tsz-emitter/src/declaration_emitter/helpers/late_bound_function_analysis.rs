//! Late-bound function assignment analysis and namespace emission.

use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::super::DeclarationEmitter;
use super::LateBoundAssignmentMember;
impl<'a> DeclarationEmitter<'a> {
    fn escape_non_ascii_for_double_quote(text: &str) -> String {
        let mut result = String::with_capacity(text.len() + 8);
        for ch in text.chars() {
            match ch {
                '\\' => result.push_str("\\\\"),
                '"' => result.push_str("\\\""),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '\0' => result.push_str("\\0"),
                ch if ch as u32 > 0x7E => {
                    let cp = ch as u32;
                    if cp > 0xFFFF {
                        let hi = 0xD800 + ((cp - 0x10000) >> 10);
                        let lo = 0xDC00 + ((cp - 0x10000) & 0x3FF);
                        result.push_str(&format!("\\u{hi:04X}\\u{lo:04X}"));
                    } else {
                        result.push_str(&format!("\\u{cp:04X}"));
                    }
                }
                _ => result.push(ch),
            }
        }
        result
    }

    fn late_bound_string_property_name_parts(text: &str) -> (String, Option<String>) {
        if Self::is_unquoted_property_name(text)
            && !tsz_solver::utils::is_numeric_literal_name(text)
        {
            (
                text.to_string(),
                Self::late_bound_namespace_member_name(text),
            )
        } else {
            (
                format!("\"{}\"", Self::escape_non_ascii_for_double_quote(text)),
                None,
            )
        }
    }

    fn late_bound_namespace_member_name(text: &str) -> Option<String> {
        (!Self::is_late_bound_reserved_binding_name(text)).then(|| text.to_string())
    }

    fn is_late_bound_reserved_binding_name(text: &str) -> bool {
        matches!(
            text,
            "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "new"
                | "null"
                | "return"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "implements"
                | "interface"
                | "let"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "static"
                | "yield"
        )
    }

    fn should_emit_late_bound_export_alias(property_name_text: &str) -> bool {
        Self::is_unquoted_property_name(property_name_text)
            && !tsz_solver::utils::is_numeric_literal_name(property_name_text)
            && Self::is_late_bound_reserved_binding_name(property_name_text)
    }

    fn is_late_bound_contextual_keyword_property_name(text: &str) -> bool {
        matches!(
            text,
            "abstract"
                | "as"
                | "asserts"
                | "any"
                | "async"
                | "await"
                | "boolean"
                | "constructor"
                | "declare"
                | "get"
                | "infer"
                | "is"
                | "keyof"
                | "module"
                | "namespace"
                | "never"
                | "readonly"
                | "require"
                | "number"
                | "object"
                | "set"
                | "string"
                | "symbol"
                | "type"
                | "undefined"
                | "unique"
                | "unknown"
                | "from"
                | "global"
                | "bigint"
                | "of"
        )
    }

    fn late_bound_synthetic_member_name(index: usize) -> String {
        let mut counter = index;
        if counter >= 8 {
            counter += 1;
        }
        if counter >= 13 {
            counter += 1;
        }

        if counter < 26 {
            format!("_{}", (b'a' + counter as u8) as char)
        } else {
            format!("_{}", counter - 26)
        }
    }

    fn js_late_bound_synthetic_member_name(
        property_name_text: &str,
        reserved_member_names: &mut FxHashSet<String>,
    ) -> String {
        let base = format!("_{property_name_text}");
        if reserved_member_names.insert(base.clone()) {
            return base;
        }

        let mut suffix = 1usize;
        loop {
            let candidate = format!("{base}_{suffix}");
            if reserved_member_names.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn resolved_const_late_bound_assignment_key(
        &self,
        sym_id: SymbolId,
        depth: u8,
    ) -> Option<(String, Option<String>)> {
        if depth > 8 {
            return None;
        }

        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|decl| decl.is_some())?
        };
        if !self.arena.is_const_variable_declaration(decl_idx) {
            return None;
        }

        let decl_node = self.arena.get(decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        let init_idx = var_decl.initializer;
        if init_idx.is_none() {
            return None;
        }
        let init_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(init_idx);
        let init_node = self.arena.get(init_idx)?;

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(init_node)?;
                Some(Self::late_bound_string_property_name_parts(&literal.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let literal = self.arena.get_literal(init_node)?;
                Some((Self::normalize_numeric_literal(literal.text.as_ref()), None))
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(init_node)?;
                let operand_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(unary.operand);
                let operand_node = self.arena.get(operand_idx)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let literal = self.arena.get_literal(operand_node)?;
                let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => {
                        Some((format!("-{normalized}"), None))
                    }
                    k if k == SyntaxKind::PlusToken as u16 => Some((normalized, None)),
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(init_idx)?;
                let next_sym = binder.file_locals.get(&name)?;
                self.resolved_const_late_bound_assignment_key(next_sym, depth + 1)
            }
            _ => None,
        }
    }

    fn late_bound_assignment_property_key_parts(
        &self,
        access_idx: NodeIndex,
    ) -> Option<(String, Option<String>)> {
        let access_node = self.arena.get(access_idx)?;
        let access = self.arena.get_access_expr(access_node)?;

        match access_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let name = self.get_identifier_text(access.name_or_argument)?;
                Some((name.clone(), Self::late_bound_namespace_member_name(&name)))
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let key_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
                let key_node = self.arena.get(key_idx)?;
                match key_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                    {
                        let literal = self.arena.get_literal(key_node)?;
                        Some(Self::late_bound_string_property_name_parts(&literal.text))
                    }
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        let literal = self.arena.get_literal(key_node)?;
                        Some((Self::normalize_numeric_literal(literal.text.as_ref()), None))
                    }
                    k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                        let unary = self.arena.get_unary_expr(key_node)?;
                        let operand_idx = self
                            .arena
                            .skip_parenthesized_and_assertions_and_comma(unary.operand);
                        let operand_node = self.arena.get(operand_idx)?;
                        if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                            return None;
                        }
                        let literal = self.arena.get_literal(operand_node)?;
                        let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                        match unary.operator {
                            k if k == SyntaxKind::MinusToken as u16 => {
                                Some((format!("-{normalized}"), None))
                            }
                            k if k == SyntaxKind::PlusToken as u16 => Some((normalized, None)),
                            _ => None,
                        }
                    }
                    k if k == SyntaxKind::Identifier as u16 => {
                        let binder = self.binder?;
                        let ident = self.get_identifier_text(key_idx)?;
                        binder
                            .file_locals
                            .get(&ident)
                            .and_then(|sym_id| {
                                self.resolved_const_late_bound_assignment_key(sym_id, 0)
                            })
                            .or_else(|| Some((format!("[{ident}]"), None)))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn late_bound_assignment_member_for_expression(
        &self,
        expr_idx: NodeIndex,
        root_name: &str,
        root_symbol: Option<SymbolId>,
    ) -> Option<LateBoundAssignmentMember> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }

        let lhs_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let lhs_node = self.arena.get(lhs_idx)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let receiver_matches_root = self.get_identifier_text(receiver_idx).as_deref()
            == Some(root_name)
            || (self.source_is_js_file
                && self
                    .module_exports_property_reference_name(receiver_idx)
                    .as_deref()
                    == Some(root_name));
        if !receiver_matches_root {
            return None;
        }
        if let Some(root_symbol) = root_symbol
            && let Some(receiver_name) = self.get_identifier_text(receiver_idx)
            && receiver_name == root_name
            && let Some(receiver_symbol) =
                self.resolve_identifier_symbol(receiver_idx, &receiver_name)
            && receiver_symbol != root_symbol
        {
            return None;
        }

        let rhs_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        if self.source_is_js_file {
            if self.get_identifier_text(rhs_idx).is_some()
                || self
                    .module_exports_property_reference_name(rhs_idx)
                    .is_some()
            {
                return None;
            }
            if self
                .arena
                .get(rhs_idx)
                .is_some_and(|rhs_node| rhs_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
            {
                return None;
            }
        }
        let (property_name_text, namespace_member_name) =
            self.late_bound_assignment_property_key_parts(lhs_idx)?;
        if self.source_is_js_file && property_name_text == "prototype" {
            return None;
        }
        let type_text = self
            .preferred_object_member_initializer_type_text(rhs_idx, self.indent_level + 1)
            .or_else(|| {
                self.resolve_declaration_type_text(&[rhs_idx], Some(rhs_idx))
                    .map(|resolved| resolved.emitted_type_text)
            })
            .unwrap_or_else(|| "any".to_string());

        Some(LateBoundAssignmentMember {
            property_name_text,
            namespace_member_name,
            type_text,
        })
    }

    fn collect_late_bound_assignment_members_from_node(
        &self,
        node_idx: NodeIndex,
        root_name: &str,
        root_symbol: Option<SymbolId>,
        declared_members: &FxHashSet<String>,
        members: &mut Vec<LateBoundAssignmentMember>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
            || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || node.kind == syntax_kind_ext::MODULE_DECLARATION
        {
            return;
        }

        if let Some(member) =
            self.late_bound_assignment_member_for_expression(node_idx, root_name, root_symbol)
        {
            Self::record_late_bound_assignment_member(members, member, declared_members);
        }

        if let Some(block) = self.arena.get_block(node) {
            for stmt_idx in block.statements.nodes.iter().copied() {
                self.collect_late_bound_assignment_members_from_node(
                    stmt_idx,
                    root_name,
                    root_symbol,
                    declared_members,
                    members,
                );
            }
            return;
        }

        if let Some(loop_data) = self.arena.get_loop(node) {
            if node.kind == syntax_kind_ext::DO_STATEMENT {
                self.collect_late_bound_assignment_members_from_node(
                    loop_data.statement,
                    root_name,
                    root_symbol,
                    declared_members,
                    members,
                );
                if loop_data.condition.is_some() {
                    self.collect_late_bound_assignment_members_from_node(
                        loop_data.condition,
                        root_name,
                        root_symbol,
                        declared_members,
                        members,
                    );
                }
                return;
            }

            if loop_data.initializer.is_some() {
                self.collect_late_bound_assignment_members_from_node(
                    loop_data.initializer,
                    root_name,
                    root_symbol,
                    declared_members,
                    members,
                );
            }
            if loop_data.condition.is_some() {
                self.collect_late_bound_assignment_members_from_node(
                    loop_data.condition,
                    root_name,
                    root_symbol,
                    declared_members,
                    members,
                );
            }
            self.collect_late_bound_assignment_members_from_node(
                loop_data.statement,
                root_name,
                root_symbol,
                declared_members,
                members,
            );
            if loop_data.incrementor.is_some() {
                self.collect_late_bound_assignment_members_from_node(
                    loop_data.incrementor,
                    root_name,
                    root_symbol,
                    declared_members,
                    members,
                );
            }
            return;
        }

        if let Some(for_in_of) = self.arena.get_for_in_of(node) {
            self.collect_late_bound_assignment_members_from_node(
                for_in_of.initializer,
                root_name,
                root_symbol,
                declared_members,
                members,
            );
            self.collect_late_bound_assignment_members_from_node(
                for_in_of.expression,
                root_name,
                root_symbol,
                declared_members,
                members,
            );
            self.collect_late_bound_assignment_members_from_node(
                for_in_of.statement,
                root_name,
                root_symbol,
                declared_members,
                members,
            );
            return;
        }

        for child_idx in self.arena.get_children(node_idx) {
            self.collect_late_bound_assignment_members_from_node(
                child_idx,
                root_name,
                root_symbol,
                declared_members,
                members,
            );
        }
    }

    fn record_late_bound_assignment_member(
        members: &mut Vec<LateBoundAssignmentMember>,
        member: LateBoundAssignmentMember,
        declared_members: &FxHashSet<String>,
    ) {
        if declared_members.contains(&member.property_name_text) {
            return;
        }
        if let Some(existing) =
            members
                .iter_mut()
                .find(|existing: &&mut LateBoundAssignmentMember| {
                    existing.property_name_text == member.property_name_text
                })
        {
            *existing = member;
        } else {
            members.push(member);
        }
    }

    fn declared_late_bound_namespace_member_names(&self, root_name: &str) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        let Some(source_file) = self.arena.source_files.first() else {
            return names;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = self.arena.get_module(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(module.name).as_deref() != Some(root_name) {
                continue;
            }
            self.collect_declared_namespace_member_names(module.body, &mut names);
        }

        names
    }

    fn collect_declared_namespace_member_names(
        &self,
        node_idx: NodeIndex,
        names: &mut FxHashSet<String>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        if let Some(var_decl) = self.arena.get_variable_declaration(node)
            && let Some(name) = self.get_identifier_text(var_decl.name)
        {
            names.insert(name);
        } else if let Some(func) = self.arena.get_function(node)
            && let Some(name) = self.get_identifier_text(func.name)
        {
            names.insert(name);
        } else if let Some(class) = self.arena.get_class(node)
            && let Some(name) = self.get_identifier_text(class.name)
        {
            names.insert(name);
        } else if let Some(iface) = self.arena.get_interface(node)
            && let Some(name) = self.get_identifier_text(iface.name)
        {
            names.insert(name);
        } else if let Some(alias) = self.arena.get_type_alias(node)
            && let Some(name) = self.get_identifier_text(alias.name)
        {
            names.insert(name);
        } else if let Some(enum_data) = self.arena.get_enum(node)
            && let Some(name) = self.get_identifier_text(enum_data.name)
        {
            names.insert(name);
        }

        for child_idx in self.arena.get_children(node_idx) {
            self.collect_declared_namespace_member_names(child_idx, names);
        }
    }

    /// Returns true when `initializer` is the kind of node
    /// `emit_function_initializer_call_signature` can synthesize a call
    /// signature for — that is, a function expression or arrow function.
    /// Used by the late-bound expando-function path to bail out
    /// **before** any partial output is written, so non-function
    /// initializers do not leak a broken `: {` into the .d.ts.
    fn initializer_is_function_like_for_late_bound(&self, initializer: NodeIndex) -> bool {
        self.arena.get(initializer).is_some_and(|node| {
            node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
        })
    }

    pub(in crate::declaration_emitter) fn collect_ts_late_bound_assignment_members(
        &self,
        root_name_idx: NodeIndex,
    ) -> Vec<LateBoundAssignmentMember> {
        if self.source_is_declaration_file {
            return Vec::new();
        }

        let Some(root_name) = self.get_identifier_text(root_name_idx) else {
            return Vec::new();
        };
        if self.source_is_js_file && self.js_export_equals_names.contains(&root_name) {
            return Vec::new();
        }
        let root_symbol = self.binder.and_then(|binder| {
            binder
                .get_node_symbol(root_name_idx)
                .or_else(|| binder.file_locals.get(&root_name))
        });
        let Some(source_file) = self.arena.source_files.first() else {
            return Vec::new();
        };

        let mut members = Vec::new();
        let declared_members = self.declared_late_bound_namespace_member_names(&root_name);
        for &stmt_idx in &source_file.statements.nodes {
            if self.source_is_js_file && self.js_class_static_member_stmts.contains(&stmt_idx) {
                continue;
            }
            self.collect_late_bound_assignment_members_from_node(
                stmt_idx,
                &root_name,
                root_symbol,
                &declared_members,
                &mut members,
            );
        }

        members
    }

    pub(in crate::declaration_emitter) fn should_emit_ts_late_bound_function_namespace(
        &self,
        func_idx: NodeIndex,
        name_idx: NodeIndex,
        is_overload: bool,
    ) -> bool {
        if !is_overload {
            return true;
        }

        let Some(root_name) = self.get_identifier_text(name_idx) else {
            return true;
        };
        let Some(source_file) = self.arena.source_files.first() else {
            return true;
        };

        let mut found_current = false;
        for &stmt_idx in &source_file.statements.nodes {
            if stmt_idx == func_idx {
                found_current = true;
                continue;
            }
            if !found_current {
                continue;
            }

            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let Some(func) = self.arena.get_function(stmt_node) else {
                continue;
            };
            let Some(name_text) = self.get_identifier_text(func.name) else {
                continue;
            };
            if name_text == root_name && func.body.is_none() {
                return false;
            }
        }

        true
    }

    pub(in crate::declaration_emitter) fn emit_ts_late_bound_function_initializer_type_annotation(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        let members = self.collect_ts_late_bound_assignment_members(decl_name);
        if members.is_empty() {
            return false;
        }

        // The `: {` and the call signature were previously written before
        // checking whether the initializer is actually a function; for
        // non-function initializers (array literals, object literals, etc.)
        // `emit_function_initializer_call_signature` then returned false
        // and the caller fell through to a different branch — but the
        // partial `: {\n    ` was already in the output, producing invalid
        // TypeScript like `declare var t: {\n    : number[];`. Probe the
        // initializer shape first and bail out before any writes when the
        // late-bound function pattern does not apply.
        if !self.initializer_is_function_like_for_late_bound(initializer) {
            return false;
        }

        self.write(": {");
        self.write_line();
        self.increase_indent();
        self.write_indent();
        if !self.emit_function_initializer_call_signature(initializer) {
            self.decrease_indent();
            return false;
        }
        self.write(";");
        self.write_line();

        for member in members {
            self.write_indent();
            if let Some(method_text) = Self::late_bound_function_type_method_signature_text(
                &member.property_name_text,
                &member.type_text,
            ) {
                self.write(&method_text);
            } else {
                self.write(&member.property_name_text);
                self.write(": ");
                self.write(&member.type_text);
            }
            self.write(";");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        true
    }

    fn late_bound_function_type_method_signature_text(
        property_name_text: &str,
        type_text: &str,
    ) -> Option<String> {
        if !Self::is_unquoted_property_name(property_name_text) {
            return None;
        }
        let type_text = type_text.trim();
        let arrow_idx = Self::top_level_function_type_arrow(type_text)?;
        let params_text = type_text[..arrow_idx].trim_end();
        if params_text.starts_with("new ") {
            return None;
        }
        let return_type_text = type_text[arrow_idx + 2..].trim_start();
        if return_type_text.is_empty()
            || !(params_text.starts_with('(') || params_text.starts_with('<'))
        {
            return None;
        }

        Some(format!(
            "{property_name_text}{params_text}: {return_type_text}"
        ))
    }

    fn top_level_function_type_arrow(type_text: &str) -> Option<usize> {
        let mut paren_depth = 0u32;
        let mut bracket_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut angle_depth = 0u32;
        let bytes = type_text.as_bytes();
        let mut idx = 0;
        while idx + 1 < bytes.len() {
            match bytes[idx] {
                b'=' if bytes[idx + 1] == b'>'
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    return Some(idx);
                }
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                _ => {}
            }
            idx += 1;
        }
        None
    }

    fn generate_unique_namespace_member_name(
        &self,
        base: &str,
        reserved_member_names: &FxHashSet<String>,
    ) -> String {
        let mut i = 1usize;
        loop {
            let candidate = format!("{base}_{i}");
            if !self.reserved_names.contains(&candidate)
                && !reserved_member_names.contains(&candidate)
            {
                return candidate;
            }
            i += 1;
        }
    }

    pub(in crate::declaration_emitter) fn emit_ts_late_bound_function_namespace_from_members(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
        members: &[LateBoundAssignmentMember],
    ) {
        if members.is_empty() {
            return;
        }

        let namespace_members: Vec<LateBoundAssignmentMember> = members
            .iter()
            .filter(|&member| {
                member.namespace_member_name.is_some()
                    || !member.property_name_text.is_empty()
                        && Self::should_emit_late_bound_export_alias(&member.property_name_text)
            })
            .cloned()
            .collect();

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);

        if namespace_members.is_empty() {
            self.write(" { }");
            self.write_line();
            return;
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();
        let has_export_aliases = namespace_members
            .iter()
            .any(|member| member.namespace_member_name.is_none());
        let mut export_aliases: Vec<(String, String)> = Vec::new();
        let mut reserved_member_names: FxHashSet<String> = namespace_members
            .iter()
            .filter_map(|member| member.namespace_member_name.clone())
            .collect();
        let mut synthetic_member_count = 0usize;
        let mut emitted_keyword_export_alias = false;
        for member in namespace_members {
            let mut export_alias = None;
            let namespace_member_name = if let Some(namespace_member_name) =
                member.namespace_member_name.as_deref()
            {
                if self.source_is_js_file
                    && (self.reserved_names.contains(namespace_member_name)
                        || emitted_keyword_export_alias
                            && !Self::is_late_bound_contextual_keyword_property_name(
                                namespace_member_name,
                            ))
                {
                    let synthetic_name = self.generate_unique_namespace_member_name(
                        namespace_member_name,
                        &reserved_member_names,
                    );
                    reserved_member_names.insert(synthetic_name.clone());
                    export_alias =
                        Some((synthetic_name.clone(), namespace_member_name.to_string()));
                    synthetic_name
                } else {
                    namespace_member_name.to_string()
                }
            } else {
                let synthetic_name = if self.source_is_js_file {
                    Self::js_late_bound_synthetic_member_name(
                        &member.property_name_text,
                        &mut reserved_member_names,
                    )
                } else {
                    loop {
                        let candidate =
                            Self::late_bound_synthetic_member_name(synthetic_member_count);
                        synthetic_member_count += 1;
                        if reserved_member_names.insert(candidate.clone()) {
                            break candidate;
                        }
                    }
                };
                export_alias = Some((synthetic_name.clone(), member.property_name_text.clone()));
                synthetic_name
            };
            self.write_indent();
            if export_alias.is_none()
                && has_export_aliases
                && member.namespace_member_name.is_some()
            {
                self.write("export ");
            }
            if self.source_is_js_file {
                self.write("let ");
            } else {
                self.write("var ");
            }
            self.write(&namespace_member_name);
            self.write(": ");
            self.write(&member.type_text);
            self.write(";");
            self.write_line();
            if let Some((local_name, exported_name)) = export_alias {
                if self.source_is_js_file {
                    self.write_indent();
                    self.write("export { ");
                    self.write(&local_name);
                    self.write(" as ");
                    self.write(&exported_name);
                    self.write(" };");
                    self.write_line();
                } else {
                    export_aliases.push((local_name, exported_name));
                }
                emitted_keyword_export_alias |=
                    Self::is_late_bound_reserved_binding_name(&member.property_name_text);
            }
        }
        if !export_aliases.is_empty() {
            self.write_indent();
            self.write("export { ");
            for (idx, (local_name, exported_name)) in export_aliases.iter().enumerate() {
                if idx > 0 {
                    self.write(", ");
                }
                self.write(local_name);
                self.write(" as ");
                self.write(exported_name);
            }
            self.write(" };");
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }
}
