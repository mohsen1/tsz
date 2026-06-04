impl<'a> DeclarationEmitter<'a> {
    fn extract_type_param_name(segment: &str) -> Option<String> {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return None;
        }
        let trimmed = trimmed.strip_prefix("const ").unwrap_or(trimmed).trim();
        let trimmed = trimmed.strip_prefix("in ").unwrap_or(trimmed).trim();
        let trimmed = trimmed.strip_prefix("out ").unwrap_or(trimmed).trim();
        let name: String = trimmed
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }

    pub(in crate::declaration_emitter) fn replace_whole_word(
        text: &str,
        word: &str,
        replacement: &str,
    ) -> String {
        let mut result = String::with_capacity(text.len() + 16);
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let text_len = bytes.len();
        let mut i = 0;
        while i < text_len {
            if i + word_len <= text_len && &bytes[i..i + word_len] == word_bytes {
                let before_ok = i == 0 || !Self::is_ident_char(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char(bytes[i + word_len]);
                if before_ok && after_ok {
                    result.push_str(replacement);
                    i += word_len;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }
        result
    }

    pub(in crate::declaration_emitter) const fn is_ident_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
    }

    pub(in crate::declaration_emitter) fn print_synthetic_class_extends_alias_type(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(type_id);
        };
        let Some(callable_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) else {
            return self.print_type_id(type_id);
        };
        let callable = interner.callable_shape(callable_id);
        let has_properties = callable.properties.iter().any(|prop| {
            let name = interner.resolve_atom(prop.name);
            name != "prototype" && !name.starts_with("__private_brand_")
        });

        if callable.symbol.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && callable.construct_signatures[0].type_predicate.is_none()
        {
            return self.print_construct_signature_arrow_text(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        self.print_type_id(type_id)
    }

    pub(in crate::declaration_emitter) fn print_construct_signature_arrow_text(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_abstract: bool,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(sig.return_type);
        };

        let type_params = if sig.type_params.is_empty() {
            String::new()
        } else {
            let params = sig
                .type_params
                .iter()
                .map(|tp| {
                    let mut text = String::new();
                    if tp.is_const {
                        text.push_str("const ");
                    }
                    text.push_str(&interner.resolve_atom(tp.name));
                    if let Some(constraint) = tp.constraint {
                        text.push_str(" extends ");
                        text.push_str(&self.print_type_id(constraint));
                    }
                    if let Some(default) = tp.default {
                        text.push_str(" = ");
                        text.push_str(&self.print_type_id(default));
                    }
                    text
                })
                .collect::<Vec<_>>();
            format!("<{}>", params.join(", "))
        };

        let params = sig
            .params
            .iter()
            .map(|param| {
                let mut text = String::new();
                if param.rest {
                    text.push_str("...");
                }
                if let Some(name) = param.name {
                    text.push_str(&interner.resolve_atom(name));
                    if param.optional {
                        text.push('?');
                    }
                    text.push_str(": ");
                }
                text.push_str(&self.print_type_id(param.type_id));
                text
            })
            .collect::<Vec<_>>();

        let prefix = if is_abstract { "abstract new " } else { "new " };
        format!(
            "{prefix}{}({}) => {}",
            type_params,
            params.join(", "),
            self.print_type_id(sig.return_type)
        )
    }
}
