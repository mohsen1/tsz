//! Core type display and formatting utilities for error reporting.

mod annotation_format;
mod recursion_guard;

use crate::query_boundaries::diagnostics as query;
use crate::state::CheckerState;
pub(in crate::error_reporter::core) use recursion_guard::DisplayRecursionGuard;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

thread_local! {
    /// When set, `normalize_assignability_display_type` preserves the *literal*
    /// return type of a function/method/constructor signature instead of
    /// widening it for display (`{ m(): 1 }` stays `{ m(): 1 }`, not
    /// `{ m(): number }`).
    ///
    /// `tsc`'s `getWidenedType` widens only *fresh* literals, so a literal in a
    /// **declared** signature position is rendered verbatim, while a **fresh**
    /// function expression's inferred return literal is widened (`(x) => 1`
    /// displays as `(x) => number`). tsz loses per-literal freshness for inferred
    /// arrow returns, so the discriminator is the *source provenance*: only the
    /// declared-identifier source path activates this scope. Off by default, so
    /// fresh function-expression sources keep widening.
    static PRESERVE_SIGNATURE_RETURN_LITERALS: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

fn preserve_signature_return_literals_active() -> bool {
    PRESERVE_SIGNATURE_RETURN_LITERALS.with(std::cell::Cell::get)
}

/// Reorder the top-level members of a repainted union annotation so the
/// nullish keywords render at the tail — `null` first, then `undefined` —
/// matching `tsc`'s `formatUnionTypes`, which filters nullable constituents
/// out of the printed member walk and appends them after it. The annotation
/// repaint otherwise preserves the written order (`undefined | (() => void)`),
/// an order `tsc` never prints.
///
/// Only parts that are exactly the `null`/`undefined` keywords move; both are
/// reserved in type-name position, so a top-level union part with that text
/// can only be the intrinsic. The text is returned unchanged (original
/// spacing included) when the parts are already in canonical order.
fn reorder_nullish_union_parts_to_tail(text: &str) -> String {
    // Split on top-level ` | `, tracking bracket depth and string/template
    // literals so pipes inside parameter lists, generics, tuples, object
    // shapes, or literal types don't split. A `>` that closes an arrow (`=>`)
    // is not a bracket.
    let bytes = text.as_bytes();
    let mut parts: Vec<&str> = Vec::new();
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if let Some(active) = quote {
            if ch == active && (i == 0 || bytes[i - 1] != b'\\') {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '<' | '[' | '{' => depth += 1,
            '>' if i > 0 && bytes[i - 1] == b'=' => {}
            ')' | '>' | ']' | '}' => depth = depth.saturating_sub(1),
            '|' if depth == 0
                && i > 0
                && bytes[i - 1] == b' '
                && bytes.get(i + 1) == Some(&b' ') =>
            {
                parts.push(text[start..i - 1].trim());
                start = i + 2;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    if parts.len() < 2 {
        return text.to_string();
    }

    let mut ordered: Vec<&str> = Vec::with_capacity(parts.len());
    let mut has_null = false;
    let mut has_undefined = false;
    for &part in &parts {
        match part {
            "null" => has_null = true,
            "undefined" => has_undefined = true,
            other => ordered.push(other),
        }
    }
    if !has_null && !has_undefined {
        return text.to_string();
    }
    if has_null {
        ordered.push("null");
    }
    if has_undefined {
        ordered.push("undefined");
    }
    if ordered == parts {
        return text.to_string();
    }
    ordered.join(" | ")
}

/// RAII guard that makes assignability display normalization preserve declared
/// signature return literals for the duration of its lifetime, restoring the
/// previous state on drop (including on unwind), mirroring the depth/budget
/// guards in `normalize_assignability_display_type`.
pub(in crate::error_reporter) struct PreserveSignatureReturnLiteralsScope(bool);

impl PreserveSignatureReturnLiteralsScope {
    pub(in crate::error_reporter) fn enter() -> Self {
        Self(PRESERVE_SIGNATURE_RETURN_LITERALS.with(|cell| cell.replace(true)))
    }
}

impl Drop for PreserveSignatureReturnLiteralsScope {
    fn drop(&mut self) {
        PRESERVE_SIGNATURE_RETURN_LITERALS.with(|cell| cell.set(self.0));
    }
}

impl<'a> CheckerState<'a> {
    /// Apply display widening to an already-normalized signature return type.
    ///
    /// A deferred conditional return is left as-is; a fresh function
    /// expression's inferred return literal widens to its base (`(x) => 1` →
    /// `(x) => number`); a declared (non-fresh) signature return literal is kept
    /// verbatim (`{ m(): 1 }`) when [`PreserveSignatureReturnLiteralsScope`] is
    /// active. A fresh object-literal return widens in both cases.
    fn display_widen_signature_return(
        &self,
        original_return: TypeId,
        normalized_return: TypeId,
    ) -> TypeId {
        if crate::query_boundaries::common::is_conditional_type(self.ctx.types, original_return) {
            return normalized_return;
        }
        // A declared (non-fresh) signature return literal is kept verbatim; a
        // fresh function expression's inferred return literal widens to its base.
        // A fresh object-literal return widens in both cases.
        let return_type = if preserve_signature_return_literals_active() {
            normalized_return
        } else {
            crate::query_boundaries::common::widen_type(self.ctx.types, normalized_return)
        };
        self.widen_fresh_object_literal_properties_for_display(return_type)
    }

    pub(in crate::error_reporter) fn sanitize_type_annotation_text_for_diagnostic(
        &self,
        text: String,
        allow_object_shapes: bool,
    ) -> Option<String> {
        fn parenthesize_intersection_in_union_text(text: &str) -> String {
            let mut parts = Vec::new();
            let mut current = String::new();
            let mut depth = 0u32;

            for (i, ch) in text.char_indices() {
                match ch {
                    '(' | '<' | '[' => {
                        depth += 1;
                        current.push(ch);
                    }
                    ')' | '>' | ']' => {
                        depth = depth.saturating_sub(1);
                        current.push(ch);
                    }
                    '|' if depth == 0
                        && text.get(i.saturating_sub(1)..i) == Some(" ")
                        && text.get(i + 1..i + 2) == Some(" ") =>
                    {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                    _ => current.push(ch),
                }
            }
            parts.push(current.trim().to_string());

            // Only wrap intersection parts in parens when there are multiple union
            // alternatives. A standalone intersection like `T & (0 | 1 | 2)` should
            // not get extra outer parens, but in a union like `A & B | C & D`, both
            // intersection parts need parens: `(A & B) | (C & D)`.
            if parts.len() == 1 {
                return parts.into_iter().next().unwrap_or_default();
            }

            parts
                .into_iter()
                .map(|part| {
                    if part.contains(" & ") && !part.starts_with('(') {
                        format!("({part})")
                    } else {
                        part
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ")
        }

        let mut text = text.trim().trim_start_matches(':').trim().to_string();
        if let Some(newline) = text.find('\n') {
            text = text[..newline].trim_end().to_string();
            if text.chars().filter(|&c| c == '{').count()
                != text.chars().filter(|&c| c == '}').count()
            {
                return None;
            }
        }
        if text.ends_with('=') {
            text.pop();
            text = text.trim_end().to_string();
        }
        while matches!(text.chars().last(), Some(',') | Some(';')) {
            text.pop();
            text = text.trim_end().to_string();
        }
        while matches!(text.chars().last(), Some(')')) {
            let open_count = text.chars().filter(|&ch| ch == '(').count();
            let close_count = text.chars().filter(|&ch| ch == ')').count();
            if close_count <= open_count {
                break;
            }
            text.pop();
            text = text.trim_end().to_string();
        }
        if !allow_object_shapes && (text.starts_with('{') || text.starts_with('[')) {
            return None;
        }
        let open_count = text.chars().filter(|&ch| ch == '(').count();
        let close_count = text.chars().filter(|&ch| ch == ')').count();
        if open_count != close_count || text.is_empty() {
            return None;
        }
        if text.contains(" | ") {
            text = reorder_nullish_union_parts_to_tail(&text);
        }
        if text.contains(" & ") && text.contains(" | ") {
            text = parenthesize_intersection_in_union_text(&text);
        }
        Some(text)
    }

    fn param_matches_property_key_literal(&self, prop_name: Atom, ty: TypeId) -> bool {
        let prop_name = self.ctx.types.resolve_atom_ref(prop_name);
        if query::display_string_literal_type(self.ctx.types, prop_name.as_ref()) == ty {
            return true;
        }
        prop_name
            .parse::<f64>()
            .ok()
            .is_some_and(|num| query::display_number_literal_type(self.ctx.types, num) == ty)
    }

    fn normalize_excess_display_type_for_property(
        &self,
        prop_name: Option<Atom>,
        ty: TypeId,
    ) -> TypeId {
        let ty = self.normalize_excess_display_type(ty);
        let Some(prop_name) = prop_name else {
            return ty;
        };

        if let Some(narrowed) = self.narrow_excess_function_param_by_property_key(prop_name, ty) {
            return narrowed;
        }

        if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| {
                    let normalized = crate::query_boundaries::common::evaluate_type(
                        self.ctx.types,
                        param.type_id,
                    );
                    let normalized = self
                        .narrow_excess_function_param_by_property_key(prop_name, normalized)
                        .unwrap_or(normalized);
                    let type_id = if self.param_matches_property_key_literal(prop_name, normalized)
                        || crate::query_boundaries::common::type_application(
                            self.ctx.types,
                            normalized,
                        )
                        .is_some()
                        || crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            normalized,
                        )
                        .is_some()
                    {
                        normalized
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            normalized,
                        )
                    };
                    query::display_param_with_type(param, type_id)
                })
                .collect();

            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b) {
                ty
            } else {
                query::function_type_with_params_replaced(self.ctx.types, shape.as_ref(), params)
            }
        } else {
            ty
        }
    }

    pub(in crate::error_reporter) fn widen_function_like_display_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        if self.union_is_all_function_like(type_id) {
            return type_id;
        }
        let constructor_display_def = self
            .ctx
            .definition_store
            .find_def_for_type(type_id)
            .and_then(|def_id| {
                self.ctx
                    .definition_store
                    .get(def_id)
                    .filter(|def| def.is_class_constructor())
                    .map(|_| def_id)
            });

        // Preserve the `Name<Args>` surface of a generic interface/class
        // application whose instance is *callable* (it carries a call or
        // construct signature, as in prop-types' `Validator<T>`:
        // `interface Validator<T> { (x: object): Error | null; [brand]?: T }`).
        // Evaluating such an application to its structural callable instance
        // (below) resolves the def name but discards the type arguments, so the
        // source would render as a bare `Validator` instead of
        // `Validator<string>`, diverging from tsc. A *non-callable* generic
        // instance (`Box<Animal>`) already reconstructs `Name<Args>` from the
        // evaluated instance downstream and needs that instance for nested
        // missing-member elaboration, so it is deliberately left to the normal
        // evaluation path. A reducing *type-alias* application (`DeepReadonly<X>`)
        // is likewise untouched, matching tsc dropping the alias name.
        let original_application = type_id;
        let original_is_generic_interface_or_class_application =
            query::is_generic_application(self.ctx.types, type_id)
                && query::type_application(self.ctx.types, type_id)
                    .and_then(|app| query::lazy_def_id(self.ctx.types, app.base))
                    .and_then(|def_id| self.ctx.definition_store.get(def_id))
                    .is_some_and(|def| {
                        matches!(
                            def.kind,
                            tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
                        )
                    });

        let type_id = self.evaluate_type_with_env(type_id);

        if original_is_generic_interface_or_class_application
            && (query::has_call_signatures(self.ctx.types, type_id)
                || query::has_construct_signatures(self.ctx.types, type_id))
        {
            let widened = query::widen_type(self.ctx.types, original_application);
            if let Some(def_id) = constructor_display_def {
                self.ctx
                    .definition_store
                    .register_type_to_def(widened, def_id);
            }
            return widened;
        }
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, type_id) {
            let widened =
                crate::query_boundaries::diagnostics::widen_type_preserving_unique_symbols(
                    self.ctx.types,
                    type_id,
                );
            if let Some(def_id) = constructor_display_def {
                self.ctx
                    .definition_store
                    .register_type_to_def(widened, def_id);
            }
            return widened;
        }
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);
        // Preserve a unique-symbol source (`typeof x` / `unique symbol`) — tsc
        // never renders it as `symbol`; the `symbol` widening is a
        // mutable-location semantic rule, not a display rule.
        let mut widened =
            crate::query_boundaries::diagnostics::widen_type_preserving_unique_symbols(
                self.ctx.types,
                type_id,
            );
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, widened)
        {
            let widened_return =
                self.widen_fresh_object_literal_properties_for_display(shape.return_type);
            if widened_return != shape.return_type {
                widened = query::function_type_with_return_replaced(
                    self.ctx.types,
                    shape.as_ref(),
                    widened_return,
                );
            }
        } else if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, widened)
        {
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;

            for sig in &mut widened_shape.call_signatures {
                let widened_return =
                    self.widen_fresh_object_literal_properties_for_display(sig.return_type);
                if widened_return != sig.return_type {
                    changed = true;
                    sig.return_type = widened_return;
                }
            }
            for sig in &mut widened_shape.construct_signatures {
                let widened_return =
                    self.widen_fresh_object_literal_properties_for_display(sig.return_type);
                if widened_return != sig.return_type {
                    changed = true;
                    sig.return_type = widened_return;
                }
            }

            if changed {
                widened = query::callable_type_from_shape(self.ctx.types, widened_shape);
            }
        }
        if let Some(def_id) = constructor_display_def {
            self.ctx
                .definition_store
                .register_type_to_def(widened, def_id);
        }
        widened
    }

    pub(crate) fn widen_fresh_object_literal_properties_for_display(&self, ty: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };
        // Only widen properties when the outer object type is itself a fresh
        // object literal (e.g. inferred return type from `() => ({ a: 1 })`).
        // Annotated types like `{ a: "x" }` carry the user's intent and must
        // not have their literal property types widened away in diagnostics —
        // tsc preserves them as-is, so when we receive a non-fresh shape here
        // we have to leave it untouched.
        if !crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty) {
            return ty;
        }
        let mut widened_shape = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut widened_shape.properties {
            let widened_read =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop.type_id);
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                prop.write_type,
            );
            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }
            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }
        if !changed {
            return ty;
        }
        query::object_type_from_shape(self.ctx.types, widened_shape)
    }

    pub(in crate::error_reporter) fn normalize_property_receiver_application_display_type(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        // Bound the structural / lazy-resolution recursion so deeply
        // self-expanding generic types cannot overflow the stack while a
        // diagnostic type is normalized for display (issue #12455). The
        // downstream formatter truncates nested printing well below this depth,
        // so leaving the type unchanged at the cap never alters rendered output.
        let Some(_display_guard) = DisplayRecursionGuard::enter() else {
            return ty;
        };
        let Some(app) = query::type_application(self.ctx.types, ty) else {
            return ty;
        };
        let args: Vec<_> = app
            .args
            .iter()
            .map(|&arg| self.normalize_property_receiver_application_display_arg(arg))
            .collect();

        if args == app.args {
            ty
        } else {
            query::display_application_type(self.ctx.types, app.base, args)
        }
    }

    fn normalize_property_receiver_application_display_arg(&mut self, ty: TypeId) -> TypeId {
        let Some(_display_guard) = DisplayRecursionGuard::enter() else {
            return ty;
        };
        // Only resolve `Lazy(DefId)` references via the type environment.
        // Calling `evaluate_type_with_env` on richer shapes (e.g. `keyof T`,
        // `T[K]`, conditional types) eagerly expands them to their evaluated
        // structural form and loses the original syntactic identity that tsc
        // preserves in property-receiver diagnostics. Structural recursion
        // below already handles applications/unions/intersections/objects.
        if crate::query_boundaries::common::is_lazy_type(self.ctx.types.as_type_database(), ty) {
            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated != ty {
                return self.normalize_property_receiver_application_display_arg(evaluated);
            }
        }

        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_property_receiver_application_display_arg(arg))
                .collect();
            return if args == app.args {
                ty
            } else {
                query::display_application_type(self.ctx.types, app.base, args)
            };
        }

        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let normalized: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_property_receiver_application_display_arg(member))
                .collect();
            return if normalized == members {
                ty
            } else {
                query::display_union_preserve_members_type(self.ctx.types, normalized)
            };
        }

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let normalized: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_property_receiver_application_display_arg(member))
                .collect();
            return if normalized == members {
                ty
            } else {
                query::display_intersection_type(self.ctx.types, normalized)
            };
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };
        let should_widen_properties =
            crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty)
                || (self.ctx.types.get_display_properties(ty).is_some() && shape.symbol.is_none());
        if !should_widen_properties {
            return ty;
        }

        let mut normalized_shape = shape.as_ref().clone();
        let mut changed = self.ctx.types.get_display_properties(ty).is_some();

        for prop in &mut normalized_shape.properties {
            let normalized_read =
                self.normalize_property_receiver_application_display_arg(prop.type_id);
            let normalized_write =
                self.normalize_property_receiver_application_display_arg(prop.write_type);
            let widened_read = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                normalized_read,
            );
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                normalized_write,
            );

            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }

            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }

        if let Some(index) = normalized_shape.string_index.as_mut() {
            let normalized =
                self.normalize_property_receiver_application_display_arg(index.value_type);
            let widened =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, normalized);
            if widened != index.value_type {
                changed = true;
                index.value_type = widened;
            }
        }

        if let Some(index) = normalized_shape.number_index.as_mut() {
            let normalized =
                self.normalize_property_receiver_application_display_arg(index.value_type);
            let widened =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, normalized);
            if widened != index.value_type {
                changed = true;
                index.value_type = widened;
            }
        }

        if changed {
            let new_ty = query::object_type_from_shape(self.ctx.types, normalized_shape);
            if let Some(alias_origin) = self.ctx.types.get_display_alias(ty) {
                let alias_origin =
                    self.normalize_property_receiver_application_display_type(alias_origin);
                if query::type_application(self.ctx.types, alias_origin).is_some() {
                    self.ctx
                        .types
                        .store_display_alias_preferring_application(new_ty, alias_origin);
                } else {
                    self.ctx.types.store_display_alias(new_ty, alias_origin);
                }
            }
            new_ty
        } else {
            ty
        }
    }

    pub(in crate::error_reporter::core) fn normalize_excess_display_type(
        &self,
        ty: TypeId,
    ) -> TypeId {
        // Bound the recursion so deeply self-expanding generic types cannot
        // overflow the stack while an excess-property diagnostic type is
        // normalized for display (issue #12455).
        let Some(_display_guard) = DisplayRecursionGuard::enter() else {
            return ty;
        };
        let ty = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_excess_display_type(arg))
                .collect();
            if args == app.args {
                ty
            } else {
                query::display_application_type(self.ctx.types, app.base, args)
            }
        } else if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| {
                    query::display_param_with_type(
                        param,
                        self.normalize_excess_display_type(param.type_id),
                    )
                })
                .collect();
            let return_type = self.normalize_excess_display_type(shape.return_type);
            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                && return_type == shape.return_type
            {
                ty
            } else {
                query::function_type_with_params_and_return_replaced(
                    self.ctx.types,
                    shape.as_ref(),
                    params,
                    return_type,
                )
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            query::display_union_preserve_members_type(
                self.ctx.types,
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            query::display_intersection_type(
                self.ctx.types,
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else if let Some(normalized) = self.normalize_excess_display_object_type(ty) {
            normalized
        } else {
            ty
        }
    }

    pub(in crate::error_reporter) fn normalize_assignability_display_type(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        // Depth guard: recursive types (e.g., `interface Foo { j: Foo }`) cause
        // unbounded recursion when normalizing property types for display. Deep
        // recursion can trip the stack overflow breaker in get_type_of_symbol,
        // permanently poisoning symbol resolution and causing subsequent type
        // evaluations to return ERROR — which silently suppresses real
        // assignability diagnostics (e.g., TS2322).
        //
        // The decrement is owned by an RAII guard so the depth is restored on
        // every exit — including when the inner walk unwinds via a panic a
        // caller (`try_tsz`, LSP) catches mid-recursion. A manual post-call
        // restore would be skipped on unwind, leaking a positive depth into the
        // next compilation on a reused batch worker thread; later display
        // normalizations at that stale depth would bail at the cap and return
        // early, making display/eval differ run-to-run (#13368). The counter is
        // function-private, so the batch boundary reset cannot reach it — RAII
        // self-cleaning is the only correct isolation here.
        thread_local! {
            static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }
        struct DepthReset;
        impl Drop for DepthReset {
            fn drop(&mut self) {
                DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            }
        }
        let depth = DEPTH.get();
        if depth >= 10 {
            return ty;
        }

        DEPTH.set(depth + 1);
        let _depth_reset = DepthReset;
        // The recursion below is depth-capped but not breadth-capped: each
        // node can fan out into freshly interned children, so the work
        // budget is what bounds it (issue #13040).
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        let mut visiting = FxHashSet::default();
        self.normalize_assignability_display_type_inner(ty, &mut visiting, 0)
    }

    fn should_truncate_assignability_display_type(&self, ty: TypeId, depth: usize) -> bool {
        if depth < 3 {
            return false;
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || crate::query_boundaries::common::function_shape_id(self.ctx.types, ty).is_some()
            || crate::query_boundaries::common::callable_shape_id(self.ctx.types, ty).is_some()
        {
            return true;
        }

        if depth < 5 {
            return false;
        }

        if query::type_application(self.ctx.types, ty).is_some() {
            return true;
        }

        if query::union_members(self.ctx.types, ty).is_some_and(|members| members.len() > 4)
            || query::intersection_members(self.ctx.types, ty)
                .is_some_and(|members| members.len() > 3)
        {
            return true;
        }

        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty).is_some_and(
            |shape| {
                shape.properties.len() > 6
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            },
        )
    }

    fn normalize_assignability_display_type_inner(
        &mut self,
        ty: TypeId,
        visiting: &mut FxHashSet<TypeId>,
        depth: usize,
    ) -> TypeId {
        const MAX_ASSIGNABILITY_DISPLAY_DEPTH: usize = 12;
        // Type parameters should not be normalized — they should display as their
        // name (e.g., `T`) not their constraint (e.g., `String`). The solver's
        // `get_object_shape` looks through type parameter constraints, which causes
        // the object-shape branch below to incorrectly resolve `T extends String`
        // to the `String` interface's object type.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, ty) {
            return ty;
        }
        // Literal types should be preserved as-is — don't evaluate/widen them
        // to their base type.  tsc shows `"TypeTwo"` in error messages, not
        // `string`.  Without this guard the else-branch evaluates the literal
        // and the widened primitive replaces the original.
        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some() {
            return ty;
        }
        // Work budget: once the per-rendered-type visit budget is exhausted,
        // truncate hard and display the type as-is (issue #13040).
        if !crate::error_reporter::display_budget::try_consume_visit() {
            return ty;
        }
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        if crate::error_reporter::display_budget::is_exhausted() {
            return ty;
        }

        if depth >= MAX_ASSIGNABILITY_DISPLAY_DEPTH || !visiting.insert(ty) {
            return ty;
        }

        if self.should_truncate_assignability_display_type(ty, depth) {
            visiting.remove(&ty);
            return ty;
        }

        let result = if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let has_undefined = members.contains(&TypeId::UNDEFINED);
            let has_null = members.contains(&TypeId::NULL);
            let generic_scaffolding_only = members.iter().all(|&member| {
                member == TypeId::UNDEFINED
                    || member == TypeId::NULL
                    || crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        member,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        member,
                    )
            });
            if generic_scaffolding_only {
                if has_undefined {
                    TypeId::UNDEFINED
                } else if has_null {
                    TypeId::NULL
                } else {
                    ty
                }
            } else if query::union_members(self.ctx.types, ty).is_none() {
                // Non-generic intersection that isn't also a union: preserve as-is.
                // Evaluation would lose display_properties (literal values) on fresh
                // object members. tsc shows `{ fooProp: "frizzlebizzle"; } & Bar`
                // not `{ fooProp: string; } & Bar`.
                ty
            } else if let Some(members) = query::union_members(self.ctx.types, ty) {
                let mut normalized = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    normalized.push(self.normalize_assignability_display_type_inner(
                        member,
                        visiting,
                        depth + 1,
                    ));
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return ty;
                    }
                }
                if normalized == members {
                    ty
                } else {
                    query::display_union_preserve_members_type(self.ctx.types, normalized)
                }
            } else if query::function_shape(self.ctx.types, ty).is_some_and(|shape| {
                crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    shape.return_type,
                )
            }) {
                ty
            } else {
                let evaluated =
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            ty,
                        )
                    {
                        ty
                    } else {
                        self.evaluate_type_for_assignability(ty)
                    };

                if crate::error_reporter::display_budget::is_exhausted() {
                    visiting.remove(&ty);
                    return evaluated;
                }

                if self.should_truncate_assignability_display_type(evaluated, depth) {
                    visiting.remove(&ty);
                    return evaluated;
                }

                if let Some(app) = query::type_application(self.ctx.types, evaluated) {
                    let mut args = Vec::with_capacity(app.args.len());
                    for &arg in app.args.iter() {
                        args.push(self.normalize_assignability_display_type_inner(
                            arg,
                            visiting,
                            depth + 1,
                        ));
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                    }
                    if args == app.args {
                        evaluated
                    } else {
                        query::display_application_type(self.ctx.types, app.base, args)
                    }
                } else if let Some(shape) = query::function_shape(self.ctx.types, evaluated) {
                    let mut params = Vec::with_capacity(shape.params.len());
                    for param in shape.params.iter() {
                        // Skip normalizing TypeQuery param types to preserve typeof
                        // syntax, matching tsc's behavior of not expanding typeof
                        // references in parameter positions.
                        let type_id = if crate::query_boundaries::common::is_type_query_type(
                            self.ctx.types,
                            param.type_id,
                        ) {
                            param.type_id
                        } else {
                            self.normalize_assignability_display_type_inner(
                                param.type_id,
                                visiting,
                                depth + 1,
                            )
                        };
                        params.push(query::display_param_with_type(param, type_id));
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                    }
                    // Skip normalizing TypeQuery return types to preserve the typeof
                    // syntax. Resolving TypeQuery to the full function type causes double
                    // arrows like `() => () => typeof fn` instead of `() => typeof fn`.
                    let return_type = if crate::query_boundaries::common::is_type_query_type(
                        self.ctx.types,
                        shape.return_type,
                    ) || crate::query_boundaries::common::is_conditional_type(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        shape.return_type
                    } else {
                        self.normalize_assignability_display_type_inner(
                            shape.return_type,
                            visiting,
                            depth + 1,
                        )
                    };
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                    let return_type =
                        self.display_widen_signature_return(shape.return_type, return_type);
                    if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                        && return_type == shape.return_type
                    {
                        evaluated
                    } else {
                        query::function_type_with_params_and_return_replaced(
                            self.ctx.types,
                            shape.as_ref(),
                            params,
                            return_type,
                        )
                    }
                } else if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    evaluated,
                ) {
                    let mut shape = shape.as_ref().clone();
                    let mut changed = false;
                    for prop in &mut shape.properties {
                        let normalized_read = self.normalize_assignability_display_type_inner(
                            prop.type_id,
                            visiting,
                            depth + 1,
                        );
                        let normalized_write = self.normalize_assignability_display_type_inner(
                            prop.write_type,
                            visiting,
                            depth + 1,
                        );
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                        changed |=
                            normalized_read != prop.type_id || normalized_write != prop.write_type;
                        prop.type_id = normalized_read;
                        prop.write_type = normalized_write;
                    }
                    if let Some(index) = shape.string_index.as_mut() {
                        let normalized = self.normalize_assignability_display_type_inner(
                            index.value_type,
                            visiting,
                            depth + 1,
                        );
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                        changed |= normalized != index.value_type;
                        index.value_type = normalized;
                    }
                    if let Some(index) = shape.number_index.as_mut() {
                        let normalized = self.normalize_assignability_display_type_inner(
                            index.value_type,
                            visiting,
                            depth + 1,
                        );
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                        changed |= normalized != index.value_type;
                        index.value_type = normalized;
                    }
                    if changed {
                        let new_ty = query::object_type_preserving_display_properties(
                            self.ctx.types,
                            evaluated,
                            shape,
                        );
                        // Propagate display_alias so the formatter can still
                        // recover the named form (e.g., `Array<string>`) for
                        // types whose property types changed during normalization.
                        if let Some(alias_origin) = self.ctx.types.get_display_alias(evaluated) {
                            self.ctx.types.store_display_alias(new_ty, alias_origin);
                        }
                        // Propagate definition store registration so the
                        // formatter can still show named types (interfaces,
                        // classes) whose properties changed during
                        // normalization — e.g., `Date` instead of the full
                        // structural expansion.
                        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(evaluated)
                        {
                            self.ctx
                                .definition_store
                                .register_type_to_def(new_ty, def_id);
                        }
                        new_ty
                    } else {
                        evaluated
                    }
                } else if let Some(members) = query::union_members(self.ctx.types, evaluated) {
                    let mut normalized = Vec::with_capacity(members.len());
                    for &member in members.iter() {
                        normalized.push(self.normalize_assignability_display_type_inner(
                            member,
                            visiting,
                            depth + 1,
                        ));
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                    }
                    query::display_union_preserve_members_type(self.ctx.types, normalized)
                } else if let Some(members) = query::intersection_members(self.ctx.types, evaluated)
                {
                    let mut normalized = Vec::with_capacity(members.len());
                    for &member in members.iter() {
                        normalized.push(self.normalize_assignability_display_type_inner(
                            member,
                            visiting,
                            depth + 1,
                        ));
                        if crate::error_reporter::display_budget::is_exhausted() {
                            visiting.remove(&ty);
                            return evaluated;
                        }
                    }
                    query::display_intersection_type(self.ctx.types, normalized)
                } else {
                    evaluated
                }
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            let mut normalized = Vec::with_capacity(members.len());
            for &member in members.iter() {
                normalized.push(self.normalize_assignability_display_type_inner(
                    member,
                    visiting,
                    depth + 1,
                ));
                if crate::error_reporter::display_budget::is_exhausted() {
                    visiting.remove(&ty);
                    return ty;
                }
            }
            if normalized == members {
                ty
            } else {
                query::display_union_preserve_members_type(self.ctx.types, normalized)
            }
        } else if let Some(app) = query::type_application(self.ctx.types, ty) {
            if query::preserves_named_application_base(self.ctx.types, app.base) {
                let mut args = Vec::with_capacity(app.args.len());
                for &arg in app.args.iter() {
                    args.push(self.normalize_assignability_display_type_inner(
                        arg,
                        visiting,
                        depth + 1,
                    ));
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return ty;
                    }
                }
                if args == app.args {
                    ty
                } else {
                    query::display_application_type(self.ctx.types, app.base, args)
                }
            } else {
                let evaluated =
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            ty,
                        )
                    {
                        ty
                    } else {
                        self.evaluate_type_for_assignability(ty)
                    };

                if crate::error_reporter::display_budget::is_exhausted() {
                    visiting.remove(&ty);
                    return evaluated;
                }

                if self.should_truncate_assignability_display_type(evaluated, depth) {
                    visiting.remove(&ty);
                    return evaluated;
                }
                self.normalize_assignability_display_type_inner(evaluated, visiting, depth + 1)
            }
        } else {
            // For function/callable shapes whose params or return contain a
            // `TypeQuery`, skip evaluation — `evaluate_type_for_assignability`
            // expands `TypeQuery` in nested positions, which for self-
            // referential typeof (e.g. `(t: typeof C.g) => void` where `C.g`
            // IS that function) produces an extra outer wrapper. tsc keeps
            // the typeof reference intact in the rendered message.
            if crate::query_boundaries::diagnostics::function_signature_has_typeof(
                self.ctx.types,
                ty,
            ) {
                visiting.remove(&ty);
                return ty;
            }

            let evaluated =
                if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
                    && crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                {
                    ty
                } else {
                    self.evaluate_type_for_assignability(ty)
                };

            if crate::error_reporter::display_budget::is_exhausted() {
                visiting.remove(&ty);
                return evaluated;
            }

            if self.should_truncate_assignability_display_type(evaluated, depth) {
                visiting.remove(&ty);
                return evaluated;
            }

            if let Some(app) = query::type_application(self.ctx.types, evaluated) {
                let mut args = Vec::with_capacity(app.args.len());
                for &arg in app.args.iter() {
                    args.push(self.normalize_assignability_display_type_inner(
                        arg,
                        visiting,
                        depth + 1,
                    ));
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                }
                if args == app.args {
                    evaluated
                } else {
                    query::display_application_type(self.ctx.types, app.base, args)
                }
            } else if let Some(shape) = query::function_shape(self.ctx.types, evaluated) {
                let mut params = Vec::with_capacity(shape.params.len());
                for param in shape.params.iter() {
                    params.push(query::display_param_with_type(
                        param,
                        self.normalize_assignability_display_type_inner(
                            param.type_id,
                            visiting,
                            depth + 1,
                        ),
                    ));
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                }
                // Skip normalizing TypeQuery return types to preserve the typeof
                // syntax. Resolving TypeQuery to the full function type causes double
                // arrows like `() => () => typeof fn` instead of `() => typeof fn`.
                let return_type = if crate::query_boundaries::common::is_type_query_type(
                    self.ctx.types,
                    shape.return_type,
                ) || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    shape.return_type,
                ) {
                    shape.return_type
                } else {
                    self.normalize_assignability_display_type_inner(
                        shape.return_type,
                        visiting,
                        depth + 1,
                    )
                };
                if crate::error_reporter::display_budget::is_exhausted() {
                    visiting.remove(&ty);
                    return evaluated;
                }
                let return_type =
                    self.display_widen_signature_return(shape.return_type, return_type);
                if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                    && return_type == shape.return_type
                {
                    evaluated
                } else {
                    query::function_type_with_params_and_return_replaced(
                        self.ctx.types,
                        shape.as_ref(),
                        params,
                        return_type,
                    )
                }
            } else if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
            {
                let mut shape = shape.as_ref().clone();
                let mut changed = false;
                for prop in &mut shape.properties {
                    let normalized_read = self.normalize_assignability_display_type_inner(
                        prop.type_id,
                        visiting,
                        depth + 1,
                    );
                    let normalized_write = self.normalize_assignability_display_type_inner(
                        prop.write_type,
                        visiting,
                        depth + 1,
                    );
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                    changed |=
                        normalized_read != prop.type_id || normalized_write != prop.write_type;
                    prop.type_id = normalized_read;
                    prop.write_type = normalized_write;
                }
                if let Some(index) = shape.string_index.as_mut() {
                    let normalized = self.normalize_assignability_display_type_inner(
                        index.value_type,
                        visiting,
                        depth + 1,
                    );
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                    changed |= normalized != index.value_type;
                    index.value_type = normalized;
                }
                if let Some(index) = shape.number_index.as_mut() {
                    let normalized = self.normalize_assignability_display_type_inner(
                        index.value_type,
                        visiting,
                        depth + 1,
                    );
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                    changed |= normalized != index.value_type;
                    index.value_type = normalized;
                }
                if changed {
                    let new_ty = query::object_type_preserving_display_properties(
                        self.ctx.types,
                        evaluated,
                        shape,
                    );
                    // Propagate display_alias and def-store registration so the
                    // formatter can still show named types (Date, Error, etc.)
                    // after normalization modifies property types.
                    if let Some(alias_origin) = self.ctx.types.get_display_alias(evaluated) {
                        self.ctx.types.store_display_alias(new_ty, alias_origin);
                    }
                    if let Some(def_id) = self.ctx.definition_store.find_def_for_type(evaluated) {
                        self.ctx
                            .definition_store
                            .register_type_to_def(new_ty, def_id);
                    }
                    new_ty
                } else {
                    evaluated
                }
            } else if let Some(members) = query::intersection_members(self.ctx.types, evaluated) {
                let mut normalized = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    normalized.push(self.normalize_assignability_display_type_inner(
                        member,
                        visiting,
                        depth + 1,
                    ));
                    if crate::error_reporter::display_budget::is_exhausted() {
                        visiting.remove(&ty);
                        return evaluated;
                    }
                }
                query::display_intersection_type(self.ctx.types, normalized)
            } else {
                evaluated
            }
        };

        visiting.remove(&ty);
        result
    }

    fn split_optional_object_for_excess_display(&self, ty: TypeId) -> TypeId {
        let ty = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let non_undefined: Vec<_> = members
                .iter()
                .copied()
                .filter(|member| *member != TypeId::UNDEFINED)
                .collect();
            if non_undefined.len() == 1 && non_undefined.len() != members.len() {
                return non_undefined[0];
            }
        }
        ty
    }

    fn split_wildcard_object_for_excess_display(&mut self, ty: TypeId) -> Option<String> {
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let ty = self.split_optional_object_for_excess_display(ty);
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }

        let wildcard_name = self.ctx.types.intern_string("*");
        let mut wildcard_props = Vec::new();
        let mut named_props = Vec::new();

        for prop in &shape.properties {
            let mut cloned = prop.clone();
            cloned.type_id =
                self.normalize_excess_display_type_for_property(Some(cloned.name), cloned.type_id);
            cloned.write_type = self
                .normalize_excess_display_type_for_property(Some(cloned.name), cloned.write_type);
            if cloned.name == wildcard_name {
                wildcard_props.push(cloned);
            } else {
                named_props.push(cloned);
            }
        }

        if wildcard_props.is_empty() || named_props.is_empty() {
            return None;
        }

        let named_obj = query::object_type_from_properties(self.ctx.types, named_props);
        let wildcard_obj = query::object_type_from_properties(self.ctx.types, wildcard_props);
        Some(format!(
            "{} & {}",
            self.format_type_diagnostic(named_obj),
            self.format_type_diagnostic(wildcard_obj)
        ))
    }

    fn materialize_finite_mapped_type_for_display(&mut self, ty: TypeId) -> Option<TypeId> {
        // Bound the recursion so deeply self-expanding generic types cannot
        // overflow the stack while materializing a finite mapped type for
        // display (issue #12455).
        let _display_guard = DisplayRecursionGuard::enter()?;
        if !crate::error_reporter::display_budget::try_consume_visit() {
            return None;
        }
        if let Some((mapped_id, mapped)) = query::mapped_type(self.ctx.types, ty) {
            let names =
                crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                    self.ctx.types,
                    mapped_id,
                )?;
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort_by(|a, b| {
                self.ctx
                    .types
                    .resolve_atom_ref(*a)
                    .cmp(&self.ctx.types.resolve_atom_ref(*b))
            });

            let mut properties = Vec::with_capacity(names.len());
            for name in names {
                if !crate::error_reporter::display_budget::try_consume_visit() {
                    return None;
                }
                let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
                let type_id =
                    crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                        self.ctx.types,
                        mapped_id,
                        &property_name,
                    )?;
                let type_id = self.normalize_excess_display_type_for_property(Some(name), type_id);
                let property = query::mapped_display_property(
                    name,
                    type_id,
                    mapped.optional_modifier,
                    mapped.readonly_modifier,
                );
                properties.push(property);
            }

            Some(query::object_type_from_properties(
                self.ctx.types,
                properties,
            ))
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let mut changed = false;
            let remapped: Vec<_> = members
                .iter()
                .map(|&member| {
                    if crate::error_reporter::display_budget::is_exhausted() {
                        return member;
                    }
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        materialized
                    } else {
                        member
                    }
                })
                .collect();
            changed.then(|| query::display_intersection_type(self.ctx.types, remapped))
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            let mut changed = false;
            let remapped: Vec<_> = members
                .iter()
                .map(|&member| {
                    if crate::error_reporter::display_budget::is_exhausted() {
                        return member;
                    }
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        materialized
                    } else {
                        member
                    }
                })
                .collect();
            changed.then(|| query::display_union_type(self.ctx.types, remapped))
        } else {
            None
        }
    }

    pub(crate) fn format_excess_property_target_type(&mut self, ty: TypeId) -> String {
        // Preserve named aliases before evaluation strips the Lazy(DefId).
        if crate::query_boundaries::common::is_lazy_type(self.ctx.types, ty) {
            return self.format_type_diagnostic_widened(ty);
        }

        // TS2353 displays the object-like branch that rejected the property, not
        // the optional `undefined | ...` wrapper that often appears on contextual
        // object properties. Do this before union/intersection formatting so a
        // single remaining intersection can use the specialized object display path.
        let ty = self.strip_non_object_union_members_for_excess_display(ty);

        if let Some(display) = self.format_object_before_callable_union_for_excess_display(ty) {
            return display;
        }

        if let Some(display) = self.format_intersection_union_for_excess_display(ty) {
            return display;
        }

        // Preserve generic Application syntax in excess-property messages.
        let is_application =
            crate::query_boundaries::common::type_application(self.ctx.types, ty).is_some();
        let evaluated_application = if is_application {
            None
        } else if let Some(alias) = self.ctx.types.get_display_alias(ty) {
            crate::query_boundaries::common::type_application(self.ctx.types, alias).map(|_| alias)
        } else {
            None
        };
        let application_display = if is_application {
            Some(ty)
        } else {
            evaluated_application
        };
        if let Some(application_display) = application_display {
            let normalized =
                self.normalize_property_receiver_application_display_type(application_display);
            if normalized != application_display {
                return self.format_type_diagnostic_widened(normalized);
            }
            let mut formatter = self
                .ctx
                .create_diagnostic_type_formatter()
                .with_display_properties()
                .with_skip_application_alias_names();
            return formatter.format(ty).into_owned();
        }

        // For already-evaluated types, check if a type alias name can be recovered
        // via body_to_alias or type_to_def. This handles cases where the Lazy
        // reference was resolved before reaching this function.
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(ty) {
            return alias_name;
        }

        if let Some(display) = self.split_wildcard_object_for_excess_display(ty) {
            return display;
        }

        // For union targets, tsc strips non-object members (primitives like number,
        // undefined, null, etc.) from the displayed type. Excess property checking
        // only applies to object-like members, so the diagnostic should reference
        // only those members rather than the full union.
        let ty = self.strip_non_object_union_members_for_excess_display(ty);

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let preserve_intersection_parts = members.iter().any(|member| {
                crate::query_boundaries::common::evaluate_type(self.ctx.types, *member)
                    == TypeId::OBJECT
            });
            let mut changed = false;
            let parts: Vec<String> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        self.format_type_diagnostic_widened(materialized)
                    } else {
                        self.format_type_diagnostic_widened(member)
                    }
                })
                .collect();
            if changed || preserve_intersection_parts {
                return parts.join(" & ");
            }
        }

        let display_ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let display_ty = self.strip_top_level_readonly_for_excess_display(display_ty);
        let display_ty = self.normalize_nested_excess_display_type(display_ty);
        self.format_type_diagnostic_widened(display_ty)
    }

    pub(in crate::error_reporter) fn format_extract_keyof_string_type(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let members = crate::query_boundaries::common::intersection_members(self.ctx.types, ty)?;
        if members.len() != 2 || !members.contains(&TypeId::STRING) {
            return None;
        }

        let other = members
            .iter()
            .copied()
            .find(|&member| member != TypeId::STRING)?;
        if !crate::query_boundaries::common::is_keyof_type(self.ctx.types, other) {
            return None;
        }

        Some(format!(
            "Extract<{}, string>",
            self.format_type_for_assignability_message(other)
        ))
    }

    pub(in crate::error_reporter) fn format_annotation_like_type(&mut self, text: &str) -> String {
        let mut formatted = text.trim().to_string();
        if formatted.contains(";}") {
            formatted = formatted.replace(";}", "; }");
        }
        if formatted.contains(':') && formatted.ends_with(" }") && !formatted.ends_with("; }") {
            formatted = format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        // Normalize `{prop: type}` to `{ prop: type; }` — tsc always adds
        // spaces inside braces and trailing semicolons for inline object types.
        // Handle both standalone `{...}` and intersection parts `& {...}`.
        formatted = Self::normalize_single_quoted_string_literal_types(&formatted);
        if !self.ctx.compiler_options.exact_optional_property_types {
            formatted = Self::add_undefined_to_optional_object_property_display(&formatted);
        }
        formatted = Self::normalize_inline_object_braces(&formatted);
        // Prefer Array<T> shorthand conversion in annotation text, but preserve
        // generic constraint surface syntax (`<T extends Array<U>>`) where tsc
        // keeps the declared Array form.
        formatted = Self::normalize_array_generic_to_shorthand(&formatted);
        formatted
    }

    pub(in crate::error_reporter) fn should_use_evaluated_assignability_display(
        &self,
        ty: TypeId,
        evaluated: TypeId,
    ) -> bool {
        if ty == evaluated || evaluated == TypeId::ERROR {
            return false;
        }

        if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
            return false;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some() {
            return false;
        }

        // For TypeQuery (typeof X), don't use evaluated display - preserve the
        // typeof syntax instead of expanding to the full function type.
        // This prevents double function arrows like `() => () => typeof fn`.
        if crate::query_boundaries::common::is_type_query_type(self.ctx.types, ty) {
            return false;
        }

        // For function/callable types whose signatures carry a `TypeQuery`
        // in any param or return position, don't use the evaluated display.
        // Evaluation would resolve the `TypeQuery` to the full function
        // type, causing double arrows like `() => () => typeof fn`
        // (return-side) or extra wrapping like `(t: (t: typeof g) => void)
        // => void` (param-side, for recursive `typeof X` referring to the
        // enclosing function).
        if crate::query_boundaries::diagnostics::function_signature_has_typeof(self.ctx.types, ty) {
            return false;
        }

        // Generic Application of a TypeAlias whose body is IndexedAccess or
        // Conditional: expand only when evaluation reduces to a concrete
        // (non-conditional, non-indexed-access) shape. tsc keeps the alias
        // when reduction stalls (e.g. free type params in a conditional).
        // Must run before the contains-type-parameters guard below.
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
            && let Some(def_id) =
                crate::query_boundaries::common::get_application_lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && let Some(body) = def.body
            && (crate::query_boundaries::common::is_index_access_type(self.ctx.types, body)
                || crate::query_boundaries::common::is_conditional_type(self.ctx.types, body))
        {
            return !crate::query_boundaries::common::is_conditional_type(
                self.ctx.types,
                evaluated,
            ) && !crate::query_boundaries::common::is_index_access_type(
                self.ctx.types,
                evaluated,
            );
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
        {
            return false;
        }

        if evaluated == TypeId::NEVER
            || crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
        {
            return true;
        }

        if (crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty).is_some()
            || crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, ty)
                .is_some())
            && (crate::query_boundaries::common::is_template_literal_type(
                self.ctx.types,
                evaluated,
            ) || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some())
        {
            return true;
        }

        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_keyof_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }

        // For IndexAccess types, display the evaluated form when it resolves to a
        // concrete type (union, object, primitive). This makes error messages show
        // the resolved type instead of the raw indexed access syntax.
        // e.g., `Pairs<FooBar>[keyof FooBar]` → `{ key: "foo"; value: string; } | { key: "bar"; value: number; }`
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
            return true;
        }

        matches!(
            evaluated,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
        )
    }

    pub(in crate::error_reporter) fn format_structural_indexed_object_type(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
        if shape.string_index.is_none() && shape.number_index.is_none() {
            return None;
        }

        let mut parts = Vec::new();
        for idx in shape.string_index.iter().chain(shape.number_index.iter()) {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            let key_kind = self.format_type(idx.key_type);
            parts.push(format!(
                "[{key_name}: {key_kind}]: {}",
                self.format_type(idx.value_type)
            ));
        }
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            let optional = if prop.optional { "?" } else { "" };
            let readonly = if prop.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{readonly}{name}{optional}: {}",
                self.format_type(prop.type_id)
            ));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    /// Check if a type contains string literal types (directly or as union members).
    /// Used to determine whether an object literal property should display its
    /// literal value (for discriminated union contexts) or the widened type.
    pub(in crate::error_reporter) fn type_contains_string_literal(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::type_contains_string_literal(self.ctx.types, type_id)
    }

    /// True when `type_id` is — or, recursively, has a union member that is — a
    /// unit literal type whose widened primitive base equals `primitive_base`
    /// (one of `string` / `number` / `boolean` / `bigint`).
    ///
    /// Generalizes the string-literal-only acceptance test used when deciding
    /// whether to preserve a fresh source literal in an assignment mismatch
    /// display. It mirrors the per-kind constituent check in tsc's
    /// `isLiteralOfContextualType`: the source literal is kept only when the
    /// contextual target carries a literal of the matching primitive base, so a
    /// numeric source against a string-literal target still widens.
    ///
    /// When `primitive_base` is not itself a primitive base (e.g. the source was
    /// not a literal) no literal member can match and this returns `false`,
    /// preserving the prior widening behavior for non-literal sources.
    pub(in crate::error_reporter) fn type_contains_literal_of_primitive_base(
        &self,
        type_id: TypeId,
        primitive_base: TypeId,
    ) -> bool {
        if let Some(members) = query::union_members(self.ctx.types, type_id) {
            return members.iter().any(|&member| {
                self.type_contains_literal_of_primitive_base(member, primitive_base)
            });
        }
        let widened = query::widen_literal_to_primitive(self.ctx.types, type_id);
        widened != type_id && widened == primitive_base
    }

    pub(in crate::error_reporter) fn literal_expression_display(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        // Skip only parentheses, NOT type assertions. A type assertion like
        // `'bar' as any` changes the type to `any`, so the literal display
        // should not be used — the asserted type should be displayed instead.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        // If this is a type assertion expression (as/angle-bracket), don't
        // display the inner literal — let the caller use the asserted type.
        if node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                let escaped = lit
                    .text
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                Some(format!("\"{escaped}\""))
            }
            // Numeric and bigint literals render from their scanned token text
            // verbatim. The bigint token value carries the trailing `n` (e.g.
            // `1n`), matching tsc; without the bigint case a fresh object-literal
            // `bigint` property (interned in widened form) could not have its
            // literal text resurrected and displayed as `bigint` where tsc shows
            // `1n`.
            k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::BigIntLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.literal_expression_display(unary.operand)?;
                match unary.operator {
                    k if k == tsz_scanner::SyntaxKind::MinusToken as u16 => {
                        if operand.parse::<f64>().is_ok_and(|value| value == 0.0) {
                            return Some("0".to_string());
                        }
                        Some(format!("-{operand}"))
                    }
                    k if k == tsz_scanner::SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let cond = self.ctx.arena.get_conditional_expr(node)?;
                let left = self.literal_expression_display(cond.when_true)?;
                let right = self.literal_expression_display(cond.when_false)?;
                if left == right {
                    Some(left)
                } else {
                    Some(format!("{left} | {right}"))
                }
            }
            _ => None,
        }
    }
}
