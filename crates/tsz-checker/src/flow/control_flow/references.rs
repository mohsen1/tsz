//! Reference matching, literal parsing, and symbol resolution utilities
//! for control flow analysis.
//!
//! Extracted from `narrowing.rs` to keep modules focused.
//! Contains:
//! - Reference matching (`is_matching_reference`, `property_reference`)
//! - Literal value extraction from AST nodes (`literal_number_from_node`, `literal_atom_from`_*)
//! - Numeric parsing (`bigint_base_to_decimal`; numeric-literal parsing lives in `tsz_common::numeric`)
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
        if let Some(shared) = self.shared_reference_match_cache()
            && let Some(&cached) = shared.borrow().get(&key)
        {
            return cached;
        }
        if let Some(&cached) = self.reference_match_cache.borrow().get(&key) {
            return cached;
        }

        trace!(?a, ?b, "is_matching_reference called");

        let result = self.is_matching_reference_uncached(a, b);

        if let Some(shared) = self.shared_reference_match_cache() {
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
            // `import.meta` and `new.target` have no symbol backing; match two
            // `import` meta-property roots so narrowing can flow through
            // `import.meta.foo` chains. Two distinct ImportKeyword nodes within
            // the same file always refer to the same `import.meta`.
            if node_a.kind == SyntaxKind::ImportKeyword as u16
                && node_b.kind == SyntaxKind::ImportKeyword as u16
            {
                trace!("Matched: both are 'import' meta-property root");
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

    pub(crate) fn is_member_like_reference(&self, idx: NodeIndex) -> bool {
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

    /// Map a property/element reference *path* to a session-stable synthetic
    /// cache [`SymbolId`], so every syntactic occurrence of the same path shares
    /// flow-cache entries instead of re-walking the flow graph per occurrence.
    ///
    /// Returns `None` when the reference is not a stable narrowable path (e.g.
    /// `f().x`, or a base that does not resolve to a symbol); callers then fall
    /// back to the per-node synthetic key, preserving prior behavior.
    ///
    /// The structural key is `[base_symbol_id, prop_atom_0, prop_atom_1, ...]`:
    /// position 0 is always a base `SymbolId` and the rest are always property
    /// `Atom`s, so two distinct paths can never collide. The id is folded into a
    /// synthetic symbol via [`super::structural_flow_cache_symbol`], keyed
    /// disjointly from real and per-node symbols (see the symbol-space partition
    /// in `core.rs`).
    /// Decompose one member-access hop for the structural cache key, tolerating
    /// optional-chain `?.` (which `property_reference` rejects). Reports the
    /// receiver, the property/literal-index atom, and whether the hop was
    /// optional. Optionality is folded into the key so a mixed `o?.a` / `o.a`
    /// program keys them distinctly (they could narrow differently and must not
    /// share), while repeated reads of the *same* optional path still share and
    /// stay O(n). Entity-name element indices (`obj[key]`) return `None` here, as
    /// in `property_reference`, and fall back to the per-node walk.
    fn member_access_key_parts(&self, idx: NodeIndex) -> Option<(NodeIndex, Atom, bool)> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.member_access_key_parts(unary.expression);
        }
        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.member_access_key_parts(assertion.expression);
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let ident = self.arena.get_identifier_at(access.name_or_argument)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((access.expression, name, access.question_dot_token));
        }
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            let name = self.literal_atom_from_node_or_type(access.name_or_argument)?;
            return Some((access.expression, name, access.question_dot_token));
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            let ident = self.arena.get_identifier_at(qn.right)?;
            let name = self.interner.intern_string(&ident.escaped_text);
            return Some((qn.left, name, false));
        }
        None
    }

    pub(crate) fn flow_reference_path_symbol(&self, reference: NodeIndex) -> Option<SymbolId> {
        let interner = self.shared_flow_reference_keys()?;
        let mut key = Vec::new();
        if let Some((base, prop, optional)) = self.member_access_key_parts(reference) {
            // Property/element path (`a.b`, `this.x`, `o?.a`): walk base-first.
            self.collect_flow_reference_path(base, &mut key)?;
            key.push(prop.0);
            key.push(u32::from(optional));
        } else {
            // Bare `this` / `super`: no binder symbol and no property hop, but a
            // stable receiver within its flow scope. Without an occurrence-stable
            // key they fall back to a per-node symbol and re-walk the flow graph
            // per read (O(n^2) for `this`-heavy method bodies). Intern a base-only
            // key so repeated bare reads share flow-cache entries. Bare
            // identifiers already carry a resolved `SymbolId` and never reach here.
            let leaf = self.skip_parenthesized(reference);
            let node = self.arena.get(leaf)?;
            if node.kind == SyntaxKind::ThisKeyword as u16 {
                key.push(super::FLOW_CACHE_THIS_BASE_KEY);
            } else if node.kind == SyntaxKind::SuperKeyword as u16 {
                key.push(super::FLOW_CACHE_SUPER_BASE_KEY);
            } else {
                return None;
            }
        }
        let mut map = interner.borrow_mut();
        let next = map.len() as u32;
        let id = *map.entry(key).or_insert(next);
        if id >= super::FLOW_CACHE_STRUCTURAL_ID_LIMIT {
            // Structural-key space exhausted (pathologically many distinct paths);
            // fall back to the per-node key rather than risk aliasing.
            return None;
        }
        Some(super::structural_flow_cache_symbol(id))
    }

    /// Append the base-first structural key of `reference` to `out`.
    ///
    /// Recurses through property/element hops (each contributing a property atom
    /// plus an optional-chain flag), then resolves the leaf base to its real
    /// binder `SymbolId` (or `this`/`super` sentinel) at position 0.
    fn collect_flow_reference_path(&self, reference: NodeIndex, out: &mut Vec<u32>) -> Option<()> {
        if let Some((base, prop, optional)) = self.member_access_key_parts(reference) {
            self.collect_flow_reference_path(base, out)?;
            out.push(prop.0);
            out.push(u32::from(optional));
            return Some(());
        }
        // `this` / `super` carry no binder symbol but are valid narrowable bases
        // (`this.x`, `super.x`). Reserve disjoint base components so their paths
        // share flow-cache entries across occurrences within one container.
        let leaf = self.skip_parenthesized(reference);
        if let Some(node) = self.arena.get(leaf) {
            if node.kind == SyntaxKind::ThisKeyword as u16 {
                out.push(super::FLOW_CACHE_THIS_BASE_KEY);
                return Some(());
            }
            if node.kind == SyntaxKind::SuperKeyword as u16 {
                out.push(super::FLOW_CACHE_SUPER_BASE_KEY);
                return Some(());
            }
        }
        let sym = self.reference_symbol(reference)?;
        // The leaf must be a real binder symbol; refuse if a synthetic value ever
        // leaked in, rather than risk crossing key spaces.
        if !super::is_real_binder_symbol(sym) {
            return None;
        }
        out.push(sym.0);
        Some(())
    }

    /// True when `idx` is a narrowable member-access reference: a property or
    /// element access whose receiver chain bottoms out in an identifier, `this`,
    /// or `super`, where every element-access index is a string/number literal
    /// or an entity-name expression.
    ///
    /// Faithful mirror of tsc's `isNarrowableReference` (over `isDottedName`).
    /// Non-reference receivers (fresh object literals, call results) and
    /// non-narrowable element indices (`arr[i % 3]`, `arr[f()]`) return `false`:
    /// such accesses cannot match any control-flow antecedent, so a flow walk
    /// over them only ever returns the declared type and can be skipped. Note
    /// this is broader than the *structural-cache* path (`property_reference`,
    /// which keys only literal indices): an entity-name-indexed access like
    /// `obj[key]` is narrowable (tsc narrows it) even though it has no stable
    /// structural key, so it must not be skipped — it falls back to the per-node
    /// walk.
    pub(crate) fn is_narrowable_member_reference(&self, idx: NodeIndex) -> bool {
        let leaf = self.skip_parenthesized(idx);
        let Some(node) = self.arena.get(leaf) else {
            return false;
        };
        // Only property/element accesses reach the member-access skip; bare
        // identifiers, `this`, and `super` are handled by the identifier path.
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        self.reference_is_narrowable(leaf)
    }

    /// True when a member-access reference's receiver chain bottoms out at a
    /// `CallExpression` result (e.g. `readIndexed('p').a.b`). Such a reference
    /// has no narrowable storage root — no flow node can `is_matching_reference`-
    /// match it — so the backward flow walk provably returns the declared type
    /// unchanged. This is the *positive* counterpart used to gate the
    /// non-narrowable short-circuit: only call-rooted member paths are skipped,
    /// so `this`/`super`/identifier/meta-property roots (which are narrowable and
    /// participate in `typeof`-query and discriminant narrowing) always walk.
    pub(crate) fn reference_bottoms_at_call_result(&self, idx: NodeIndex) -> bool {
        let idx = self.skip_parenthesized(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            return true;
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            return self
                .arena
                .get_unary_expr_ex(node)
                .is_some_and(|u| self.reference_bottoms_at_call_result(u.expression));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return self
                .arena
                .get_access_expr(node)
                .is_some_and(|a| self.reference_bottoms_at_call_result(a.expression));
        }
        false
    }

    fn reference_is_narrowable(&self, idx: NodeIndex) -> bool {
        let idx = self.skip_parenthesized(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::ThisKeyword as u16
            || node.kind == SyntaxKind::SuperKeyword as u16
            || node.kind == syntax_kind_ext::QUALIFIED_NAME
            // `import.meta` / `new.target`: symbol-less meta-property roots that
            // `is_matching_reference` already treats as stable references, so
            // member accesses over them (`import.meta.env.FOO`) are narrowable.
            || node.kind == syntax_kind_ext::META_PROPERTY
            || node.kind == SyntaxKind::ImportKeyword as u16
        {
            return true;
        }
        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            return self
                .arena
                .get_unary_expr_ex(node)
                .is_some_and(|u| self.reference_is_narrowable(u.expression));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            return !access.question_dot_token && self.reference_is_narrowable(access.expression);
        }
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let Some(access) = self.arena.get_access_expr(node) else {
                return false;
            };
            return !access.question_dot_token
                && self.element_index_is_narrowable(access.name_or_argument)
                && self.reference_is_narrowable(access.expression);
        }
        false
    }

    /// An element-access index is narrowable when it is a string/number literal
    /// or an entity-name expression (`obj[key]`, `obj[ns.key]`), matching tsc.
    fn element_index_is_narrowable(&self, idx: NodeIndex) -> bool {
        if self.literal_atom_from_node_or_type(idx).is_some() {
            return true;
        }
        self.argument_is_entity_name(idx)
    }

    fn argument_is_entity_name(&self, idx: NodeIndex) -> bool {
        let idx = self.skip_parenthesized(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::ThisKeyword as u16
            || node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            return true;
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return self.arena.get_access_expr(node).is_some_and(|a| {
                !a.question_dot_token && self.argument_is_entity_name(a.expression)
            });
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
            let name = self.literal_atom_from_node_or_type(access.name_or_argument);
            tracing::trace!(
                ?idx,
                key = ?access.name_or_argument,
                resolved = ?name,
                key_type = ?self
                    .node_types
                    .and_then(|nt| nt.get(&access.name_or_argument.0).copied()),
                "element-access property_reference key resolution"
            );
            let name = name?;
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
        // Well-known symbol keys (e.g. `Symbol.iterator in x`): members keyed
        // by a built-in `Symbol.*` unique symbol are stored under the
        // "[Symbol.<name>]" member name — the same syntax-derived convention
        // computed property declarations use — not the generic
        // "__unique_<id>" name that `literal_property_name` derives. Without
        // this mapping, `in`-narrowing fails to match any union constituent
        // and the guard degrades to `T & Record<__unique_N, unknown>`.
        if let Some(atom) = self.well_known_symbol_member_atom(idx, type_id) {
            return Some((atom, false));
        }
        if let Some(atom) = crate::query_boundaries::type_computation::access::literal_property_name(
            self.interner,
            type_id,
        ) {
            return Some((atom, false));
        }
        match classify_for_literal_value(self.interner, type_id) {
            LiteralValueKind::String(atom) => Some((atom, false)),
            LiteralValueKind::Number(value) => Some((self.atom_from_numeric_value(value), true)),
            LiteralValueKind::None => None,
        }
    }

    /// Member-name atom for a well-known unique-symbol key written as
    /// `Symbol.<name>` (`Symbol.iterator`, `Symbol.asyncIterator`, …).
    ///
    /// Class/interface members keyed by these well-known symbols are stored
    /// under "[Symbol.<name>]" — the same syntax-derived convention used when
    /// the member declaration's computed property name is resolved (see
    /// `types/type_node_property_names.rs`). The key expression must actually
    /// carry a `unique symbol` type so a user-defined `Symbol` object with
    /// ordinary members does not alias the well-known names.
    fn well_known_symbol_member_atom(
        &self,
        idx: NodeIndex,
        type_id: tsz_solver::TypeId,
    ) -> Option<Atom> {
        crate::query_boundaries::common::unique_symbol_ref(
            self.interner.as_type_database(),
            type_id,
        )?;
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let base_node = self.arena.get(access.expression)?;
        let base_ident = self.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }
        let name_node = self.arena.get(access.name_or_argument)?;
        let ident = self.arena.get_identifier(name_node)?;
        let name = format!("[Symbol.{}]", ident.escaped_text);
        Some(self.interner.intern_string(&name))
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
        if let Some(atom) = crate::query_boundaries::type_computation::access::literal_property_name(
            self.interner,
            type_id,
        ) {
            return Some(atom);
        }
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
                lit.value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
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
                let value = lit
                    .value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))?;
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
        if let Some(shared) = self.shared_numeric_atom_cache()
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

        if let Some(shared) = self.shared_numeric_atom_cache()
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

    pub(crate) fn assignment_root_symbols_may_overlap(
        &self,
        assignment: NodeIndex,
        reference: NodeIndex,
        reference_symbol: Option<SymbolId>,
    ) -> bool {
        let Some(assignment_root) = self.reference_root_symbol(assignment) else {
            return true;
        };
        let Some(reference_root) =
            reference_symbol.or_else(|| self.reference_root_symbol(reference))
        else {
            return true;
        };
        assignment_root == reference_root
    }

    pub(crate) fn reference_root_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;

        if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            let unary = self.arena.get_unary_expr_ex(node)?;
            return self.reference_root_symbol(unary.expression);
        }

        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            || node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(node)?;
            return self.reference_root_symbol(assertion.expression);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if self.is_assignment_operator(bin.operator_token) {
                return self.reference_root_symbol(bin.left);
            }
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let access = self.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            return self.reference_root_symbol(access.expression);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.arena.get_qualified_name(node)?;
            return self.reference_root_symbol(qn.left);
        }

        self.reference_symbol(idx)
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
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return Some(sym_id);
        }
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            // For non-`import =` aliases (ImportSpecifier, ImportClause,
            // NamespaceImport), we can't resolve through to the original
            // export target. Return the alias symbol itself so that
            // is_matching_reference can still match two references to the
            // same imported binding (e.g. `if (a0) x = a0` narrows `a0`).
            return Some(sym_id);
        }
        let import = self.arena.get_import_decl(decl_node)?;
        self.reference_symbol_inner(import.module_specifier, visited)
    }
}
