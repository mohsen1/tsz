//! Compound type formatting methods for `TypeFormatter`.

mod object_parts;
mod object_plain;
mod object_with_index;

use super::TypeFormatter;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, LiteralValue, MappedModifier,
    MappedType, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement,
    TupleListId, TypeData, TypeId, TypeParamInfo,
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

        let last = params.len().wrapping_sub(1);
        for (i, p) in params.iter().enumerate() {
            // tsc's `signatureToString` expands a trailing rest parameter whose
            // type is a concrete tuple into positional parameters
            // (`...rest: [A, B]` renders as `rest_0: A, rest_1: B`). Only the
            // final parameter is a candidate.
            if i == last
                && p.rest
                && let Some(expanded) = self.expand_rest_tuple_param_for_display(p)
            {
                rendered.extend(expanded);
                continue;
            }
            let name = p
                .name
                .map_or_else(|| "_".to_string(), |atom| self.atom(atom).to_string());
            // An untyped-JS parameter is optional only for call arity; `tsc`
            // renders it as required (`x: any`, not `x?: any`).
            rendered.push(self.render_param_display(
                &name,
                p.is_display_optional(),
                p.rest,
                p.type_id,
            ));
        }

        rendered
    }

    /// Render a single parameter (`{...}{name}{?}: {type}`), applying tsc's
    /// optional-parameter surface (`x?: T | undefined`).
    fn render_param_display(
        &mut self,
        name: &str,
        optional: bool,
        rest: bool,
        type_id: TypeId,
    ) -> String {
        let optional_marker = if optional { "?" } else { "" };
        let rest_prefix = if rest { "..." } else { "" };
        let type_str: String = if optional {
            let formatted = self.format(type_id).into_owned();
            if self.preserve_optional_parameter_surface_syntax {
                formatted
            } else if type_id == TypeId::NEVER {
                "undefined".to_string()
            } else if !self.type_contains_undefined(type_id) {
                format!("{formatted} | undefined")
            } else {
                formatted
            }
        } else {
            self.format(type_id).into_owned()
        };
        format!("{rest_prefix}{name}{optional_marker}: {type_str}")
    }

    /// Expand a trailing rest parameter whose type is a concrete tuple into
    /// per-element positional parameters, matching tsc's `signatureToString`
    /// (`getExpandedParameters` / `getParameterNameAtPosition`).
    ///
    /// Returns `None` (keep the written `...rest: [..]` form) when the rest type
    /// is not a tuple, the rest parameter is unnamed, or the tuple carries a
    /// rest element in a non-trailing position (`[string, ...number[], boolean]`)
    /// — a parameter list can't hold a middle rest, so tsc leaves those
    /// unexpanded.
    ///
    /// Each element's name follows tsc: its tuple label when present, otherwise
    /// `{restname}_{index}` for a fixed/optional element and the bare `{restname}`
    /// for the trailing variadic element.
    fn expand_rest_tuple_param_for_display(
        &mut self,
        rest_param: &ParamInfo,
    ) -> Option<Vec<String>> {
        let rest_name_atom = rest_param.name?;
        let tuple_type = self.resolve_rest_param_display_tuple(rest_param.type_id)?;
        let Some(TypeData::Tuple(list_id)) = self.interner.lookup(tuple_type) else {
            return None;
        };
        let elements = self.interner.tuple_list(list_id);
        let len = elements.len();
        if elements
            .iter()
            .enumerate()
            .any(|(i, e)| e.rest && i + 1 != len)
        {
            return None;
        }
        let rest_name = self.atom(rest_name_atom).to_string();
        let mut out = Vec::with_capacity(len);
        for (i, e) in elements.iter().enumerate() {
            let name = if let Some(label) = e.name {
                self.atom(label).to_string()
            } else if e.rest {
                rest_name.clone()
            } else {
                format!("{rest_name}_{i}")
            };
            out.push(self.render_param_display(&name, e.optional, e.rest, e.type_id));
        }
        Some(out)
    }

    /// Resolve a rest parameter's declared type to the concrete tuple to expand
    /// for display, or `None` to keep the written `...rest: T` form.
    ///
    /// A directly-written tuple (optionally behind one `ReadonlyType` wrapper)
    /// is returned as-is. tsc also expands a rest parameter whose type is a
    /// *type alias* or *generic application* that resolves to a tuple:
    /// `type R = [a, b]; ...rest: R` renders like the inline tuple, and this
    /// holds through nested and generic aliases
    /// (`type R<T> = [T, number]; ...rest: R<string>`). Such a `Lazy(DefId)` /
    /// `Application` is evaluated to its underlying type through the formatter's
    /// own definition store, then re-checked for a tuple. A resolved array
    /// (`type R = number[]`) or a tuple with a non-trailing rest is left
    /// unexpanded, matching tsc, which keeps the written alias form for those.
    fn resolve_rest_param_display_tuple(&self, type_id: TypeId) -> Option<TypeId> {
        // Fast path: a directly-written tuple, optionally behind one readonly
        // wrapper (tsc drops the `readonly` modifier when expanding). This keeps
        // the common inline-tuple case off the evaluator.
        let peeled = self.peel_readonly_type(type_id);
        if matches!(self.interner.lookup(peeled), Some(TypeData::Tuple(_))) {
            return Some(peeled);
        }
        // Only an alias, a generic application, or one of those behind a
        // readonly wrapper can resolve to a different tuple; evaluating anything
        // else would be wasted display work.
        if !matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Lazy(_) | TypeData::Application(_) | TypeData::ReadonlyType(_))
        ) {
            return None;
        }
        // Resolve through the formatter's definition store: it carries the alias
        // bodies and generic type-parameter lists the evaluator needs, which the
        // default (`NoopResolver`) evaluator does not have. Without a store the
        // alias cannot be named either, so keeping the written form is correct.
        let def_store = self.def_store?;
        let resolver = crate::caches::query_cache_evaluation::StoreOnlyResolver::new(def_store);
        let evaluated = crate::evaluation::evaluate::evaluate_type_with_resolver(
            self.interner,
            &resolver,
            type_id,
        );
        if evaluated == type_id {
            return None;
        }
        let peeled = self.peel_readonly_type(evaluated);
        matches!(self.interner.lookup(peeled), Some(TypeData::Tuple(_))).then_some(peeled)
    }

    /// Peel one `ReadonlyType` wrapper. tsc drops the `readonly` modifier when
    /// it expands a rest tuple, so the wrapper must not block the tuple check.
    fn peel_readonly_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner)) => inner,
            _ => type_id,
        }
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
