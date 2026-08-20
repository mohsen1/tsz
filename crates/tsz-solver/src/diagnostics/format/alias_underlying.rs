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
        // A non-generic alias whose tuple body was built by flattening a
        // fixed-tuple spread (`type T = [...[a, b], c]`) carries no
        // `aliasSymbol` in tsc — the spread produces a fresh tuple — so render
        // the structural tuple (`[a, b, c]`) rather than the alias name. The
        // flag is keyed per def because the flattened tuple interns to the same
        // shape as a directly-written `type T = [a, b, c]`, which keeps its name.
        if def_store.is_tuple_spread_flattened_alias(current_def) {
            return Some(body);
        }
        // A non-generic alias whose declared body was a *bare* reference to a
        // non-generic interface or class resolves to the declaration's shared
        // nominal type, which never carries tsc's `aliasSymbol`, so tsc
        // renders the declaration's own name (`type IA = Iface` renders
        // `Iface`; `type CA = Cls` renders `Cls`). The checker records the
        // referenced declaration at body publication because the stored body
        // may have flattened to the declaration's structural shape (class
        // instance types and alias chains do), erasing the reference; render
        // the declaration's deferred ref, which formats as its name.
        if let Some(target_def) = def_store.bare_nominal_ref_alias_target(current_def) {
            return Some(interner.lazy(target_def));
        }
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
        // Operators that never carry tsc's `aliasSymbol` onto their result.
        // A conditional or indexed access *resolves away* into its
        // branch/element, and `keyof`, template-literal, and the
        // `Uppercase`/`Lowercase`/… string-mapping intrinsics build their
        // result without ever stamping the enclosing alias onto it. tsc
        // therefore renders the *evaluated* underlying type for any resolved
        // shape — scalar, literal, `never`, union, object, tuple, or even
        // another computed type. The syntactic checks above never see this
        // because the body is e.g. `true extends true ? { a: 1 } : never` or
        // `keyof { a: 1 }`. The operator set is shared with the generic-alias
        // gate via `type_queries::is_name_dropping_reducing_operator`.
        if crate::type_queries::is_name_dropping_reducing_operator(interner, body) {
            return alias_resolved_body_underlying(interner, body);
        }
        match interner.lookup(body) {
            // A bare reference to an enum or an enum member resolves to the
            // declaration's shared nominal type, which never carries tsc's
            // `aliasSymbol`. tsc therefore renders the declaration's own name
            // (`Mode`, `Mode.A`, or the bare enum name for a single-member
            // enum) instead of the alias name. An alias-to-alias reference
            // keeps following the chain.
            Some(TypeData::Lazy(next_def)) => {
                if is_enum_or_enum_member_ref(def_store, next_def) {
                    return Some(body);
                }
                // A bare reference to a *non-generic* interface or class also
                // resolves to the declaration's shared nominal type, which
                // never carries tsc's `aliasSymbol`: `type IA = Iface` renders
                // `Iface`, `type CA = Cls` renders `Cls`. A generic
                // declaration (even fully defaulted, `class GC[T = string]`;
                // `type GCA = GC`) instantiates a fresh reference that keeps
                // the alias symbol, so those keep the alias name.
                if is_non_generic_interface_or_class_ref(def_store, next_def) {
                    return Some(body);
                }
                current_def = next_def;
            }
            // An already-evaluated enum or enum-member body (`TypeData::Enum`)
            // is the same shared nominal type in evaluated form; render it
            // under its own name for the same reason.
            Some(TypeData::Enum(_, _)) => return Some(body),
            // A utility/generic application's display depends on the head alias'
            // declared body. A *conditional*-bodied utility loses tsc's alias
            // symbol once the conditional reduces, so the evaluated result is
            // rendered structurally for any concrete shape (`Reverse<[1, 2, 3]>`
            // → `[3, 2, 1]`, `Unbox<Promise<Promise<number>>>` → `number`,
            // `Box<1>` → `{ v: 1; }`). A mapped/object-bodied utility keeps its
            // alias symbol on a freshly-constructed structural result (`Pick<…>`
            // → object, `Array<number>`) and drops it only when the result
            // bottoms out at a shared scalar singleton (`ReturnType<…>` is itself
            // conditional-bodied; a mapped utility reducing to a primitive is the
            // residual case here).
            Some(TypeData::Application(_)) => {
                return alias_application_underlying(interner, def_store, body);
            }
            _ => return None,
        }
    }
}

/// True when `def_id` names an enum declaration or an enum member. Members are
/// identified by their parent-enum edge, which is how member defs are keyed
/// regardless of the `DefKind` they were stabilized under. A type alias whose
/// body is a bare reference to one of these points at the shared nominal enum
/// type, so it renders the declaration's own name.
///
fn is_enum_or_enum_member_ref(def_store: &DefinitionStore, def_id: DefId) -> bool {
    if def_store.get_enum_parent(def_id).is_some() {
        return true;
    }
    def_store
        .get(def_id)
        .is_some_and(|def| def.kind == DefKind::Enum)
}

/// True when `def_id` names a *non-generic* interface or class declaration. A
/// type alias whose body is a bare reference to one of these points at the
/// declaration's shared nominal type — which never carries tsc's
/// `aliasSymbol` — so the alias renders under the declaration's own name
/// (`type IA = Iface` renders `Iface`). Generic declarations are excluded:
/// referencing one (with explicit arguments, or bare when every parameter is
/// defaulted) builds a fresh instantiation that keeps the alias symbol, so
/// those aliases keep their declared name.
fn is_non_generic_interface_or_class_ref(def_store: &DefinitionStore, def_id: DefId) -> bool {
    def_store.get(def_id).is_some_and(|def| {
        matches!(def.kind, DefKind::Interface | DefKind::Class) && def.type_params.is_empty()
    })
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
/// type to display in place of the alias name.
///
/// tsc's `aliasSymbol` policy splits on the head alias' declared body:
/// * A **conditional**-bodied utility (`Reverse`, `Unbox`, `ReturnType`,
///   `Parameters`, `Box<T> = T extends … ? … : …`) loses its alias symbol once
///   the conditional reduces, so tsc renders the evaluated result structurally
///   for any concrete shape — scalar (`number`), tuple (`[3, 2, 1]`), array
///   (`1[]`), `never`, or object (`{ v: 1; }`). See
///   [`application_reduces_to_displayable_shape`] for the shapes covered; bare
///   literal / union results are excluded because tsc applies literal-union
///   display widening to those (a separate display concern).
/// * A **mapped/object**-bodied utility (`Pick`, `Partial`, `Record`) keeps its
///   alias symbol on a freshly-constructed structural result and drops it only
///   when the result bottoms out at a shared scalar/`never` singleton.
///
/// Returns `None` when the body stays generic, errors, or does not change under
/// evaluation, so those aliases keep their declared name.
fn alias_application_underlying(
    interner: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    body: TypeId,
) -> Option<TypeId> {
    let evaluated = crate::evaluation::evaluate::evaluate_type(interner, body);
    if evaluated == body
        || evaluated == TypeId::ERROR
        || crate::type_queries::contains_type_parameters_db(interner, evaluated)
    {
        return None;
    }
    // tsc only drops the alias symbol once the application is fully instantiated;
    // a still-generic application stays deferred as `Name<Args>` even when the
    // display-time evaluator collapses an unconstrained conditional to `never`.
    if crate::type_queries::application_base_has_conditional_alias_body(interner, def_store, body)
        && !crate::type_queries::contains_type_parameters_db(interner, body)
        && application_reduces_to_displayable_shape(interner, evaluated)
    {
        return Some(evaluated);
    }
    (crate::visitor::is_primitive_type(interner, evaluated) || evaluated == TypeId::NEVER)
        .then_some(evaluated)
}

/// The set of evaluated shapes that a reduced conditional-bodied alias
/// application is rendered as structurally (tsc drops the alias symbol):
/// object/mapped, tuple, array, a primitive keyword, or `never`.
///
/// Two carve-outs keep the rendering honest:
/// * A result that still contains a nested `Recursive` node is a
///   **non-converged** recursive reduction. Expanding it renders a truncated
///   cycle (`Reverse<[1, 2, 3]>` would print `[...[...[......, 3], 2], 1]`), so
///   the alias name is kept — it is strictly clearer than the garbage form.
///   (A nested `Lazy` alias reference is *not* a disqualifier: a resolved
///   member such as `{ x: Named }` is fine to render.)
/// * Bare **literal** and **union** results are excluded: tsc applies
///   literal-union widening when displaying a fresh conditional result in those
///   cases (`Extract<"a" | "b" | 1, string>` shows `string`, not `"a" | "b"`),
///   which is a separate display behavior. Keeping those on the application
///   surface avoids substituting one divergence for another.
pub fn application_reduces_to_displayable_shape(
    interner: &dyn TypeDatabase,
    evaluated: TypeId,
) -> bool {
    // A non-converged recursive reduction leaves a nested `Recursive` cycle
    // marker that renders as a truncated cycle; keep the alias name instead.
    if crate::visitor::contains_type_matching(interner, evaluated, |key| {
        matches!(key, TypeData::Recursive(_))
    }) {
        return false;
    }
    if evaluated == TypeId::NEVER
        || crate::type_queries::is_object_or_mapped_type(interner, evaluated)
        || crate::type_queries::mapped::is_array_or_tuple_type(interner, evaluated)
    {
        return true;
    }
    // A primitive keyword (`string`, `number`, `boolean`, …) but not a unit
    // literal, which the comment above keeps out of scope.
    crate::visitor::is_primitive_type(interner, evaluated)
        && !matches!(interner.lookup(evaluated), Some(TypeData::Literal(_)))
}

/// When `type_id` is an application of a generic type alias whose body merely
/// applies another generic base over the alias's own parameters — declared
/// order (`type Fwd<X, Y> = Pair<X, Y>`), permuted (`type Flip<X, Y> =
/// Pair<Y, X>`), or repeated (`type Dup<T> = Pair<T, T>`) — return the
/// underlying application with the outer arguments carried into the body's
/// argument positions, composing the remap across a chain of such aliases
/// (fuel-bounded).
///
/// This is `tsc`'s normalized-source rendering for the nested lines of a
/// failed relation: the headline keeps the written alias application
/// (`Flip<A, B>`), while each nested member frame re-enters the relation with
/// the source's alias erased, rendering the underlying application
/// (`Pair<B, A>`). A body argument that is any other shape — a compound
/// mentioning a parameter (`T[]`) or a concrete type (`Pair<X, number>`) —
/// declines the hop rather than risking a wrong alignment, mirroring the
/// inference-side `alias_forwarded_application` rule.
pub fn forwarded_alias_application_display_view(
    interner: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    let mut current = type_id;
    for _ in 0..4 {
        let Some(TypeData::Application(app_id)) = interner.lookup(current) else {
            break;
        };
        let app = interner.type_application(app_id);
        let Some(next) = forwarded_alias_application_hop(interner, def_store, app.base, &app.args)
        else {
            break;
        };
        current = next;
    }
    (current != type_id).then_some(current)
}

/// One hop of [`forwarded_alias_application_display_view`]: rewrite
/// `base<args>` through `base`'s alias body when that body is an application
/// over the alias's own parameters, matched by binder identity.
fn forwarded_alias_application_hop(
    interner: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    base: TypeId,
    args: &[TypeId],
) -> Option<TypeId> {
    let Some(TypeData::Lazy(def_id)) = interner.lookup(base) else {
        return None;
    };
    let def = def_store.get(def_id)?;
    if def.kind != DefKind::TypeAlias || def.type_params.is_empty() {
        return None;
    }
    let body = def.body?;
    let Some(TypeData::Application(body_app_id)) = interner.lookup(body) else {
        return None;
    };
    let body_app = interner.type_application(body_app_id);
    if args.len() != def.type_params.len() {
        return None;
    }
    let mut mapped = Vec::with_capacity(body_app.args.len());
    for &body_arg in &body_app.args {
        let Some(TypeData::TypeParameter(tp)) = interner.lookup(body_arg) else {
            return None;
        };
        let position = def
            .type_params
            .iter()
            .position(|param| tp.is_same_binder(*param))?;
        mapped.push(args[position]);
    }
    Some(interner.application(body_app.base, mapped))
}
