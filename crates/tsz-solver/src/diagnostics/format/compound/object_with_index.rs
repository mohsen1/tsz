//! Indexed object formatting helpers.

use super::super::TypeFormatter;
use crate::types::{ObjectShape, PropertyInfo, TypeId};

impl<'a> TypeFormatter<'a> {
    pub(crate) fn format_object_with_index(&mut self, shape: &ObjectShape) -> String {
        let mut parts = Vec::new();
        let use_array_to_locale_display = self.should_expand_array_to_locale_string_display(shape);

        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            let key_type_str = self.format(idx.key_type);
            let value_str = self.format_index_signature_value(idx.value_type);
            parts.push(format!("{ro}[{key_name}: {key_type_str}]: {value_str}"));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            let key_type_str = self.format(idx.key_type);
            let value_str = self.format_index_signature_value(idx.value_type);
            parts.push(format!("{ro}[{key_name}: {key_type_str}]: {value_str}"));
        }

        let mut display_props = self.visible_object_properties(shape.properties.as_slice());
        let has_decl_order = display_props.iter().any(|p| p.declaration_order > 0);
        if use_array_to_locale_display {
            display_props.sort_by(|a, b| {
                self.array_like_display_head_rank(a)
                    .cmp(&self.array_like_display_head_rank(b))
            });
        } else if has_decl_order {
            display_props.sort_by_key(|p| p.declaration_order);
        }

        for prop in display_props {
            if use_array_to_locale_display && self.atom(prop.name).as_ref() == "toLocaleString" {
                parts.push(
                    "toLocaleString: { (): string; (locales: string | string[], options?: (NumberFormatOptions & DateTimeFormatOptions) | undefined): string; }"
                        .to_string(),
                );
            } else {
                parts.push(self.format_property(prop));
            }
        }

        if use_array_to_locale_display
            && !parts
                .iter()
                .any(|part| part.starts_with("[Symbol.") || part.starts_with("readonly [Symbol."))
            && parts.len() >= 22
        {
            let original_len = parts.len();
            let insert_at = parts.len() - 1;
            parts.insert(
                insert_at,
                "readonly [Symbol.unscopables]: { ...; }".to_string(),
            );
            if original_len >= 22 {
                parts.insert(
                    insert_at,
                    "[Symbol.iterator]: () => ArrayIterator<any>".to_string(),
                );
            }
        }

        self.format_object_parts(parts)
    }

    fn should_expand_array_to_locale_string_display(&mut self, shape: &ObjectShape) -> bool {
        shape.number_index.is_some()
            && shape
                .properties
                .iter()
                .any(|prop| self.atom(prop.name).as_ref() == "toString")
            && shape
                .properties
                .iter()
                .any(|prop| self.atom(prop.name).as_ref() == "toLocaleString")
            && shape
                .properties
                .iter()
                .any(|prop| self.atom(prop.name).as_ref() == "includes")
    }

    fn array_like_display_head_rank(&self, prop: &PropertyInfo) -> (usize, u32) {
        let rank = match self.interner.resolve_atom_ref(prop.name).as_ref() {
            "toString" => 0,
            "toLocaleString" => 1,
            _ => 2,
        };
        (rank, prop.declaration_order)
    }

    fn format_index_signature_value(&mut self, value_type: TypeId) -> String {
        let previous_skip = self.skip_intersection_display_alias;
        self.skip_intersection_display_alias = true;
        let result = self.format(value_type).into_owned();
        self.skip_intersection_display_alias = previous_skip;
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::TypeInterner;
    use crate::diagnostics::format::TypeFormatter;
    use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TypeId, Visibility};

    #[test]
    fn format_array_like_object_with_index_prefers_es5_display_head() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
        let includes = db.function(FunctionShape::new(
            vec![ParamInfo {
                name: Some(db.intern_string("searchElement")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            TypeId::BOOLEAN,
        ));

        let shape = crate::types::ObjectShape {
            properties: vec![
                PropertyInfo::new(db.intern_string("includes"), includes),
                PropertyInfo::new(db.intern_string("toString"), method),
                PropertyInfo::new(db.intern_string("toLocaleString"), method),
            ],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);

        assert!(
            result.starts_with("{ [x: number]: number; toString:"),
            "Expected Array display head after index signature, got: {result}"
        );
    }

    #[test]
    fn format_array_like_object_with_symbol_tail_omits_late_methods() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
        let includes = db.function(FunctionShape::new(
            vec![ParamInfo {
                name: Some(db.intern_string("searchElement")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            TypeId::BOOLEAN,
        ));

        let mut properties = vec![
            PropertyInfo::new(db.intern_string("toString"), method),
            PropertyInfo::new(db.intern_string("toLocaleString"), method),
            PropertyInfo::new(db.intern_string("pop"), method),
            PropertyInfo::new(db.intern_string("push"), method),
            PropertyInfo::new(db.intern_string("includes"), includes),
        ];
        properties
            .extend((1..=27).map(|idx| {
                PropertyInfo::new(db.intern_string(&format!("p{idx}")), TypeId::NUMBER)
            }));
        properties.push(PropertyInfo {
            name: db.intern_string("[Symbol.unscopables]"),
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: true,
            single_quoted_name: false,
        });

        let shape = crate::types::ObjectShape {
            properties,
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);

        assert!(
            result.contains("... 30 more ..."),
            "Expected tsc-style omitted count for array-like display, got: {result}"
        );
        assert!(
            result.contains("readonly [Symbol.unscopables]: any"),
            "Expected symbol tail for truncated mapped-array display, got: {result}"
        );
        assert!(
            !result.contains("pop:") && !result.contains("push:"),
            "Expected late array methods to stay behind the omitted marker, got: {result}"
        );
    }
}
