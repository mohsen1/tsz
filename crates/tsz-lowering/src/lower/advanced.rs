//! Advanced type lowering: conditionals, mapped types, indexed access,
//! literal parsing, type references, and remaining simple type forms.

use super::core::*;

use tsz_parser::parser::base::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::types::{
    ConditionalType, MappedModifier, MappedType, ParamInfo, SymbolRef, TemplateSpan, TypeId,
    TypeParamInfo, TypePredicate, TypePredicateTarget,
};

impl<'a> TypeLowering<'a> {
    /// Lower a conditional type (T extends U ? X : Y)
    pub(super) fn lower_conditional_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_conditional_type(node) {
            let is_distributive = self.is_naked_type_param(data.check_type);
            let check_type = self.lower_type(data.check_type);
            let extends_type = self.lower_type(data.extends_type);

            self.push_type_param_scope();
            for (name, type_id) in tsz_solver::collect_infer_bindings(self.interner, extends_type) {
                self.add_type_param_binding(name, type_id);
            }
            let true_type = self.lower_type(data.true_type);
            let false_type = self.lower_type(data.false_type);
            self.pop_type_param_scope();

            let cond = ConditionalType {
                check_type,
                extends_type,
                true_type,
                false_type,
                is_distributive,
            };
            self.interner.conditional(cond)
        } else {
            TypeId::ERROR
        }
    }

    pub(super) fn is_naked_type_param(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > tsz_common::limits::MAX_TREE_WALK_ITERATIONS {
                // Safety limit reached - return false to prevent infinite loop
                return false;
            }
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                    if let Some(data) = self.arena.get_wrapped_type(node) {
                        current = data.type_node;
                        continue;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    let Some(data) = self.arena.get_type_ref(node) else {
                        return false;
                    };
                    if let Some(args) = &data.type_arguments
                        && !args.nodes.is_empty()
                    {
                        return false;
                    }
                    let Some(name_node) = self.arena.get(data.type_name) else {
                        return false;
                    };
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        return self.lookup_type_param(&ident.escaped_text).is_some();
                    }
                    return false;
                }
                k if k == SyntaxKind::Identifier as u16 => {
                    let Some(ident) = self.arena.get_identifier(node) else {
                        return false;
                    };
                    return self.lookup_type_param(&ident.escaped_text).is_some();
                }
                _ => return false,
            }
        }
    }

    /// Lower a mapped type ({ [K in Keys]: `ValueType` })
    pub(super) fn lower_mapped_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_mapped_type(node) {
            let (type_param, constraint) = self.lower_mapped_type_param(data.type_parameter);
            self.push_type_param_scope();
            let type_param_id = self.interner.type_param(type_param);
            self.add_type_param_binding(type_param.name, type_param_id);
            let name_type =
                (data.name_type != NodeIndex::NONE).then(|| self.lower_type(data.name_type));
            let template = self.lower_type(data.type_node);
            self.pop_type_param_scope();
            let mapped = MappedType {
                type_param,
                constraint,
                name_type,
                template,
                readonly_modifier: self
                    .lower_mapped_modifier(data.readonly_token, SyntaxKind::ReadonlyKeyword as u16),
                optional_modifier: self
                    .lower_mapped_modifier(data.question_token, SyntaxKind::QuestionToken as u16),
            };
            self.interner.mapped(mapped)
        } else {
            TypeId::ERROR
        }
    }

    pub(super) fn lower_mapped_type_param(&self, node_idx: NodeIndex) -> (TypeParamInfo, TypeId) {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => {
                let name = self.interner.intern_string("K");
                return (
                    TypeParamInfo {
                        is_const: false,
                        variance: tsz_solver::TypeParamVariance::None,
                        name,
                        constraint: None,
                        default: None,
                    },
                    TypeId::ERROR, // Missing node - propagate error
                );
            }
        };

        if let Some(param_data) = self.arena.get_type_parameter(node) {
            let name = self
                .arena
                .get(param_data.name)
                .and_then(|ident_node| self.arena.get_identifier(ident_node))
                .map_or_else(
                    || self.interner.intern_string("K"),
                    |ident| self.interner.intern_string(&ident.escaped_text),
                );

            let constraint = (param_data.constraint != NodeIndex::NONE)
                .then(|| self.lower_type(param_data.constraint));

            let default = (param_data.default != NodeIndex::NONE)
                .then(|| self.lower_type(param_data.default));

            // Use Unknown instead of Any for stricter type checking
            // When a generic parameter has no constraint, use Unknown to prevent
            // invalid values from being accepted
            let constraint_type = constraint.unwrap_or(TypeId::UNKNOWN);

            (
                TypeParamInfo {
                    is_const: false,
                    variance: tsz_solver::TypeParamVariance::None,
                    name,
                    constraint,
                    default,
                },
                constraint_type,
            )
        } else {
            let name = self.interner.intern_string("K");
            (
                TypeParamInfo {
                    is_const: false,
                    variance: tsz_solver::TypeParamVariance::None,
                    name,
                    constraint: None,
                    default: None,
                },
                TypeId::ERROR, // Missing type parameter data - propagate error
            )
        }
    }

    pub(super) fn lower_mapped_modifier(
        &self,
        token_idx: NodeIndex,
        default_kind: u16,
    ) -> Option<MappedModifier> {
        use tsz_scanner::SyntaxKind;

        if token_idx == NodeIndex::NONE {
            return None;
        }

        let kind = self.arena.get(token_idx).map(|node| node.kind)?;
        if kind == SyntaxKind::PlusToken as u16 || kind == default_kind {
            Some(MappedModifier::Add)
        } else if kind == SyntaxKind::MinusToken as u16 {
            Some(MappedModifier::Remove)
        } else {
            None
        }
    }

    /// Lower an indexed access type (T[K])
    pub(super) fn lower_indexed_access_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_indexed_access_type(node) {
            let object_type = self.lower_type(data.object_type);
            let index_type = self.lower_type(data.index_type);
            self.interner.index_access(object_type, index_type)
        } else {
            TypeId::ERROR
        }
    }

    pub(super) fn strip_numeric_separators<'b>(text: &'b str) -> std::borrow::Cow<'b, str> {
        if !text.as_bytes().contains(&b'_') {
            return std::borrow::Cow::Borrowed(text);
        }

        let mut out = String::with_capacity(text.len());
        for &byte in text.as_bytes() {
            if byte != b'_' {
                out.push(byte as char);
            }
        }
        std::borrow::Cow::Owned(out)
    }

    pub(super) fn normalize_bigint_literal<'b>(
        &self,
        text: &'b str,
    ) -> Option<std::borrow::Cow<'b, str>> {
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            return Self::bigint_base_to_decimal(rest, 16).map(std::borrow::Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            return Self::bigint_base_to_decimal(rest, 2).map(std::borrow::Cow::Owned);
        }
        if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            return Self::bigint_base_to_decimal(rest, 8).map(std::borrow::Cow::Owned);
        }

        match Self::strip_numeric_separators(text) {
            std::borrow::Cow::Borrowed(cleaned) => {
                let trimmed = cleaned.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(std::borrow::Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned.len() {
                    return Some(std::borrow::Cow::Borrowed(cleaned));
                }
                Some(std::borrow::Cow::Borrowed(trimmed))
            }
            std::borrow::Cow::Owned(mut cleaned) => {
                let cleaned_ref = cleaned.as_str();
                let trimmed = cleaned_ref.trim_start_matches('0');
                if trimmed.is_empty() {
                    return Some(std::borrow::Cow::Borrowed("0"));
                }
                if trimmed.len() == cleaned_ref.len() {
                    return Some(std::borrow::Cow::Owned(cleaned));
                }

                let trim_len = cleaned_ref.len() - trimmed.len();
                cleaned.drain(..trim_len);
                Some(std::borrow::Cow::Owned(cleaned))
            }
        }
    }

    pub(super) fn bigint_base_to_decimal(text: &str, base: u32) -> Option<String> {
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

        while digits.len() > 1 && matches!(digits.last(), Some(&0)) {
            digits.pop();
        }

        let mut out = String::with_capacity(digits.len());
        for digit in digits.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        Some(out)
    }

    /// Lower a literal type ("foo", 42, etc.)
    pub(super) fn lower_literal_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_literal_type(node) {
            // The literal node contains the actual literal value
            if let Some(literal_node) = self.arena.get(data.literal) {
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            self.interner.literal_string(&lit_data.text)
                        } else {
                            TypeId::STRING
                        }
                    }
                    k if k == SyntaxKind::NumericLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            if let Some(value) = lit_data.value.or_else(|| {
                                tsz_common::numeric::parse_numeric_literal_value(&lit_data.text)
                            }) {
                                self.interner.literal_number(value)
                            } else {
                                TypeId::NUMBER
                            }
                        } else {
                            TypeId::NUMBER
                        }
                    }
                    k if k == SyntaxKind::BigIntLiteral as u16 => {
                        if let Some(lit_data) = self.arena.get_literal(literal_node) {
                            let text = lit_data.text.strip_suffix('n').unwrap_or(&lit_data.text);
                            if let Some(normalized) = self.normalize_bigint_literal(text) {
                                self.interner.literal_bigint(normalized.as_ref())
                            } else {
                                TypeId::BIGINT
                            }
                        } else {
                            TypeId::BIGINT
                        }
                    }
                    k if k == SyntaxKind::TrueKeyword as u16 => self.interner.literal_boolean(true),
                    k if k == SyntaxKind::FalseKeyword as u16 => {
                        self.interner.literal_boolean(false)
                    }
                    k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                        if let Some(unary) = self.arena.get_unary_expr(literal_node) {
                            let op = unary.operator;
                            let Some(operand_node) = self.arena.get(unary.operand) else {
                                return TypeId::ERROR; // Propagate error for missing operand
                            };
                            match operand_node.kind {
                                k if k == SyntaxKind::NumericLiteral as u16 => {
                                    if let Some(lit_data) = self.arena.get_literal(operand_node) {
                                        if let Some(value) = lit_data.value.or_else(|| {
                                            tsz_common::numeric::parse_numeric_literal_value(
                                                &lit_data.text,
                                            )
                                        }) {
                                            let value = if op == SyntaxKind::MinusToken as u16 {
                                                -value
                                            } else {
                                                value
                                            };
                                            self.interner.literal_number(value)
                                        } else {
                                            TypeId::NUMBER
                                        }
                                    } else {
                                        TypeId::NUMBER
                                    }
                                }
                                k if k == SyntaxKind::BigIntLiteral as u16 => {
                                    if let Some(lit_data) = self.arena.get_literal(operand_node) {
                                        let text = lit_data
                                            .text
                                            .strip_suffix('n')
                                            .unwrap_or(&lit_data.text);
                                        let negative = op == SyntaxKind::MinusToken as u16;
                                        if let Some(normalized) =
                                            self.normalize_bigint_literal(text)
                                        {
                                            self.interner.literal_bigint_with_sign(
                                                negative,
                                                normalized.as_ref(),
                                            )
                                        } else {
                                            TypeId::BIGINT
                                        }
                                    } else {
                                        TypeId::BIGINT
                                    }
                                }
                                _ => TypeId::ERROR, // Propagate error for unknown operand kind
                            }
                        } else {
                            TypeId::ERROR // Propagate error for missing unary expression data
                        }
                    }
                    _ => TypeId::ERROR, // Propagate error for unknown literal kind
                }
            } else {
                TypeId::ERROR // Propagate error for missing literal node
            }
        } else {
            TypeId::ERROR // Propagate error for missing literal type data
        }
    }

    /// Lower a type reference (`NamedType` or `NamedType`<Args>)
    pub(super) fn lower_type_reference(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_ref(node) {
            if let Some(name_node) = self.arena.get(data.type_name)
                && let Some(ident) = self.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();

                // Handle string manipulation intrinsic types.
                // String intrinsics (Uppercase, Lowercase, Capitalize, Uncapitalize)
                // are always compiler intrinsics regardless of symbol resolution,
                // so check them first (but after type param lookup).
                if self.lookup_type_param(name).is_none() {
                    match name {
                        "Uppercase" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.string_intrinsic(
                                    tsz_solver::types::StringIntrinsicKind::Uppercase,
                                    type_arg,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        "Lowercase" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.string_intrinsic(
                                    tsz_solver::types::StringIntrinsicKind::Lowercase,
                                    type_arg,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        "Capitalize" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.string_intrinsic(
                                    tsz_solver::types::StringIntrinsicKind::Capitalize,
                                    type_arg,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        "Uncapitalize" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg = self.lower_type(first_arg);
                                return self.interner.string_intrinsic(
                                    tsz_solver::types::StringIntrinsicKind::Uncapitalize,
                                    type_arg,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        "NoInfer" => {
                            if let Some(args) = &data.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let inner = self.lower_type(first_arg);
                                return self.interner.no_infer(inner);
                            }
                            return TypeId::ERROR;
                        }
                        _ => {}
                    }
                }

                // Handle built-in generic types that need special lowering
                // Only when the name doesn't shadow a local type parameter and
                // doesn't resolve to a user-defined type symbol.
                if self.lookup_type_param(name).is_none()
                    && self.resolve_type_symbol(data.type_name).is_none()
                {
                    match name {
                        "Array" | "ReadonlyArray" => {
                            let elem_type = data
                                .type_arguments
                                .as_ref()
                                .and_then(|args| args.nodes.first().copied())
                                .map_or(TypeId::UNKNOWN, |idx| self.lower_type(idx));
                            let array_type = self.interner.array(elem_type);
                            if name == "ReadonlyArray" {
                                return self.interner.readonly_type(array_type);
                            }
                            return array_type;
                        }
                        _ => {}
                    }
                }
            }

            // For now, just lower the type name as an identifier
            let base_type = self.lower_type(data.type_name);
            if let Some(args) = &data.type_arguments
                && !args.nodes.is_empty()
            {
                let mut type_args: Vec<TypeId> =
                    args.nodes.iter().map(|&idx| self.lower_type(idx)).collect();
                let type_symbol = self.resolve_type_symbol(data.type_name);
                let value_symbol = self.resolve_value_symbol(data.type_name);
                let base_type = if type_symbol.is_some() && base_type != TypeId::ERROR {
                    base_type
                } else if base_type == TypeId::ERROR {
                    value_symbol
                        .map(|symbol_id| self.interner.type_query(SymbolRef(symbol_id)))
                        .unwrap_or(base_type)
                } else {
                    base_type
                };
                // Fill in missing type arguments from defaults (tsc's
                // fillMissingTypeArguments). When `Effect<void>` is written but
                // `Effect` has 3 type params with defaults for the last 2, the
                // Application should be `Application(Effect, [void, never, never])`.
                if let Some(tsz_solver::TypeData::Lazy(def_id)) = self.interner.lookup(base_type)
                    && let Some(resolve_params) = self.lazy_type_params_resolver
                    && let Some(type_params) = resolve_params(def_id)
                    && type_args.len() < type_params.len()
                    && type_params[type_args.len()..]
                        .iter()
                        .all(|p| p.default.is_some())
                {
                    for param in &type_params[type_args.len()..] {
                        type_args.push(param.default.unwrap());
                    }
                }
                return self.interner.application(base_type, type_args);
            }

            if let Some(tsz_solver::TypeData::Lazy(def_id)) = self.interner.lookup(base_type)
                && let Some(resolve_params) = self.lazy_type_params_resolver
                && let Some(type_params) = resolve_params(def_id)
                && !type_params.is_empty()
                && type_params.iter().all(|param| param.default.is_some())
            {
                let default_args = type_params
                    .into_iter()
                    .map(|param| param.default.unwrap_or(TypeId::ERROR))
                    .collect();
                return self.interner.application(base_type, default_args);
            }

            base_type
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a qualified name type (A.B).
    pub(super) fn lower_qualified_name_type(&self, node_idx: NodeIndex) -> TypeId {
        // Same-arena lowering (the common case for user source files) must prefer
        // the NodeIndex-based DefId resolver because `N.X` should bind to the exact
        // namespace member, not to a globally-named lib symbol that happens to share
        // the right-hand identifier (e.g., a user-declared `namespace N { class Promise {} }`
        // must not collide with the lib's global `Promise`).
        //
        // Cross-arena lib lowering opts into name-first resolution because raw
        // NodeIndex values are arena-local and cannot be resolved across arenas.
        if self.prefer_name_def_id_resolution {
            if let Some(name) = self.type_name_text(node_idx)
                && let Some(def_id) = self.resolve_def_id_by_name(&name)
            {
                return self.interner.lazy(def_id);
            }
            if let Some(def_id) = self.resolve_def_id(node_idx) {
                return self.interner.lazy(def_id);
            }
        } else {
            if let Some(def_id) = self.resolve_def_id(node_idx) {
                return self.interner.lazy(def_id);
            }
            if let Some(name) = self.type_name_text(node_idx)
                && let Some(def_id) = self.resolve_def_id_by_name(&name)
            {
                return self.interner.lazy(def_id);
            }
        }
        if let Some(name) = self.type_name_text(node_idx) {
            return self
                .interner
                .unresolved_type_name(self.interner.intern_string(&name));
        }
        TypeId::ERROR
    }

    /// Lower an identifier as a type (simple type reference)
    pub(super) fn lower_identifier_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_identifier(node) {
            let name = &data.escaped_text;

            if let Some(type_param) = self.lookup_type_param(name) {
                return type_param;
            }

            // Check for built-in type names FIRST before attempting symbol resolution.
            // This ensures that primitive type keywords like "symbol", "string", "number"
            // always resolve to their primitive types (TypeId::SYMBOL, TypeId::STRING, etc.)
            // and are never shadowed by user-defined or lib-defined symbols.
            //
            // Primitive type keywords must be resolved before symbol lookup so
            // they are never shadowed by user-defined or lib-defined symbols.
            match name.as_ref() {
                "any" => return TypeId::ANY,
                "unknown" => return TypeId::UNKNOWN,
                "never" => return TypeId::NEVER,
                "void" => return TypeId::VOID,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "boolean" => return TypeId::BOOLEAN,
                "number" => return TypeId::NUMBER,
                "string" => return TypeId::STRING,
                "bigint" => return TypeId::BIGINT,
                "symbol" => return TypeId::SYMBOL,
                "object" => return TypeId::OBJECT,
                _ => {}
            }

            if self.preferred_self_name.as_deref() == Some(name.as_ref())
                && let Some(def_id) = self.preferred_self_def_id
            {
                return self.interner.lazy(def_id);
            }

            // Must resolve to DefId.
            //
            // Same-arena lowering should prefer the NodeIndex-based path because it
            // preserves the exact bound symbol, including namespace-local bindings.
            // Cross-arena lowering can opt into name-first resolution because raw
            // NodeIndex values are arena-local and can collide across declarations.
            if self.prefer_name_def_id_resolution {
                if let Some(scoped_name) = self.scoped_identifier_name_text(node_idx)
                    && let Some(def_id) = self.resolve_def_id_by_name(&scoped_name)
                {
                    return self.interner.lazy(def_id);
                }
                if let Some(def_id) = self.resolve_def_id_by_name(name) {
                    return self.interner.lazy(def_id);
                }
                if let Some(def_id) = self.resolve_def_id(node_idx) {
                    return self.interner.lazy(def_id);
                }
            } else {
                if let Some(def_id) = self.resolve_def_id(node_idx) {
                    return self.interner.lazy(def_id);
                }
                if let Some(scoped_name) = self.scoped_identifier_name_text(node_idx)
                    && let Some(def_id) = self.resolve_def_id_by_name(&scoped_name)
                {
                    return self.interner.lazy(def_id);
                }
                if let Some(def_id) = self.resolve_def_id_by_name(name) {
                    return self.interner.lazy(def_id);
                }
            }

            // Fallback: preserve the original syntactic name as `UnresolvedTypeName`
            // so diagnostics print the user-written name (e.g. `ItemSet`) instead of
            // the bare `error` token.  The checker emits TS2304 separately for the
            // missing definition; this only affects display in subsequent
            // structural-mismatch diagnostics like TS2345 / TS2322.
            //
            // Mirrors the qualified-name path in `lower_qualified_name_type`.
            self.interner
                .unresolved_type_name(self.interner.intern_string(name))
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a parenthesized type
    pub(super) fn lower_parenthesized_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        // Parenthesized types just wrap another type
        if let Some(data) = self.arena.get_wrapped_type(node) {
            self.lower_type(data.type_node)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a type query (typeof expr in type position)
    pub(super) fn lower_type_query(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_query(node) {
            // Check for a pre-resolved type from the checker (e.g., flow-narrowed typeof).
            // This allows `typeof c` inside a type alias body to pick up the narrowed
            // type of `c` when control flow has narrowed it at the declaration site.
            if let Some(override_fn) = &self.type_query_override
                && let Some(resolved) = override_fn(data.expr_name)
            {
                return resolved;
            }
            // Create a symbol reference from the expression name
            if let Some(symbol_id) = self.resolve_value_symbol(data.expr_name) {
                let base = self.interner.type_query(SymbolRef(symbol_id));
                if let Some(args) = &data.type_arguments
                    && !args.nodes.is_empty()
                {
                    let type_args: Vec<TypeId> =
                        args.nodes.iter().map(|&idx| self.lower_type(idx)).collect();
                    return self.interner.application(base, type_args);
                }
                return base;
            }
            // Handle global intrinsics that don't have binder symbols
            // (e.g., `typeof undefined`, `typeof NaN`, `typeof Infinity`, `typeof globalThis`).
            // The checker has a matching path in get_type_from_type_query; this ensures the
            // lowering pass doesn't cache ERROR for these well-known identifiers.
            if let Some(expr_node) = self.arena.get(data.expr_name)
                && let Some(ident) = self.arena.get_identifier(expr_node)
            {
                match ident.escaped_text.as_str() {
                    "undefined" => return TypeId::UNDEFINED,
                    "NaN" | "Infinity" => return TypeId::NUMBER,
                    // typeof globalThis — we don't have a synthetic globalThis
                    // object type, so use `any` to avoid spurious TS2536
                    // "cannot index type 'unknown'" for every indexed access.
                    // A follow-up pass in type_node_advanced validates specific
                    // indexed access patterns (block-scoped let/const keys).
                    "globalThis" => return TypeId::ANY,
                    _ => {}
                }
            }
            TypeId::ERROR
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a type operator (keyof, readonly, unique)
    pub(super) fn lower_type_operator(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_type_operator(node) {
            let inner_type = self.lower_type(data.type_node);

            // Check which operator it is
            match data.operator {
                // KeyOfKeyword = 143
                143 => self.interner.keyof(inner_type),
                // ReadonlyKeyword = 148
                148 => self.interner.readonly_type(inner_type),
                // UniqueKeyword = 158 - unique symbol
                158 => {
                    // unique symbol creates a unique symbol type
                    // Use node index as unique identifier
                    self.interner.unique_symbol(SymbolRef(node_idx.0))
                }
                _ => inner_type,
            }
        } else {
            TypeId::ERROR
        }
    }

    pub(super) fn lower_type_predicate(&self, node_idx: NodeIndex) -> TypeId {
        self.lower_type_predicate_return(node_idx, &[]).0
    }

    pub(super) fn lower_type_predicate_target(
        &self,
        node_idx: NodeIndex,
    ) -> Option<TypePredicateTarget> {
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::ThisKeyword as u16 || node.kind == syntax_kind_ext::THIS_TYPE {
            return Some(TypePredicateTarget::This);
        }

        self.arena.get_identifier(node).map(|ident| {
            TypePredicateTarget::Identifier(self.interner.intern_string(&ident.escaped_text))
        })
    }

    pub(super) fn lower_type_predicate_return(
        &self,
        node_idx: NodeIndex,
        params: &[ParamInfo],
    ) -> (TypeId, Option<TypePredicate>) {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return (TypeId::ERROR, None),
        };

        let Some(data) = self.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.lower_type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = (data.type_node != NodeIndex::NONE).then(|| self.lower_type(data.type_node));

        let mut parameter_index = None;
        if let TypePredicateTarget::Identifier(name) = &target {
            parameter_index = params.iter().position(|p| p.name == Some(*name));
        }

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
            parameter_index,
        };

        (return_type, Some(predicate))
    }

    /// Lower an infer type (infer R)
    pub(super) fn lower_infer_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_infer_type(node) {
            if let Some(info) = self.lower_type_parameter(data.type_parameter) {
                return self.interner.infer(info);
            }

            // Fallback: synthesize a name if the node isn't a type parameter.
            let name = if let Some(tp_node) = self.arena.get(data.type_parameter) {
                if let Some(id_data) = self.arena.get_identifier(tp_node) {
                    self.interner.intern_string(&id_data.escaped_text)
                } else {
                    self.interner.intern_string("infer")
                }
            } else {
                self.interner.intern_string("infer")
            };

            self.interner.infer(TypeParamInfo {
                is_const: false,
                variance: tsz_solver::TypeParamVariance::None,
                name,
                constraint: None,
                default: None,
            })
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a template literal type (`hello${T}world`)
    pub(super) fn lower_template_literal_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_template_literal_type(node) {
            let mut spans: Vec<TemplateSpan> = Vec::new();

            // Add the head text if present.
            if let Some(head_node) = self.arena.get(data.head)
                && let Some(head_lit) = self.arena.get_literal(head_node)
                && !head_lit.text.is_empty()
            {
                spans.push(TemplateSpan::Text(
                    self.interner.intern_string(&head_lit.text),
                ));
            }

            // Add template spans (type + text pairs)
            for &span_idx in &data.template_spans.nodes {
                if let Some(span_node) = self.arena.get(span_idx)
                    && span_node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN
                    && let Some(span_data) =
                        self.arena.template_spans.get(span_node.data_index as usize)
                {
                    let type_id = self.lower_type(span_data.expression);
                    spans.push(TemplateSpan::Type(type_id));

                    if let Some(lit_node) = self.arena.get(span_data.literal)
                        && let Some(lit_data) = self.arena.get_literal(lit_node)
                        && !lit_data.text.is_empty()
                    {
                        spans.push(TemplateSpan::Text(
                            self.interner.intern_string(&lit_data.text),
                        ));
                    }
                }
            }

            self.interner.template_literal(spans)
        } else {
            TypeId::STRING // Fallback to string
        }
    }

    /// Lower a named tuple member ([name: T])
    pub(super) fn lower_named_tuple_member(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_named_tuple_member(node) {
            // Lower the type part
            self.lower_type(data.type_node)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a constructor type (new () => T)
    pub(super) fn lower_constructor_type(&self, node_idx: NodeIndex) -> TypeId {
        use tsz_solver::CallSignature;

        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        // Constructor types create a Callable with construct_signatures
        if let Some(data) = self.arena.get_function_type(node) {
            let (type_params, (params, this_type, return_type, type_predicate)) = self
                .with_type_params(&data.type_parameters, || {
                    let (params, this_type) = self.lower_params_with_this(&data.parameters);

                    let (return_type, type_predicate) =
                        self.lower_return_type(data.type_annotation, &params);
                    (params, this_type, return_type, type_predicate)
                });

            // Create a construct signature instead of a function shape
            let construct_sig = CallSignature {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_method: false,
            };

            // Create a Callable shape with construct_signatures
            let shape = tsz_solver::CallableShape {
                call_signatures: vec![],
                construct_signatures: vec![construct_sig],
                properties: vec![],
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: data.is_abstract,
            };

            self.interner.callable(shape)
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a wrapped type (optional or rest type)
    pub(super) fn lower_wrapped_type(&self, node_idx: NodeIndex) -> TypeId {
        let node = match self.arena.get(node_idx) {
            Some(n) => n,
            None => return TypeId::ERROR,
        };

        if let Some(data) = self.arena.get_wrapped_type(node) {
            return self.lower_type(data.type_node);
        }

        if let Some(data) = self.arena.type_operators.get(node.data_index as usize) {
            return self.lower_type(data.type_node);
        }

        TypeId::ERROR
    }
}

#[cfg(test)]
mod numeric_helper_tests {
    //! Unit tests for the pure numeric/bigint normalization helpers used by
    //! `lower_literal_type`. Existing end-to-end tests in `tests/lower_tests.rs`
    //! exercise the common paths through real AST parsing; these tests focus
    //! on edge cases (empty input, separators, leading zeros, invalid digits,
    //! arbitrarily large bigints) that are awkward to set up via parser tests.
    use super::*;
    use std::borrow::Cow;
    use tsz_parser::parser::NodeArena;
    use tsz_solver::TypeInterner;

    // ---- strip_numeric_separators ----

    #[test]
    fn strip_separators_returns_borrowed_when_no_underscores() {
        let result = TypeLowering::strip_numeric_separators("123");
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "123");
    }

    #[test]
    fn strip_separators_empty_string_is_borrowed() {
        let result = TypeLowering::strip_numeric_separators("");
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "");
    }

    #[test]
    fn strip_separators_removes_single_underscore() {
        let result = TypeLowering::strip_numeric_separators("1_000");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "1000");
    }

    #[test]
    fn strip_separators_removes_multiple_underscores() {
        let result = TypeLowering::strip_numeric_separators("1_000_000");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "1000000");
    }

    #[test]
    fn strip_separators_handles_leading_underscore() {
        // The helper just removes underscores, validity is the parser's concern.
        let result = TypeLowering::strip_numeric_separators("_123");
        assert_eq!(result, "123");
    }

    #[test]
    fn strip_separators_handles_trailing_underscore() {
        let result = TypeLowering::strip_numeric_separators("123_");
        assert_eq!(result, "123");
    }

    #[test]
    fn strip_separators_handles_only_underscores() {
        let result = TypeLowering::strip_numeric_separators("___");
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "");
    }

    #[test]
    fn strip_separators_handles_hex_digits() {
        // The helper is base-agnostic — it preserves all non-underscore bytes.
        let result = TypeLowering::strip_numeric_separators("F_F_AB");
        assert_eq!(result, "FFAB");
    }

    // ---- bigint_base_to_decimal ----

    #[test]
    fn bigint_base_empty_returns_none() {
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 16), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 2), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("", 8), None);
    }

    #[test]
    fn bigint_base_only_separators_returns_none() {
        // No actual digits seen — saw_digit stays false → None.
        assert_eq!(TypeLowering::bigint_base_to_decimal("_", 16), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("__", 10), None);
    }

    #[test]
    fn bigint_base_zero_returns_zero() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 16).as_deref(),
            Some("0"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 2).as_deref(),
            Some("0"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0", 10).as_deref(),
            Some("0"),
        );
    }

    #[test]
    fn bigint_base_hex_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("FF", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("ff", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("100", 16).as_deref(),
            Some("256"),
        );
    }

    #[test]
    fn bigint_base_binary_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("1010", 2).as_deref(),
            Some("10"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("11111111", 2).as_deref(),
            Some("255"),
        );
    }

    #[test]
    fn bigint_base_octal_basic_values() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("77", 8).as_deref(),
            Some("63"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("10", 8).as_deref(),
            Some("8"),
        );
    }

    #[test]
    fn bigint_base_strips_leading_zeros() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("00FF", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0001010", 2).as_deref(),
            Some("10"),
        );
    }

    #[test]
    fn bigint_base_accepts_underscore_separators() {
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("F_F", 16).as_deref(),
            Some("255"),
        );
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("1010_1010", 2).as_deref(),
            Some("170"),
        );
    }

    #[test]
    fn bigint_base_rejects_invalid_digit_for_base() {
        // 8 is not a valid octal digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("8", 8), None);
        // 2 is not a valid binary digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("2", 2), None);
        // G is not a valid hex digit.
        assert_eq!(TypeLowering::bigint_base_to_decimal("G", 16), None);
    }

    #[test]
    fn bigint_base_rejects_non_digit_byte() {
        // Non-alphanumeric bytes (other than '_') are rejected outright.
        assert_eq!(TypeLowering::bigint_base_to_decimal("1.5", 10), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("1+1", 10), None);
        assert_eq!(TypeLowering::bigint_base_to_decimal("a!", 16), None);
    }

    #[test]
    fn bigint_base_handles_max_u64_in_hex() {
        // u64::MAX = 18446744073709551615; this must not lose precision.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("FFFFFFFFFFFFFFFF", 16).as_deref(),
            Some("18446744073709551615"),
        );
    }

    #[test]
    fn bigint_base_handles_value_beyond_u64() {
        // 2^64 = 18446744073709551616 — beyond u64::MAX, still must be exact.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("10000000000000000", 16).as_deref(),
            Some("18446744073709551616"),
        );
        // 2^128 — well past u64::MAX.
        let two_to_128 = "100000000000000000000000000000000";
        assert_eq!(
            TypeLowering::bigint_base_to_decimal(two_to_128, 16).as_deref(),
            Some("340282366920938463463374607431768211456"),
        );
    }

    #[test]
    fn bigint_base_decimal_uses_base_10() {
        // Base 10 with leading zero is also handled.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("0123", 10).as_deref(),
            Some("123"),
        );
        // 9 is valid in base 10 but not in base 8.
        assert_eq!(
            TypeLowering::bigint_base_to_decimal("9", 10).as_deref(),
            Some("9"),
        );
        assert_eq!(TypeLowering::bigint_base_to_decimal("9", 8), None);
    }

    // ---- normalize_bigint_literal ----

    fn make_lowering<'a>(arena: &'a NodeArena, interner: &'a TypeInterner) -> TypeLowering<'a> {
        TypeLowering::new(arena, interner)
    }

    #[test]
    fn normalize_bigint_decimal_no_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("1234");
        assert!(matches!(result, Some(Cow::Borrowed("1234"))));
    }

    #[test]
    fn normalize_bigint_decimal_with_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("1_000_000");
        // Underscores cause owned allocation, then no leading zeros to trim.
        assert!(matches!(result.as_deref(), Some("1000000")));
    }

    #[test]
    fn normalize_bigint_zero_decimal() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // Several variants of "zero" all normalize to "0".
        assert_eq!(lowering.normalize_bigint_literal("0").as_deref(), Some("0"));
        assert_eq!(
            lowering.normalize_bigint_literal("000").as_deref(),
            Some("0"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0_0_0").as_deref(),
            Some("0"),
        );
    }

    #[test]
    fn normalize_bigint_strips_leading_zeros_decimal() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0001").as_deref(),
            Some("1"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0_001").as_deref(),
            Some("1"),
        );
    }

    #[test]
    fn normalize_bigint_hex_lowercase_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0xFF").as_deref(),
            Some("255"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0xff").as_deref(),
            Some("255"),
        );
    }

    #[test]
    fn normalize_bigint_hex_uppercase_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0XFF").as_deref(),
            Some("255"),
        );
    }

    #[test]
    fn normalize_bigint_binary_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0b1010").as_deref(),
            Some("10"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0B1010").as_deref(),
            Some("10"),
        );
    }

    #[test]
    fn normalize_bigint_octal_prefix() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0o77").as_deref(),
            Some("63"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0O77").as_deref(),
            Some("63"),
        );
    }

    #[test]
    fn normalize_bigint_prefixed_with_separators() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert_eq!(
            lowering.normalize_bigint_literal("0xFF_FF").as_deref(),
            Some("65535"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0b1010_1010").as_deref(),
            Some("170"),
        );
        assert_eq!(
            lowering.normalize_bigint_literal("0o7_7").as_deref(),
            Some("63"),
        );
    }

    #[test]
    fn normalize_bigint_empty_after_prefix_returns_none() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        assert!(lowering.normalize_bigint_literal("0x").is_none());
        assert!(lowering.normalize_bigint_literal("0b").is_none());
        assert!(lowering.normalize_bigint_literal("0o").is_none());
    }

    #[test]
    fn normalize_bigint_invalid_digit_after_prefix_returns_none() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // 'g' is not a valid hex digit.
        assert!(lowering.normalize_bigint_literal("0xG").is_none());
        // '2' is not a valid binary digit.
        assert!(lowering.normalize_bigint_literal("0b2").is_none());
        // '8' is not a valid octal digit.
        assert!(lowering.normalize_bigint_literal("0o8").is_none());
    }

    #[test]
    fn normalize_bigint_borrowed_decimal_when_no_change_needed() {
        // No prefix, no separators, no leading zeros → can stay borrowed.
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        let result = lowering.normalize_bigint_literal("42");
        assert!(matches!(result, Some(Cow::Borrowed("42"))));
    }

    #[test]
    fn normalize_bigint_handles_very_large_hex() {
        let arena = NodeArena::new();
        let interner = TypeInterner::new();
        let lowering = make_lowering(&arena, &interner);

        // u128::MAX = 340282366920938463463374607431768211455
        assert_eq!(
            lowering
                .normalize_bigint_literal("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .as_deref(),
            Some("340282366920938463463374607431768211455"),
        );
    }
}
