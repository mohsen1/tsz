//! Reference matching, literal parsing, and symbol resolution utilities
//! for control flow analysis.
//!
//! Extracted from `narrowing.rs` to keep modules focused.
//! Contains:
//! - Reference matching (`is_matching_reference`, `property_reference`)
//! - Literal value extraction from AST nodes (`literal_number_from_node`, `literal_atom_from`_*)
//! - Numeric parsing (`parse_numeric_literal_value`, `parse_radix_digits`, `bigint_base_to_decimal`)
//! - Symbol resolution (`reference_symbol`, `resolve_namespace_member`, `resolve_alias_symbol`)

use crate::query_boundaries::flow_analysis::{LiteralValueKind, classify_for_literal_value};
use std::borrow::Cow;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use super::{FlowAnalyzer, PropertyKey};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn strip_numeric_separators<'b>(&self, text: &'b str) -> Cow<'b, str> {
        if !text.as_bytes().contains(&b'_') {
            return Cow::Borrowed(text);
        }

        let mut out = String::with_capacity(text.len());
        for &byte in text.as_bytes() {
            if byte != b'_' {
                out.push(byte as char);
            }
        }
        Cow::Owned(out)
    }

    pub(crate) fn parse_numeric_literal_value(
        &self,
        value: Option<f64>,
        text: &str,
    ) -> Option<f64> {
        if let Some(value) = value {
            return Some(value);
        }

        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::parse_radix_digits(rest, 16);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::parse_radix_digits(rest, 2);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::parse_radix_digits(rest, 8);
        }

        if text.as_bytes().contains(&b'_') {
            let cleaned = self.strip_numeric_separators(text);
            return cleaned.as_ref().parse::<f64>().ok();
        }

        text.parse::<f64>().ok()
    }

    pub(crate) fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
        if text.is_empty() {
            return None;
        }

        let mut value = 0f64;
        let base_value = base as f64;
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;
            value = value * base_value + digit as f64;
        }

        if !saw_digit {
            return None;
        }

        Some(value)
    }

    pub(crate) fn normalize_bigint_literal<'b>(&self, text: &'b str) -> Option<Cow<'b, str>> {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::bigint_base_to_decimal(rest, 16).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::bigint_base_to_decimal(rest, 2).map(Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::bigint_base_to_decimal(rest, 8).map(Cow::Owned);
        }

        match self.strip_numeric_separators(text) {
            Cow::Borrowed(cleaned) => {
                let trimmed = cleaned.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned.len() {
                    return Some(Cow::Borrowed(cleaned));
                }
                Some(Cow::Borrowed(trimmed))
            }
            Cow::Owned(mut cleaned) => {
                let cleaned_ref = cleaned.as_str();
                let trimmed = cleaned_ref.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned_ref.len() {
                    return Some(Cow::Owned(cleaned));
                }

                let trim_len = cleaned_ref.len() - trimmed.len();
                cleaned.drain(..trim_len);
                Some(Cow::Owned(cleaned))
            }
        }
    }

    pub(crate) fn bigint_base_to_decimal(text: &str, base: u32) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let mut digits: Vec<u8> = vec![0];
        let mut saw_digit = false;
        for &byte in text.as_bytes() {
            if byte == b'_' {
                continue;
            }

            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as u32,
                b'a'..=b'f' => (byte - b'a' + 10) as u32,
                b'A'..=b'F' => (byte - b'A' + 10) as u32,
                _ => return None,
            };
            if digit >= base {
                return None;
            }
            saw_digit = true;

            let mut carry = digit;
            for slot in &mut digits {
                let value = (*slot as u32) * base + carry;
                *slot = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                digits.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        if !saw_digit {
            return None;
        }

        while digits.len() > 1 {
            if let Some(&last) = digits.last() {
                if last == 0 {
                    digits.pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        Some(out)
    }

    /// Check if two references point to the same symbol or property access chain.
    pub(crate) fn is_matching_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        use tracing::trace;

        let a = self.skip_parenthesized(a);
        let b = self.skip_parenthesized(b);

        // Fast path: same node index
        if a == b {
            return true;
        }

        // Check cache first to avoid O(N²) repeated comparisons
        let key = (a.0.min(b.0), a.0.max(b.0)); // Normalize order for symmetric lookup
        if let Some(shared) = self.shared_reference_match_cache
            && let Some(&cached) = shared.borrow().get(&key)
        {
            return cached;
        }
        if let Some(&cached) = self.reference_match_cache.borrow().get(&key) {
            return cached;
        }

        trace!(?a, ?b, "is_matching_reference called");

        let result = self.is_matching_reference_uncached(a, b);

        if let Some(shared) = self.shared_reference_match_cache {
            shared.borrow_mut().insert(key, result);
        }
        self.reference_match_cache.borrow_mut().insert(key, result);
        result
    }

    /// Internal uncached implementation of reference matching.
    fn is_matching_reference_uncached(&self, a: NodeIndex, b: NodeIndex) -> bool {
        use tracing::trace;

        if let (Some(node_a), Some(node_b)) = (self.arena.get(a), self.arena.get(b)) {
            if node_a.kind == SyntaxKind::ThisKeyword as u16
                && node_b.kind == SyntaxKind::ThisKeyword as u16
            {
                trace!("Matched: both are 'this'");
                return true;
            }
            if node_a.kind == SyntaxKind::SuperKeyword as u16
                && node_b.kind == SyntaxKind::SuperKeyword as u16
            {
                trace!("Matched: both are 'super'");
                return true;
            }
        }

        let sym_a = self.reference_symbol(a);
        let sym_b = self.reference_symbol(b);
        trace!(?sym_a, ?sym_b, "Symbol comparison");
        if sym_a.is_some() && sym_a == sym_b {
            let member_like_a = self.is_member_like_reference(a);
            let member_like_b = self.is_member_like_reference(b);
            if !member_like_a && !member_like_b {
                trace!("Matched: same symbol");
                return true;
            }
            trace!(
                ?a,
                ?b,
                member_like_a,
                member_like_b,
                "Same symbol but member-like refs require structural match"
            );
        }

        let property_match = self.is_matching_property_reference(a, b);
        trace!(?property_match, "Property reference match result");
        property_match
    }

    fn is_member_like_reference(&self, idx: NodeIndex) -> bool {
        let idx = self.skip_parens_and_assertions(idx);
        self.arena.get(idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::QUALIFIED_NAME
        })
    }

    pub(crate) fn is_matching_property_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        // Try the fast path: both sides produce an (object, atom) pair.
        if let (Some((a_base, a_name)), Some((b_base, b_name))) =
            (self.property_reference(a), self.property_reference(b))
        {
            if a_name == b_name {
                return self.is_matching_reference(a_base, b_base);
            }
            return false;
        }

        // Fallback for element accesses with non-literal keys (e.g. obj[key]).
        // property_reference returns None when the key isn't a literal, but two
        // element accesses with matching object and matching key variable should
        // still be considered the same reference. tsc's isMatchingReference
        // handles this by recursively comparing the argument expressions.
        let a_skipped = self.skip_parens_and_assertions(a);
        let b_skipped = self.skip_parens_and_assertions(b);
        let (Some(node_a), Some(node_b)) = (self.arena.get(a_skipped), self.arena.get(b_skipped))
        else {
            return false;
        };
        if node_a.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && node_b.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let (Some(access_a), Some(access_b)) = (
                self.arena.get_access_expr(node_a),
                self.arena.get_access_expr(node_b),
            ) else {
                return false;
            };
            if access_a.question_dot_token || access_b.question_dot_token {
                return false;
            }
            return self.is_matching_reference(access_a.expression, access_b.expression)
                && self
                    .is_matching_reference(access_a.name_or_argument, access_b.name_or_argument);
        }

        false
    }

    pub(crate) fn property_reference(&self, idx: NodeIndex) -> Option<(NodeIndex, Atom)> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.property_reference(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.property_reference(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let ident = self.arena.get_identifier_at(access.name_or_argument)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((access.expression, name));
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_atom_from_node_or_type(access.name_or_argument)?;
            return Some((access.expression, name));
        }

        // QualifiedName (e.g., `x.p` inside `typeof x.p` in type position).
        // Treat as equivalent to PropertyAccessExpression for reference matching,
        // so flow narrowing conditions on `x.p` (PropertyAccess) match `x.p` (QualifiedName).
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            let ident = self.arena.get_identifier_at(qn.right)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((qn.left, name));
        }

        None
    }

    pub(crate) fn literal_atom_from_node_or_type(&self, idx: NodeIndex) -> Option<Atom> {
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some(self.interner.intern_string(name));
        }
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(self.atom_from_numeric_value(value));
        }
        self.literal_atom_from_type(idx)
    }

    pub(crate) fn literal_atom_and_kind_from_node_or_type(
        &self,
        idx: NodeIndex,
    ) -> Option<(Atom, bool)> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some((self.atom_from_numeric_value(value), true));
        }
        if let Some(name) = self.literal_string_from_node(idx) {
            return Some((self.interner.intern_string(name), false));
        }

        // Handle private identifiers (e.g., #a in x)
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            let ident = self.arena.get_identifier(node)?;
            return Some((self.interner.intern_string(&ident.escaped_text), false));
        }

        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::String(atom) => Some((atom, false)),
            LiteralValueKind::Number(value) => Some((self.atom_from_numeric_value(value), true)),
            LiteralValueKind::None => None,
        }
    }

    pub(crate) fn literal_number_from_node_or_type(&self, idx: NodeIndex) -> Option<f64> {
        if let Some(value) = self.literal_number_from_node(idx) {
            return Some(value);
        }
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::Number(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn literal_atom_from_type(&self, idx: NodeIndex) -> Option<Atom> {
        let node_types = self.node_types?;
        let type_id = *node_types.get(&idx.0)?;
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::String(atom) => Some(atom),
            LiteralValueKind::Number(value) => Some(self.atom_from_numeric_value(value)),
            LiteralValueKind::None => None,
        }
    }

    pub(crate) fn property_key_from_name(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let name_idx = self.skip_parens_and_assertions(name_idx);
        let node = self.arena.get(name_idx)?;

        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            if let Some(value) = self.literal_number_from_node_or_type(computed.expression)
                && value.fract() == 0.0
                && value >= 0.0
            {
                return Some(PropertyKey::Index(value as usize));
            }
            if let Some(atom) = self.literal_atom_from_node_or_type(computed.expression) {
                return Some(PropertyKey::Atom(atom));
            }
            return None;
        }

        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(PropertyKey::Atom(
                self.interner.intern_string(&ident.escaped_text),
            ));
        }

        if let Some((atom, _)) = self.literal_atom_and_kind_from_node_or_type(name_idx) {
            return Some(PropertyKey::Atom(atom));
        }

        None
    }

    pub(crate) fn literal_number_from_node(&self, idx: NodeIndex) -> Option<f64> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                self.parse_numeric_literal_value(lit.value, &lit.text)
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let op = unary.operator;
                if op != SyntaxKind::MinusToken as u16 && op != SyntaxKind::PlusToken as u16 {
                    return None;
                }
                let operand = self.skip_parenthesized(unary.operand);
                let operand_node = self.arena.get(operand)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.arena.get_literal(operand_node)?;
                let value = self.parse_numeric_literal_value(lit.value, &lit.text)?;
                Some(if op == SyntaxKind::MinusToken as u16 {
                    -value
                } else {
                    value
                })
            }
            _ => None,
        }
    }

    pub(crate) fn atom_from_numeric_value(&self, value: f64) -> Atom {
        let normalized_bits = if value == 0.0 && !value.is_sign_negative() {
            0.0f64.to_bits()
        } else {
            value.to_bits()
        };

        // Check shared cache first
        if let Some(shared) = self.shared_numeric_atom_cache
            && let Ok(cache) = shared.try_borrow()
            && let Some(&cached) = cache.get(&normalized_bits)
        {
            return cached;
        }

        if let Ok(cache) = self.numeric_atom_cache.try_borrow()
            && let Some(&cached) = cache.get(&normalized_bits)
        {
            return cached;
        }

        let atom = if value == 0.0 {
            if value.is_sign_negative() {
                self.interner.intern_string("-0")
            } else {
                self.interner.intern_string("0")
            }
        } else if value.is_finite()
            && value.fract() == 0.0
            && value >= i64::MIN as f64
            && value <= i64::MAX as f64
        {
            let int = value as i64;
            if int as f64 == value {
                self.intern_i64_decimal(int)
            } else {
                self.interner.intern_string(&value.to_string())
            }
        } else {
            self.interner.intern_string(&value.to_string())
        };

        if let Some(shared) = self.shared_numeric_atom_cache
            && let Ok(mut cache) = shared.try_borrow_mut()
        {
            cache.insert(normalized_bits, atom);
        }

        if let Ok(mut cache) = self.numeric_atom_cache.try_borrow_mut() {
            cache.insert(normalized_bits, atom);
        }
        atom
    }

    fn intern_i64_decimal(&self, value: i64) -> Atom {
        if value == 0 {
            return self.interner.intern_string("0");
        }

        let negative = value < 0;
        let mut n = value.unsigned_abs();
        let mut buf = [0u8; 21];
        let mut pos = buf.len();

        while n != 0 {
            pos -= 1;
            buf[pos] = b'0' + (n % 10) as u8;
            n /= 10;
        }

        if negative {
            pos -= 1;
            buf[pos] = b'-';
        }

        let text = std::str::from_utf8(&buf[pos..]).unwrap_or("0");
        self.interner.intern_string(text)
    }

    pub(crate) fn reference_base(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.reference_base(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.reference_base(assertion.expression);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return Some(access.expression);
        }

        None
    }

    pub(crate) fn reference_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let idx = self.skip_parenthesized(idx);
        if let Some(&cached) = self.reference_symbol_cache.borrow().get(&idx.0) {
            return cached;
        }

        let mut visited = Vec::new();
        let result = self.reference_symbol_inner(idx, &mut visited);
        self.reference_symbol_cache
            .borrow_mut()
            .insert(idx.0, result);
        result
    }

    pub(crate) fn reference_symbol_inner(
        &self,
        idx: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let idx = self.skip_parenthesized(idx);
        if let Some(sym_id) = self
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.binder.resolve_identifier(self.arena, idx))
        {
            return self.resolve_alias_symbol(sym_id, visited);
        }

        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_property_assignment(node)
        {
            return self.reference_symbol_inner(prop.initializer, visited);
        }

        if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.arena.get_shorthand_property(node)
        {
            return self.reference_symbol_inner(prop.name, visited);
        }
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.arena.get_variable_declaration(node)
        {
            return self.reference_symbol_inner(decl.name, visited);
        }

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.arena.get_function(node)
            && func.name.is_some()
        {
            return self.reference_symbol_inner(func.name, visited);
        }

        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class_decl) = self.arena.get_class(node)
            && class_decl.name.is_some()
        {
            return self.reference_symbol_inner(class_decl.name, visited);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(list) = self.arena.get_variable(node)
            && list.declarations.nodes.len() == 1
            && let Some(&decl_idx) = list.declarations.nodes.first()
        {
            return self.reference_symbol_inner(decl_idx, visited);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if self.is_assignment_operator(bin.operator_token) {
                return self.reference_symbol_inner(bin.left, visited);
            }
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            return self.resolve_namespace_member(qn.left, qn.right, visited);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return self.resolve_namespace_member(
                access.expression,
                access.name_or_argument,
                visited,
            );
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let name = self.literal_string_from_node(access.name_or_argument)?;
            return self.resolve_namespace_member_by_name(access.expression, name, visited);
        }

        None
    }

    pub(crate) fn resolve_namespace_member(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let right_name = self
            .arena
            .get(right)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;
        self.resolve_namespace_member_by_name(left, right_name, visited)
    }

    pub(crate) fn resolve_namespace_member_by_name(
        &self,
        left: NodeIndex,
        right_name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let left_sym = self.reference_symbol_inner(left, visited)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited)?;
        let left_symbol = self.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        let member_sym = exports.get(right_name)?;
        self.resolve_alias_symbol(member_sym, visited)
    }

    pub(crate) fn resolve_alias_symbol(
        &self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return Some(sym_id);
        }
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import = self.arena.get_import_decl(decl_node)?;
        self.reference_symbol_inner(import.module_specifier, visited)
    }
}
