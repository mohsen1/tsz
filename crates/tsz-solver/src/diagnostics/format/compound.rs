//! Compound type formatting methods for `TypeFormatter`.

mod object_parts;
mod object_with_index;

use super::TypeFormatter;
use super::needs_property_name_quotes;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, LiteralValue, MappedModifier,
    MappedType, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement,
    TypeData, TypeId, TypeParamInfo,
};
use std::borrow::Cow;
use tsz_binder::SymbolId;

/// Named options for `format_signature_with_predicate`.
pub(super) struct SignatureFormatOpts<'a> {
    pub this_type: Option<TypeId>,
    pub type_predicate: Option<&'a crate::types::TypePredicate>,
    pub is_construct: bool,
    pub is_abstract: bool,
    pub separator: &'a str,
}

impl<'a> TypeFormatter<'a> {
    fn visible_object_properties<'b>(&self, props: &'b [PropertyInfo]) -> Vec<&'b PropertyInfo> {
        let default_name = self.interner.intern_string("default");
        let internal_default_name = self.interner.intern_string("_default");
        let default_prop = props.iter().find(|prop| prop.name == default_name);

        props
            .iter()
            .filter(|prop| {
                if prop.name != internal_default_name {
                    return true;
                }
                let Some(default_prop) = default_prop else {
                    return true;
                };

                // Some module export surfaces retain the local `_default` binding
                // alongside the real `default` export. tsc hides that duplicate
                // implementation detail in object displays.
                prop.type_id != default_prop.type_id
                    || prop.write_type != default_prop.write_type
                    || prop.optional != default_prop.optional
                    || prop.readonly != default_prop.readonly
                    || prop.is_method != default_prop.is_method
            })
            .collect()
    }

    pub(super) fn format_literal(&mut self, lit: &LiteralValue) -> String {
        match lit {
            LiteralValue::String(s) => {
                let raw = self.atom(*s);
                let escaped = raw
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("\"{escaped}\"")
            }
            LiteralValue::Number(n) => {
                // Match JS `Number.prototype.toString()` so very large/small
                // values use scientific notation (e.g. `5.46e+244`) rather
                // than Rust's default integer expansion. Also handles
                // `Infinity`, `-Infinity`, and `NaN` consistently.
                crate::utils::js_number_to_string(n.0).into_owned()
            }
            LiteralValue::BigInt(b) => format!("{}n", self.atom(*b)),
            LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }

    pub(super) fn format_object(&mut self, props: &[PropertyInfo]) -> String {
        if props.is_empty() {
            return "{}".to_string();
        }
        let mut display_props = self.visible_object_properties(props);
        // Sort properties for display. Use declaration_order as primary key when
        // available, with tsc-compatible tiebreaking: numeric keys in numeric order,
        // then string keys in existing order (stable sort preserves Atom ID order).
        // Properties are stored sorted by Atom ID for identity/hashing, so display
        // order must be restored here.
        display_props.sort_by(|a, b| {
            // Primary: declaration_order (0 means unset, treated as equal)
            let ord = a.declaration_order.cmp(&b.declaration_order);
            if ord != std::cmp::Ordering::Equal
                && a.declaration_order > 0
                && b.declaration_order > 0
            {
                return ord;
            }
            // Tiebreak for properties with same declaration_order:
            // numeric keys get sorted numerically (tsc puts them first),
            // but string keys preserve their existing order via stable sort.
            let a_name = self.interner.resolve_atom_ref(a.name);
            let b_name = self.interner.resolve_atom_ref(b.name);
            let a_num = a_name.parse::<u64>();
            let b_num = b_name.parse::<u64>();
            match (a_num, b_num) {
                (Ok(an), Ok(bn)) => an.cmp(&bn),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                // For non-numeric keys with same decl_order, preserve existing
                // order (stable sort) — Atom ID order often matches source order
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        let formatted: Vec<String> = display_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        self.format_object_parts(formatted)
    }

    pub(super) fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let raw_name = self.atom(prop.name);
        let name = if needs_property_name_quotes(&raw_name) {
            // tsc uses double quotes for JSX-specific property names
            // (namespace-prefixed like "ns:attr" and data attributes like "data-foo"),
            // for names starting with a digit, and for names containing any
            // character outside [a-zA-Z0-9_-] (e.g. "*"). Single quotes are used
            // for all other quoted property names (e.g. 'stage-0', '').
            let use_double = raw_name.contains(':')
                || raw_name.starts_with("data-")
                || raw_name.chars().next().is_some_and(|c| c.is_ascii_digit())
                || raw_name
                    .chars()
                    .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'));
            if use_double {
                let escaped = raw_name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            } else {
                let escaped = raw_name.replace('\\', "\\\\").replace('\'', "\\'");
                format!("'{escaped}'")
            }
        } else {
            raw_name.to_string()
        };

        // Method shorthand: `name(params): return_type` instead of `name: (params) => return_type`
        if prop.is_method {
            match self.interner.lookup(prop.type_id) {
                Some(TypeData::Function(f_id)) => {
                    let shape = self.interner.function_shape(f_id);
                    let type_params = self.format_type_params(&shape.type_params);
                    let params = self.format_params(&shape.params, shape.this_type);
                    let return_str = self.format(shape.return_type);
                    return format!(
                        "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                        params = params.join(", ")
                    );
                }
                Some(TypeData::Callable(callable_id)) => {
                    let shape = self.interner.callable_shape(callable_id);
                    if let Some(sig) = shape.call_signatures.first() {
                        let type_params = self.format_type_params(&sig.type_params);
                        let params = self.format_params(&sig.params, sig.this_type);
                        let return_str = self.format(sig.return_type);
                        return format!(
                            "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                            params = params.join(", ")
                        );
                    }
                }
                _ => {}
            }
        }

        // tsc displays optional object properties WITH `| undefined`:
        // `n?: number | undefined`. If the stored type doesn't already contain
        // undefined, we append it.
        let type_str: String = if prop.optional {
            if self.preserve_optional_property_surface_syntax {
                let surface_type = if prop.write_type != TypeId::NONE {
                    prop.write_type
                } else {
                    prop.type_id
                };
                self.format(surface_type).into_owned()
            } else if prop.type_id == TypeId::NEVER {
                // `never | undefined` simplifies to `undefined`; tsc displays just `undefined`
                "undefined".to_string()
            } else if !self.type_contains_undefined(prop.type_id) {
                let formatted = self.format(prop.type_id).into_owned();
                format!("{formatted} | undefined")
            } else {
                self.format(prop.type_id).into_owned()
            }
        } else {
            self.format(prop.type_id).into_owned()
        };
        format!("{readonly}{name}{optional}: {type_str}")
    }

    /// Check if a type already contains `undefined` (as a union member or is undefined itself).
    /// Also treats `any` and `unknown` as absorbing undefined, since `any | undefined` == `any`
    /// and `unknown | undefined` == `unknown` in tsc's display.
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members
                .iter()
                .any(|&m| m == TypeId::UNDEFINED || m == TypeId::ANY || m == TypeId::UNKNOWN);
        }
        false
    }

    fn format_optional_tuple_element_type(&mut self, type_id: TypeId, named: bool) -> String {
        let formatted = self.format(type_id).into_owned();
        let absorbs_undefined =
            type_id == TypeId::UNDEFINED || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN;

        if self.preserve_optional_property_surface_syntax {
            if named {
                return formatted;
            }
            if !named && !absorbs_undefined && self.type_contains_undefined(type_id) {
                return format!("({formatted})?");
            }
            return format!("{formatted}?");
        }

        if named {
            if self.type_contains_undefined(type_id) {
                formatted
            } else {
                format!("{formatted} | undefined")
            }
        } else if absorbs_undefined {
            format!("{formatted}?")
        } else if self.type_contains_undefined(type_id) {
            format!("({formatted})?")
        } else {
            format!("({formatted} | undefined)?")
        }
    }

    pub(super) fn format_type_params(&mut self, type_params: &[TypeParamInfo]) -> String {
        if type_params.is_empty() {
            return String::new();
        }

        let mut parts = Vec::with_capacity(type_params.len());
        for tp in type_params {
            let mut part = String::new();
            if tp.is_const {
                part.push_str("const ");
            }
            part.push_str(self.atom(tp.name).as_ref());
            if let Some(constraint) = tp.constraint {
                part.push_str(" extends ");
                // tsc preserves declared generic form in constraints
                let prev = self.preserve_array_generic_form;
                self.preserve_array_generic_form = true;
                part.push_str(&self.format(constraint));
                self.preserve_array_generic_form = prev;
            }
            if let Some(default) = tp.default {
                part.push_str(" = ");
                part.push_str(&self.format(default));
            }
            parts.push(part);
        }

        format!("<{}>", parts.join(", "))
    }

    pub(super) fn format_params(
        &mut self,
        params: &[ParamInfo],
        this_type: Option<TypeId>,
    ) -> Vec<String> {
        let mut rendered = Vec::with_capacity(params.len() + usize::from(this_type.is_some()));

        if let Some(this_ty) = this_type {
            rendered.push(format!("this: {}", self.format(this_ty)));
        }

        for p in params {
            let name = p
                .name
                .map_or_else(|| "_".to_string(), |atom| self.atom(atom).to_string());
            let optional = if p.optional { "?" } else { "" };
            let rest = if p.rest { "..." } else { "" };
            let type_str: String = if p.optional {
                let formatted = self.format(p.type_id).into_owned();
                if self.preserve_optional_parameter_surface_syntax {
                    formatted
                } else if p.type_id == TypeId::NEVER {
                    "undefined".to_string()
                } else if !self.type_contains_undefined(p.type_id) {
                    format!("{formatted} | undefined")
                } else {
                    formatted
                }
            } else {
                self.format(p.type_id).into_owned()
            };
            rendered.push(format!("{rest}{name}{optional}: {type_str}"));
        }

        rendered
    }

    /// Format a signature with the given separator between params and return type.
    pub(super) fn format_signature(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        this_type: Option<TypeId>,
        return_type: TypeId,
        is_construct: bool,
        is_abstract: bool,
        separator: &str,
    ) -> String {
        self.format_signature_with_predicate(
            type_params,
            params,
            return_type,
            &SignatureFormatOpts {
                this_type,
                type_predicate: None,
                is_construct,
                is_abstract,
                separator,
            },
        )
    }

    /// Format a signature including an optional type predicate in the return type.
    ///
    /// When `type_predicate` is `Some`, the return type is formatted as
    /// `asserts v is T` or `v is T` instead of the raw return type.
    /// This matches tsc's display for assertion/type guard functions.
    pub(super) fn format_signature_with_predicate(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        return_type: TypeId,
        opts: &SignatureFormatOpts<'_>,
    ) -> String {
        let prefix = if opts.is_construct && opts.is_abstract {
            "abstract new "
        } else if opts.is_construct {
            "new "
        } else {
            ""
        };
        let type_params = self.format_type_params(type_params);
        let params = self.format_params(params, opts.this_type);
        let return_str: Cow<'static, str> = if let Some(pred) = opts.type_predicate {
            let target_name = match pred.target {
                crate::types::TypePredicateTarget::This => "this".to_string(),
                crate::types::TypePredicateTarget::Identifier(atom) => self.atom(atom).to_string(),
            };
            let type_part = pred.type_id.map(|tid| format!(" is {}", self.format(tid)));
            if pred.asserts {
                Cow::Owned(format!(
                    "asserts {}{}",
                    target_name,
                    type_part.unwrap_or_default()
                ))
            } else {
                Cow::Owned(format!("{}{}", target_name, type_part.unwrap_or_default()))
            }
        } else if self.diagnostic_mode
            && self.should_elide_recursive_typeof_function_return(return_type)
        {
            Cow::Borrowed("...")
        } else if opts.is_construct && return_type == TypeId::UNKNOWN {
            Cow::Borrowed("any")
        } else {
            self.format(return_type)
        };
        format!(
            "{}{}({}){} {}",
            prefix,
            type_params,
            params.join(", "),
            opts.separator,
            return_str
        )
    }

    fn should_elide_recursive_typeof_function_return(&self, return_type: TypeId) -> bool {
        match self.interner.lookup(return_type) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                self.type_is_or_contains_type_query(shape.return_type)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.type_is_or_contains_type_query(sig.return_type))
            }
            _ => false,
        }
    }

    fn type_is_or_contains_type_query(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::TypeQuery(_)) => true,
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                self.type_is_or_contains_type_query(shape.return_type)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| self.type_is_or_contains_type_query(sig.return_type))
            }
            _ => false,
        }
    }
}

include!("compound/union_display.rs");
include!("compound/intersection_constructs.rs");
include!("compound/def_symbol_names.rs");

/// Detects whether `s` reads as an intersection at the top level — i.e.
/// contains a ` & ` separator outside any brackets, parens, or braces.
/// Used by union-member parenthesization when the lookup-based heuristic
/// can't see through Lazy/Application wrappers.
fn contains_top_level_intersection_separator(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b'&' if depth == 0
                && i > 0
                && bytes[i - 1] == b' '
                && i + 1 < bytes.len()
                && bytes[i + 1] == b' ' =>
            {
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}
