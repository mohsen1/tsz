//! tsc `aliasSymbol` display policy: when a non-generic type alias must be
//! rendered as its underlying type rather than by its declared name.
//!
//! tsc attaches an `aliasSymbol` (and therefore renders the alias name) only to
//! *freshly-constructed* structural types — unions, intersections, objects,
//! arrays, tuples, functions, mapped, conditional, etc. A non-generic alias
//! whose body resolves to a shared singleton (`string`, `42`, `never`, …) — or
//! whose body is a *computed* operator (conditional / utility application /
//! indexed access / `keyof` / template-literal / string-mapping intrinsic) that
//! reduces to such a singleton — points at a shared result that carries no
//! alias symbol, so tsc displays the underlying type.
//!
//! This policy is shared by the solver's `TypeFormatter` and by the checker's
//! assignability-message formatter (through a query boundary) so the two
//! parallel diagnostic-display pipelines cannot drift on alias rendering.

use crate::construction::TypeDatabase;
use crate::def::{DefId, DefKind, DefinitionStore};
use crate::types::{TypeData, TypeId};

/// Returns the underlying `TypeId` to display in place of the non-generic type
/// alias named by `def_id`, or `None` when the alias should keep its declared
/// name (generic alias, or a body that resolves to a structural shape that tsc
/// stamps with the alias symbol).
///
/// Alias chains are followed (`type A = B; type B = string` → `string`); a
/// bounded visited count guards against cyclic alias definitions.
pub fn type_alias_displayed_as_underlying(
    interner: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    def_id: DefId,
) -> Option<TypeId> {
    let mut current_def = def_id;
    let mut seen = 0usize;
    loop {
        // Bound iteration to avoid spinning on a cyclic alias chain.
        seen += 1;
        if seen > 64 {
            return None;
        }

        let def = def_store.get(current_def)?;
        if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        let body = def.body?;
        if crate::visitor::is_intrinsic_or_literal_type(interner, body) {
            return Some(body);
        }
        // The checker stores the *evaluated* body for a non-generic alias and
        // flags it as "computed" when the declared body was a reducing operator
        // (a conditional or indexed access that resolved away, an intersection
        // that collapsed to a primitive union). tsc attaches no `aliasSymbol` to
        // those shared results, so render the underlying structural type —
        // `[string, number]`, `number[]`, `() => void`, … — rather than the
        // alias name, matching the checker-side display.
        //
        // Object-mentioning bodies (a bare object, or a union/intersection
        // containing one) are excluded: re-formatting such a shared shape
        // re-enters the reverse `find_def_for_type` lookup, which can repaint it
        // with an unrelated alias name. Those keep the existing display path.
        if def_store.is_computed_body(body)
            && !crate::type_queries::union_or_intersection_mentions_object(interner, body)
        {
            // A computed *application* body (a reducing-bodied utility
            // application such as `DeepReadonly<Config>`, issue #10914) must
            // render its *evaluated* structural result, not the raw `Name<Args>`
            // application form, so both display pipelines agree on `{ … }`.
            if matches!(interner.lookup(body), Some(TypeData::Application(_))) {
                return alias_resolved_body_underlying(interner, body);
            }
            return Some(body);
        }
        match interner.lookup(body) {
            Some(TypeData::Lazy(next_def)) => current_def = next_def,
            // Operators that never carry tsc's `aliasSymbol` onto their result.
            // A conditional or indexed access *resolves away* into its
            // branch/element, and `keyof`, template-literal, and the
            // `Uppercase`/`Lowercase`/… string-mapping intrinsics build their
            // result without ever stamping the enclosing alias onto it. tsc
            // therefore renders the *evaluated* underlying type for any resolved
            // shape — scalar, literal, `never`, union, object, tuple, or even
            // another computed type. The syntactic checks above never see this
            // because the body is e.g. `true extends true ? { a: 1 } : never` or
            // `keyof { a: 1 }`.
            Some(
                TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. },
            ) => return alias_resolved_body_underlying(interner, body),
            // A utility/generic application *does* propagate the alias symbol
            // onto a freshly-constructed structural result (`Pick<…>` → object,
            // `Array<number>`, `Extract<…>` → union all keep their alias name),
            // but a result that bottoms out at a shared scalar/literal/`never`
            // singleton drops it (`ReturnType<() => string>` → `string`). Only
            // collapse an application when it reduces to such a singleton.
            Some(TypeData::Application(_)) => {
                return alias_application_scalar_underlying(interner, body);
            }
            _ => return None,
        }
    }
}

/// Evaluate a computed alias body whose top-level operator never carries tsc's
/// `aliasSymbol` onto its result — a conditional, an indexed access, a `keyof`,
/// a template literal, or a string-mapping intrinsic — and return the resolved
/// underlying type to display in place of the alias name.
///
/// tsc renders the evaluated result for **any** resolved shape (scalar,
/// literal, `never`, union, object, tuple, function, or a still-generic
/// template literal), so this deliberately does not gate on the result being a
/// scalar. Returns `None` only when the body stays generic, errors, or a
/// conditional/indexed access fails to reduce — leaving a deferred node that is
/// no more informative than the alias name itself.
fn alias_resolved_body_underlying(interner: &dyn TypeDatabase, body: TypeId) -> Option<TypeId> {
    let evaluated = crate::evaluation::evaluate::evaluate_type(interner, body);
    if evaluated == TypeId::ERROR
        || crate::type_queries::contains_type_parameters_db(interner, evaluated)
    {
        return None;
    }
    // A conditional or indexed access that is still deferred after evaluation
    // never reduced (e.g. an unresolved operand): rendering the raw node would
    // not improve on the alias name, so keep the name.
    if matches!(
        interner.lookup(evaluated),
        Some(TypeData::Conditional(_) | TypeData::IndexAccess(_, _))
    ) {
        return None;
    }
    Some(evaluated)
}

/// Evaluate a utility/generic *application* alias body and return the underlying
/// type **only** when it collapses to a shared scalar/literal/`never` singleton
/// — the one case where tsc drops the application's alias name
/// (`ReturnType<() => string>` → `string`). Structural results
/// (object/array/tuple/union/…) keep the alias name because tsc stamps the alias
/// symbol onto the freshly-constructed application result.
///
/// Returns `None` when the body stays generic, structural, errors, or does not
/// change under evaluation, so those aliases keep their declared name.
fn alias_application_scalar_underlying(
    interner: &dyn TypeDatabase,
    body: TypeId,
) -> Option<TypeId> {
    let evaluated = crate::evaluation::evaluate::evaluate_type(interner, body);
    if evaluated == body
        || evaluated == TypeId::ERROR
        || crate::type_queries::contains_type_parameters_db(interner, evaluated)
    {
        return None;
    }
    (crate::visitor::is_primitive_type(interner, evaluated)
        || matches!(interner.lookup(evaluated), Some(TypeData::Literal(_)))
        || evaluated == TypeId::NEVER)
        .then_some(evaluated)
}
