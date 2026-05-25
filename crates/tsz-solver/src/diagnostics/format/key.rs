//! Main `TypeData` dispatch formatting helpers.

use super::{TypeFormatter, intrinsic};
use crate::types::{
    ObjectFlags, ObjectShape, StringIntrinsicKind, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use std::borrow::Cow;
use tracing::trace;
use tsz_binder::SymbolId;

impl<'a> TypeFormatter<'a> {
    pub(super) fn format_key(&mut self, type_id: TypeId, key: &TypeData) -> Cow<'static, str> {
        match key {
            TypeData::Intrinsic(kind) => Cow::Borrowed(intrinsic::format_intrinsic(*kind)),
            TypeData::Literal(lit) => self.format_literal(lit).into(),
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                if let Some(record_display) = self.format_in_operator_record(&shape) {
                    return record_display.into();
                }
                // Use display properties (pre-widened literal types) when enabled.
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    return self.format_object(display_props.as_slice()).into();
                }
                self.format_object(shape.properties.as_slice()).into()
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(record_display) = self.format_in_operator_record(&shape) {
                    return record_display.into();
                }
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    let mut display_shape = shape.as_ref().clone();
                    display_shape.properties = display_props.as_ref().clone();
                    return self.format_object_with_index(&display_shape).into();
                }
                self.format_object_with_index(shape.as_ref()).into()
            }
            TypeData::Union(members) => {
                if self.diagnostic_mode
                    && !self.expand_primitive_key_union
                    && self.is_primitive_key_union_data(key)
                {
                    return Cow::Borrowed("PropertyKey");
                }
                // tsc preserves top-level alias names that would otherwise be
                // lost during union flattening (e.g., `T | null` should not
                // expand to T's body). The checker records the unflattened
                // input member list as a side-table "origin"; consult it here
                // before structural display.
                let members = self.interner.type_list(*members);
                if !self.ignore_union_origins
                    && let Some(origin) = self.interner.get_union_origin(type_id)
                {
                    return self.format_union(origin.as_slice()).into();
                }
                self.format_union(members.as_ref()).into()
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(*members);
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                    && let Some(rendered) = self
                        .format_intersection_with_display(members.as_ref(), display_props.as_ref())
                {
                    return rendered.into();
                }
                self.format_intersection(members.as_ref()).into()
            }
            TypeData::Array(elem) => {
                if self.preserve_array_generic_form && !elem.is_intrinsic() {
                    let elem_formatted = self.format(*elem);
                    return format!("Array<{elem_formatted}>").into();
                }
                let elem_formatted = self.format(*elem);
                if self.requires_array_element_parens(*elem) {
                    format!("({elem_formatted})[]").into()
                } else {
                    format!("{elem_formatted}[]").into()
                }
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                self.format_tuple(elements.as_ref()).into()
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                self.format_function(shape.as_ref()).into()
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                // Check for a named symbol (e.g. ObjectConstructor, SymbolConstructor)
                // before falling back to structural expansion.
                if let Some(sym_id) = shape.symbol
                    && let Some(name) = self.format_symbol_name(sym_id)
                {
                    // Class constructor types (callables with construct signatures
                    // linked to a class symbol) should display as "typeof ClassName"
                    // to match tsc behavior. The class instance type displays as
                    // just "ClassName". A class merged with a same-named namespace
                    // keeps its class symbol on the rebuilt static shape, so this
                    // branch renders the merged static side as "typeof C" too.
                    if !shape.construct_signatures.is_empty()
                        && let Some(arena) = self.symbol_arena
                        && let Some(sym) = arena.get(sym_id)
                        && sym.has_flags(tsz_binder::symbol_flags::CLASS)
                    {
                        return format!("typeof {name}").into();
                    }
                    return name.into();
                }
                self.format_callable(shape.as_ref()).into()
            }
            TypeData::TypeParameter(info) => Cow::Owned(self.atom(info.name).to_string()),
            TypeData::UnresolvedTypeName(name) => {
                if self.atom(*name).as_ref() == "BuiltinIteratorReturn"
                    && let Some(replacement) = self.builtin_iterator_return_type
                {
                    return self.format(replacement);
                }
                if let Some((def_id, body)) = self.skipped_type_alias_body_by_name(*name) {
                    return self.format_skipped_type_alias_body(def_id, body);
                }
                Cow::Owned(self.atom(*name).to_string())
            }
            TypeData::Lazy(def_id) => {
                if let Some(replacement) = self.builtin_iterator_return_type
                    && let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && def.kind == crate::def::DefKind::TypeAlias
                    && self.atom(def.name).as_ref() == "BuiltinIteratorReturn"
                {
                    return self.format(replacement);
                }
                if self.skip_type_alias_def_ids.contains(def_id)
                    && let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && def.kind == crate::def::DefKind::TypeAlias
                    && let Some(body) = def.body
                {
                    return self.format_skipped_type_alias_body(*def_id, body);
                }
                self.format_def_id_with_type_params(*def_id, "Lazy").into()
            }
            TypeData::Recursive(idx) => format!("Recursive({idx})").into(),
            TypeData::BoundParameter(idx) => format!("BoundParameter({idx})").into(),
            TypeData::Application(app) => {
                let app = self.interner.type_application(*app);
                let base_key = self.interner.lookup(app.base);

                trace!(
                    base_type_id = %app.base.0,
                    ?base_key,
                    args_count = app.args.len(),
                    "Formatting Application"
                );

                // When the base type has already been evaluated to a concrete
                // type (Array, Tuple, etc.), the type arguments are already
                // incorporated into the base.  Formatting the base directly
                // produces the correct display (e.g., `D<number>[]`); appending
                // the Application's args would duplicate them (producing
                // `D<number>[]<D<number>>`).
                if matches!(base_key, Some(TypeData::Array(_) | TypeData::Tuple(_))) {
                    return self.format(app.base);
                }

                // If the application's base resolved to an error type,
                // rendering `error<args>` produces unreadable cascades in
                // diagnostics (e.g. `error<error<error<...>>>`). Collapse to
                // the bare "error" token — the caller's parent diagnostic
                // already signals the underlying failure.
                if app.base == TypeId::ERROR || matches!(base_key, Some(TypeData::Error)) {
                    return Cow::Borrowed("error");
                }

                if let Some(evaluated) =
                    self.scalar_mapped_alias_application_display(type_id, app.base, &app.args)
                {
                    return self.format(evaluated);
                }

                if let Some(distributed) =
                    self.distributed_conditional_application_display(app.base, &app.args)
                {
                    return self.format(distributed);
                }

                // Special handling for Application(Lazy(def_id), args)
                // Format as "TypeName<Args>" instead of "Lazy(def_id)<Args>"
                let base_str: Cow<'_, str> = if let Some(TypeData::Lazy(def_id)) = base_key {
                    let name = self.format_def_id(def_id, "Lazy");
                    trace!(
                        def_id = %def_id.0,
                        name = %name,
                        "Application base resolved from DefId"
                    );
                    Cow::Owned(name)
                } else if let Some(TypeData::TypeQuery(sym)) = base_key {
                    // For Application(TypeQuery(sym), args) — class instantiation
                    // like D<string>. Display as "D<string>" not "typeof D<string>",
                    // since typeof X<T> is not valid TS syntax and this represents
                    // the instantiated class type.
                    if let Some(name) = self.resolve_symbol_ref_name(sym) {
                        Cow::Owned(name)
                    } else {
                        Cow::Owned(format!("Ref({})", sym.0))
                    }
                } else {
                    // Check if the base type has a named definition (e.g., an
                    // interface or class body that was registered in the def store).
                    // If so, use just the name — the Application's own args replace
                    // the type parameters.  Without this guard, `self.format(app.base)`
                    // would render `Name<TypeParamNames>` and the Application would
                    // then append `<Args>`, producing `Name<T, U><actual, args>`.
                    if let Some(def_store) = self.def_store
                        && let Some(def_id) = def_store.find_def_for_type(app.base)
                        && let Some(def) = def_store.get(def_id)
                    {
                        let name = self.format_def_name(&def);
                        trace!(
                            base_formatted = %name,
                            "Application base resolved via def_store (no type params)"
                        );
                        use crate::def::DefKind;
                        if matches!(
                            def.kind,
                            DefKind::Enum | DefKind::Namespace | DefKind::ClassConstructor
                        ) {
                            if name.contains('.') {
                                Cow::Owned(name)
                            } else {
                                Cow::Owned(format!("typeof {name}"))
                            }
                        } else {
                            Cow::Owned(name)
                        }
                    } else {
                        let formatted = self.format(app.base);
                        trace!(
                            base_formatted = %formatted,
                            "Application base formatted (not Lazy)"
                        );
                        formatted
                    }
                };

                // TSC shorthand: Array<T> -> T[], ReadonlyArray<T> -> readonly T[]
                // and Readonly<T[]> -> readonly T[].
                // Skipped in constraint context (preserve_array_generic_form).
                if app.args.len() == 1 && !self.preserve_array_generic_form {
                    let single_arg = app.args[0];
                    if base_str == "Array"
                        && self
                            .interner
                            .get_array_base_type()
                            .is_some_and(|array_base| app.base == array_base)
                    {
                        // Array<T> -> T[]
                        let elem_formatted = self.format(single_arg);
                        let result = if self.requires_array_element_parens(single_arg) {
                            format!("({elem_formatted})[]")
                        } else {
                            format!("{elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as array shorthand");
                        return result.into();
                    }
                    if base_str == "ReadonlyArray" {
                        // ReadonlyArray<T> -> readonly T[]
                        let elem_formatted = self.format(single_arg);
                        let result = if self.requires_array_element_parens(single_arg) {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as readonly array shorthand");
                        return result.into();
                    }
                    if base_str == "Readonly"
                        && let Some(TypeData::Array(elem)) = self.interner.lookup(single_arg)
                    {
                        // Readonly<T[]> -> readonly T[]
                        let elem_formatted = self.format(elem);
                        let result = if self.requires_array_element_parens(elem) {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as Readonly<T[]> shorthand");
                        return result.into();
                    }
                }

                // Elide trailing type arguments that equal their parameter's
                // default. tsc renders `AsyncIterable<number, any, any>` as
                // `AsyncIterable<number>` when the second and third type
                // parameters default to `any`. tsc only applies this to the
                // four iterable globals — see `typeReferenceToTypeNode` in
                // checker.ts: "Maybe we should do this for more types, but for
                // now we only elide type arguments that are identical to their
                // associated type parameters' defaults for `Iterable`,
                // `IterableIterator`, `AsyncIterable`, and
                // `AsyncIterableIterator` to provide backwards-compatible .d.ts
                // emit due to each now having three type parameters instead of
                // only one." Applying elision unconditionally would e.g. turn
                // `Generator<number, any, any>` into `Generator<number>`, which
                // tsc doesn't do.
                let should_elide_defaults = matches!(
                    base_str.as_ref(),
                    "Iterable" | "IterableIterator" | "AsyncIterable" | "AsyncIterableIterator"
                );
                // Load the base's declared type parameters. We need them in two
                // situations:
                //   1) `should_elide_defaults` — trim trailing args that equal
                //      their parameter defaults (for the 4 iterable globals).
                //   2) `app.args.len() < params.len()` — pad missing trailing
                //      args with their parameter defaults so tsc-style output
                //      shows all args (e.g. `Iterator<string>` renders as
                //      `Iterator<string, any, any>` when `TReturn = TNext = any`).
                let def_type_params: Option<Vec<TypeParamInfo>> =
                    if let Some(TypeData::Lazy(def_id)) = base_key {
                        self.def_store.and_then(|ds| ds.get_type_params(def_id))
                    } else if let Some(def_store) = self.def_store {
                        def_store
                            .find_def_for_type(app.base)
                            .and_then(|id| def_store.get_type_params(id))
                    } else {
                        None
                    };

                // Build the display arg list, padding missing trailing args
                // with their parameter defaults when available.
                let display_args: Vec<TypeId> = if let Some(params) = def_type_params.as_ref()
                    && params.len() > app.args.len()
                {
                    let mut out: Vec<TypeId> = app.args.to_vec();
                    for param in params.iter().skip(app.args.len()) {
                        // Only pad when the missing parameter carries a default;
                        // stop at the first parameter without a default.
                        let Some(default) = param.default else {
                            break;
                        };
                        out.push(default);
                    }
                    out
                } else {
                    app.args.to_vec()
                };

                let visible_arg_count = if let Some(params) = def_type_params.as_ref()
                    && should_elide_defaults
                    && params.len() == display_args.len()
                {
                    let mut n = display_args.len();
                    while n > 0 {
                        let idx = n - 1;
                        if let Some(default) = params[idx].default {
                            if display_args[idx] != default {
                                break;
                            }
                        } else if display_args[idx] != TypeId::ANY {
                            break;
                        }
                        n -= 1;
                    }
                    n
                } else {
                    display_args.len()
                };

                let previous_skip_application_display_alias_chase =
                    self.skip_application_display_alias_chase;
                if self.skip_application_alias_names && base_str.as_ref() == "Omit" {
                    self.skip_application_display_alias_chase = true;
                }
                let mut args: Vec<Cow<'static, str>> = display_args
                    .iter()
                    .take(visible_arg_count)
                    .map(|&arg| self.format(self.simplify_application_arg_for_display(arg)))
                    .collect();
                self.skip_application_display_alias_chase =
                    previous_skip_application_display_alias_chase;
                if base_str.as_ref() == "Defaultize"
                    && args.first().is_some_and(|arg| arg.len() > 120)
                {
                    for (idx, arg) in display_args
                        .iter()
                        .take(visible_arg_count)
                        .enumerate()
                        .skip(1)
                    {
                        if matches!(
                            self.interner.lookup(*arg),
                            Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
                        ) {
                            args[idx] = Cow::Borrowed("{ ...; }");
                        }
                    }
                }
                let result = if args.is_empty()
                    && matches!(
                        base_str.as_ref(),
                        "Iterable" | "IterableIterator" | "AsyncIterable" | "AsyncIterableIterator"
                    ) {
                    format!("{base_str}<any>")
                } else if args.is_empty() {
                    base_str.to_string()
                } else {
                    format!("{}<{}>", base_str, args.join(", "))
                };
                trace!(result = %result, "Application formatted");
                result.into()
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                self.format_conditional(cond.as_ref()).into()
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                self.format_mapped(mapped.as_ref()).into()
            }
            TypeData::IndexAccess(obj, idx) => {
                let resolved = self.resolve_concrete_index_access_for_display(type_id);
                if resolved != type_id {
                    return self.format(resolved);
                }
                // Homomorphic mapped indexed access simplification:
                // tsc displays `M[K]` for a homomorphic identity Mapped
                // `M = { [P in keyof X]: X[P] }` (e.g. `Partial<X>`,
                // `Readonly<X>`) as `X[K]`, with `| undefined` appended when
                // the mapped's optional modifier is `+`. The structural
                // mapped form still appears when M is formatted directly,
                // but in indexed-access position tsc collapses to the
                // simpler X[K] form.
                if let Some(simplified) =
                    self.try_format_homomorphic_mapped_index_access(*obj, *idx)
                {
                    return simplified.into();
                }
                let obj_for_display = self
                    .interner
                    .get_display_alias(*obj)
                    .filter(|&alias| {
                        matches!(self.interner.lookup(alias), Some(TypeData::Application(_)))
                    })
                    .unwrap_or(*obj);
                let obj_str = if obj_for_display == *obj
                    && matches!(self.interner.lookup(*obj), Some(TypeData::Mapped(_)))
                    && let Some(def_store) = self.def_store
                    && let Some(def_id) = def_store.find_def_for_type(*obj)
                    && let Some(def) = def_store.get(def_id)
                    && !def.type_params.is_empty()
                {
                    let params: Vec<String> = def
                        .type_params
                        .iter()
                        .map(|tp| self.atom(tp.name).to_string())
                        .collect();
                    format!("{}<{}>", self.format_def_name(&def), params.join(", "))
                } else {
                    self.format(obj_for_display).into_owned()
                };
                // Parenthesize the object when it's a union or intersection AND
                // the formatted string actually shows the compound form (contains
                // ` & ` or ` | `). Named type aliases like `Errors<T>` may be
                // stored as intersections internally but display as a single name.
                let needs_parens = matches!(
                    self.interner.lookup(*obj),
                    Some(TypeData::Union(_) | TypeData::Intersection(_))
                ) && (obj_str.contains(" & ") || obj_str.contains(" | "));
                if needs_parens {
                    format!("({obj_str})[{}]", self.format(*idx)).into()
                } else {
                    format!("{obj_str}[{}]", self.format(*idx)).into()
                }
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                self.format_template_literal(spans.as_ref()).into()
            }
            TypeData::TypeQuery(sym) => {
                // Check if the symbol is a namespace import (import * as X from "mod")
                // — tsc displays these as `typeof import("mod")` rather than `typeof X`.
                if let Some(arena) = self.symbol_arena
                    && let Some(symbol) = arena.get(SymbolId(sym.0))
                    && symbol.import_name.as_deref() == Some("*")
                    && let Some(ref module_specifier) = symbol.import_module
                {
                    let display_name = Self::strip_module_extension(
                        module_specifier
                            .strip_prefix("./")
                            .or_else(|| module_specifier.strip_prefix("../"))
                            .unwrap_or(module_specifier),
                    );
                    return format!("typeof import(\"{display_name}\")").into();
                }
                if let Some(arena) = self.symbol_arena
                    && let Some(symbol) = arena.get(SymbolId(sym.0))
                    && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                    && let Some(name) = self.format_symbol_name(SymbolId(sym.0))
                {
                    return name.into();
                }
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("Ref({})", sym.0)
                };
                // Enum member TypeQuery types: tsc resolves `typeof W.a` to the
                // enum member type `W.a` and displays without `typeof` prefix.
                // The `resolve_symbol_ref_name` qualifies with enum parents, so
                // a dot in the name reliably indicates an enum member reference.
                if name.contains('.') {
                    name.into()
                } else {
                    format!("typeof {name}").into()
                }
            }
            TypeData::KeyOf(operand) => {
                // `keyof null`, `keyof undefined`, and `keyof void` all
                // evaluate to `never`. tsc displays the reduced form, so
                // collapse to `never` whenever the operand evaluates there.
                // This catches both the direct intrinsic case and substituted
                // forms where a type parameter was bound to a nullish type.
                if matches!(*operand, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
                    || crate::evaluation::evaluate::evaluate_keyof(self.interner, *operand)
                        == TypeId::NEVER
                {
                    return self.format(TypeId::NEVER);
                }
                if *operand == TypeId::NEVER {
                    return self.format(crate::evaluation::evaluate::evaluate_keyof(
                        self.interner,
                        *operand,
                    ));
                }
                // For anonymous concrete object operands, evaluate `keyof` eagerly
                // so diagnostics show the literal key union (e.g. `"x"`) instead
                // of `keyof { x: number; }`. tsc only writes back `keyof <Name>`
                // when the operand has a user-visible name; anonymous shapes are
                // displayed as their evaluated `keyof` result.
                //
                // Skip:
                //   - named objects (preserve `keyof Foo`),
                //   - generic operands (a type parameter must remain visible),
                //   - arrays/tuples (`keyof T[]` widens to `number | "length" | ...`
                //     which is rarely useful in error text),
                //   - intrinsics (same reason).
                if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                    self.interner.lookup(*operand)
                {
                    let shape = self.interner.object_shape(shape_id);
                    if shape.symbol.is_none() {
                        let evaluated =
                            crate::evaluation::evaluate::evaluate_keyof(self.interner, *operand);
                        // Guard against the evaluator returning the same KeyOf node
                        // (e.g. when the operand cannot be reduced) — that would
                        // recurse infinitely through `format`.
                        if !matches!(self.interner.lookup(evaluated), Some(TypeData::KeyOf(_))) {
                            return self.format(evaluated);
                        }
                    }
                }
                // tsc distributes `keyof` over union and intersection of non-structural types:
                //   keyof (A | B)  →  keyof A & keyof B
                //   keyof (A & B)  →  keyof A | keyof B
                // This applies when the union/intersection members are opaque (type params,
                // named/lazy refs, or applications), not concrete structural types like `{}`.
                // Exception: if any member is a structural object or intrinsic, preserve the
                // undistributed form (e.g. `keyof (T & {})` stays as-is).
                // tsc preserves `keyof (T & {})` undistributed because the
                // empty-object intersection is a non-nullish constraint, not
                // a structural-shape contributor. Restrict the no-distribute
                // guard to that specific shape — generic intersections with
                // *any* structural member (e.g. `T & string`) still
                // distribute as before.
                let any_member_empty_object = |list_id: TypeListId| -> bool {
                    self.interner.type_list(list_id).iter().any(|&m| {
                        matches!(
                            self.interner.lookup(m),
                            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id))
                                if {
                                    let shape = self.interner.object_shape(shape_id);
                                    shape.properties.is_empty()
                                        && shape.string_index.is_none()
                                        && shape.number_index.is_none()
                                }
                        )
                    })
                };
                let distributed = match self.interner.lookup(*operand) {
                    Some(TypeData::Union(list_id)) if !any_member_empty_object(list_id) => {
                        let members = self.interner.type_list(list_id);
                        let parts: Vec<String> = members
                            .iter()
                            .map(|&m| {
                                let inner = self.format(m);
                                // Add parens around complex member types
                                let member_needs_parens = matches!(
                                    self.interner.lookup(m),
                                    Some(
                                        TypeData::Union(_)
                                            | TypeData::Intersection(_)
                                            | TypeData::Conditional(_)
                                    )
                                );
                                if member_needs_parens {
                                    format!("keyof ({inner})")
                                } else {
                                    format!("keyof {inner}")
                                }
                            })
                            .collect();
                        Some(parts.join(" & "))
                    }
                    Some(TypeData::Intersection(list_id)) if !any_member_empty_object(list_id) => {
                        let members = self.interner.type_list(list_id);
                        let parts: Vec<String> = members
                            .iter()
                            .map(|&m| {
                                let inner = self.format(m);
                                let member_needs_parens = matches!(
                                    self.interner.lookup(m),
                                    Some(
                                        TypeData::Union(_)
                                            | TypeData::Intersection(_)
                                            | TypeData::Conditional(_)
                                    )
                                );
                                if member_needs_parens {
                                    format!("keyof ({inner})")
                                } else {
                                    format!("keyof {inner}")
                                }
                            })
                            .collect();
                        Some(parts.join(" | "))
                    }
                    _ => None,
                };
                if let Some(s) = distributed {
                    return s.into();
                }
                // When we suppressed distribution because a member is structural,
                // format the intersection/union members individually so we don't
                // re-collapse `T & {}` into a body-equivalent alias like `QQ<T>`
                // via the formatter's alias-reverse-lookup.  tsc preserves the
                // user's spelling (`keyof (T & {})`) in error messages.
                let inline_compound = match self.interner.lookup(*operand) {
                    Some(TypeData::Union(list_id)) if any_member_empty_object(list_id) => {
                        Some((list_id, " | "))
                    }
                    Some(TypeData::Intersection(list_id)) if any_member_empty_object(list_id) => {
                        Some((list_id, " & "))
                    }
                    _ => None,
                };
                if let Some((list_id, sep)) = inline_compound {
                    let members = self.interner.type_list(list_id);
                    let parts: Vec<String> = members
                        .iter()
                        .map(|&m| self.format(m).into_owned())
                        .collect();
                    return format!("keyof ({})", parts.join(sep)).into();
                }
                let operand_str = self.format(*operand);
                let needs_parens = matches!(
                    self.interner.lookup(*operand),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::Function(_)
                            | TypeData::Callable(_)
                            | TypeData::Conditional(_)
                    )
                );
                if needs_parens {
                    format!("keyof ({operand_str})").into()
                } else {
                    format!("keyof {operand_str}").into()
                }
            }
            TypeData::ReadonlyType(inner) => format!("readonly {}", self.format(*inner)).into(),
            // NoInfer<T> is transparent at the outermost layer of the
            // displayed type — matching tsc, which strips a single outer
            // `NoInfer<>` wrapper but preserves nested `NoInfer<>` markers
            // (e.g. inside a union member or function return). `format()`
            // increments `current_depth` from 0 → 1 before delegating here,
            // so the top-level call sees `current_depth == 1` and inner
            // recursions see `>= 2`.
            TypeData::NoInfer(inner) => {
                if self.current_depth == 1 {
                    self.format(*inner)
                } else {
                    format!("NoInfer<{}>", self.format(*inner)).into()
                }
            }
            TypeData::UniqueSymbol(_) => Cow::Borrowed("unique symbol"),
            TypeData::Infer(info) => {
                let name = self.atom(info.name);
                if let Some(constraint) = info.constraint {
                    let constraint_str = self.format(constraint);
                    format!("infer {name} extends {constraint_str}").into()
                } else {
                    format!("infer {name}").into()
                }
            }
            TypeData::ThisType => Cow::Borrowed("this"),
            TypeData::StringIntrinsic { kind, type_arg } => {
                let kind_name = match kind {
                    StringIntrinsicKind::Uppercase => "Uppercase",
                    StringIntrinsicKind::Lowercase => "Lowercase",
                    StringIntrinsicKind::Capitalize => "Capitalize",
                    StringIntrinsicKind::Uncapitalize => "Uncapitalize",
                };
                format!("{}<{}>", kind_name, self.format(*type_arg)).into()
            }
            TypeData::Enum(def_id, _member_type) => {
                // Enum members should be qualified with their parent enum name
                // (e.g., `Foo.A` not just `A`). Try the symbol arena first, which
                // walks the parent chain and qualifies enum members correctly.
                // Use the definition's stored symbol_id (not the raw def_id) to
                // find the correct binder symbol.
                if let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && let Some(sym_raw) = def.symbol_id
                    && let Some(name) = self.format_symbol_name(SymbolId(sym_raw))
                {
                    return name.into();
                }
                // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
                // DefId and SymbolId are independent ID spaces. Using the raw
                // def_id.0 as a SymbolId would return the name of an unrelated
                // symbol, causing bugs like "Foo.A" displaying as "timeout.A".
                self.format_def_id(*def_id, "Enum").into()
            }
            TypeData::ModuleNamespace(sym) => {
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("module({})", sym.0)
                };
                let name = Self::strip_module_extension(&name);
                format!("typeof import(\"{name}\")").into()
            }
            TypeData::Error => Cow::Borrowed("error"),
        }
    }

    fn format_in_operator_record(&mut self, shape: &ObjectShape) -> Option<String> {
        if !shape.flags.contains(ObjectFlags::IN_OPERATOR_RECORD)
            || shape.properties.len() != 1
            || shape.string_index.is_some()
            || shape.number_index.is_some()
        {
            return None;
        }

        let prop = &shape.properties[0];
        if prop.type_id != TypeId::UNKNOWN || prop.optional || prop.is_method {
            return None;
        }

        let key = self.atom(prop.name);
        let key_display = if prop.is_symbol_named || key.parse::<f64>().is_ok() {
            key.to_string()
        } else {
            format!("\"{key}\"")
        };
        Some(format!("Record<{key_display}, unknown>"))
    }
}
