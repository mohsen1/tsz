//! Advanced type lowering: conditionals, mapped types, indexed access,
//! literal parsing, type references, and remaining simple type forms.

use super::*;

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
            let mut visited = FxHashSet::default();
            self.collect_infer_bindings(extends_type, &mut visited);
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
            if iterations > Self::MAX_TREE_WALK_ITERATIONS {
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

    pub(super) fn collect_infer_bindings(&self, type_id: TypeId, visited: &mut FxHashSet<TypeId>) {
        if !visited.insert(type_id) {
            return;
        }

        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return,
        };

        match key {
            TypeData::Infer(info) => {
                self.add_type_param_binding(info.name, type_id);
                if let Some(constraint) = info.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = info.default {
                    self.collect_infer_bindings(default, visited);
                }
            }
            TypeData::Array(elem) => self.collect_infer_bindings(elem, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    self.collect_infer_bindings(element.type_id, visited);
                }
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for member in members.iter() {
                    self.collect_infer_bindings(*member, visited);
                }
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
                if let Some(index) = &shape.string_index {
                    self.collect_infer_bindings(index.key_type, visited);
                    self.collect_infer_bindings(index.value_type, visited);
                }
                if let Some(index) = &shape.number_index {
                    self.collect_infer_bindings(index.key_type, visited);
                    self.collect_infer_bindings(index.value_type, visited);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_infer_bindings(param.type_id, visited);
                }
                self.collect_infer_bindings(shape.return_type, visited);
                for param in &shape.type_params {
                    if let Some(constraint) = param.constraint {
                        self.collect_infer_bindings(constraint, visited);
                    }
                    if let Some(default) = param.default {
                        self.collect_infer_bindings(default, visited);
                    }
                }
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.collect_infer_bindings(param.type_id, visited);
                    }
                    self.collect_infer_bindings(sig.return_type, visited);
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint {
                            self.collect_infer_bindings(constraint, visited);
                        }
                        if let Some(default) = param.default {
                            self.collect_infer_bindings(default, visited);
                        }
                    }
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.collect_infer_bindings(param.type_id, visited);
                    }
                    self.collect_infer_bindings(sig.return_type, visited);
                    for param in &sig.type_params {
                        if let Some(constraint) = param.constraint {
                            self.collect_infer_bindings(constraint, visited);
                        }
                        if let Some(default) = param.default {
                            self.collect_infer_bindings(default, visited);
                        }
                    }
                }
                for prop in &shape.properties {
                    self.collect_infer_bindings(prop.type_id, visited);
                }
            }
            TypeData::TypeParameter(info) => {
                if let Some(constraint) = info.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = info.default {
                    self.collect_infer_bindings(default, visited);
                }
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_infer_bindings(app.base, visited);
                for &arg in &app.args {
                    self.collect_infer_bindings(arg, visited);
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.collect_infer_bindings(cond.check_type, visited);
                self.collect_infer_bindings(cond.extends_type, visited);
                self.collect_infer_bindings(cond.true_type, visited);
                self.collect_infer_bindings(cond.false_type, visited);
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                if let Some(constraint) = mapped.type_param.constraint {
                    self.collect_infer_bindings(constraint, visited);
                }
                if let Some(default) = mapped.type_param.default {
                    self.collect_infer_bindings(default, visited);
                }
                self.collect_infer_bindings(mapped.constraint, visited);
                if let Some(name_type) = mapped.name_type {
                    self.collect_infer_bindings(name_type, visited);
                }
                self.collect_infer_bindings(mapped.template, visited);
            }
            TypeData::IndexAccess(obj, idx) => {
                self.collect_infer_bindings(obj, visited);
                self.collect_infer_bindings(idx, visited);
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.collect_infer_bindings(inner, visited);
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_infer_bindings(*inner, visited);
                    }
                }
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.collect_infer_bindings(type_arg, visited);
            }
            TypeData::Enum(_def_id, member_type) => {
                self.collect_infer_bindings(member_type, visited);
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => {}
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
            let type_param_id = self.interner.type_param(type_param.clone());
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

    pub(super) fn parse_numeric_literal_value(
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
            let cleaned = Self::strip_numeric_separators(text);
            return cleaned.as_ref().parse::<f64>().ok();
        }

        text.parse::<f64>().ok()
    }

    pub(super) fn parse_radix_digits(text: &str, base: u32) -> Option<f64> {
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
                            if let Some(value) =
                                self.parse_numeric_literal_value(lit_data.value, &lit_data.text)
                            {
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
                                        if let Some(value) = self.parse_numeric_literal_value(
                                            lit_data.value,
                                            &lit_data.text,
                                        ) {
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
                let type_args: Vec<TypeId> =
                    args.nodes.iter().map(|&idx| self.lower_type(idx)).collect();
                return self.interner.application(base_type, type_args);
            }
            base_type
        } else {
            TypeId::ERROR
        }
    }

    /// Lower a qualified name type (A.B).
    pub(super) fn lower_qualified_name_type(&self, node_idx: NodeIndex) -> TypeId {
        // Phase 4.2: Must resolve to DefId - no fallback to SymbolRef
        // The def_id_resolver closure must be provided and must return valid DefIds
        if let Some(def_id) = self.resolve_def_id(node_idx) {
            return self.interner.lazy(def_id);
        }
        // If def_id resolution failed, this is an error - don't create bogus Lazy types
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

            // Phase 4.2: Must resolve to DefId - no fallback to SymbolRef
            //
            // Try name-based resolution — it's reliable for cross-arena
            // lowering because it uses the identifier text (extracted from the
            // current arena) to look up directly in file_locals. The NodeIndex-
            // based resolver iterates ALL declaration arenas and can produce
            // false positives when the same NodeIndex maps to different
            // identifiers in different arenas (e.g., NodeIndex(50) is "Promise"
            // in arena A but "AbortSignal" in arena B).
            if let Some(def_id) = self.resolve_def_id_by_name(name) {
                let lazy_type = self.interner.lazy(def_id);
                return lazy_type;
            }
            // Fall back to NodeIndex-based resolution for same-arena contexts
            // where no name-based resolver is available (e.g., user code).
            if let Some(def_id) = self.resolve_def_id(node_idx) {
                let lazy_type = self.interner.lazy(def_id);
                return lazy_type;
            }

            TypeId::ERROR
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
