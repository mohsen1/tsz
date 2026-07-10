//! Literal preservation for tuples packed from trailing rest arguments.
//!
//! When a call site packs trailing arguments into a synthetic tuple to infer a
//! generic rest parameter (`function f<T extends string[]>(...args: T)`), tsc
//! decides *per element* whether a literal argument keeps its literal type or
//! widens to its primitive (`getSpreadArgumentType`, checker.ts). The decision
//! consults the per-index contextual type `T[i]`:
//!
//! * While `T` is **unfixed** (no outer contextual type has fixed it), `T[i]`
//!   stays an instantiable indexed access and `isLiteralOfContextualType`
//!   consults its base constraint: a `string` constituent preserves string
//!   literals, `number` preserves numeric literals, `boolean` preserves
//!   boolean literals, and literal-flavored constituents (literal unions,
//!   `keyof`, template literals, string-mapping intrinsics) preserve their
//!   matching literal kinds.
//! * When an outer contextual type has **fixed** `T` before the arguments are
//!   packed, `T[i]` instantiates to a concrete type and only literal-flavored
//!   constituents preserve; bare primitives (`string`, `number`, `boolean`)
//!   no longer do.
//!
//! Elements that do not preserve widen exactly as tsz's inference resolution
//! widened them before: literals to their primitive, fresh object literals
//! deeply (tsc's `getWidenedType` at `getInferredType` time).

use crate::construction::TypeDatabase;
use crate::types::{TupleElement, TypeData, TypeId};

/// How tsc would treat the per-index contextual type `T[i]` when deciding
/// whether a literal rest argument keeps its literal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpreadRestLiteralMode {
    /// The rest type parameter is not fixed by an outer contextual type:
    /// `T[i]` stays instantiable and its base constraint decides, with bare
    /// primitive constituents preserving matching literal kinds.
    Unfixed,
    /// An outer contextual type fixes the rest type parameter before the
    /// arguments are packed: `T[i]` instantiates to a concrete type where
    /// only literal-flavored constituents preserve literals.
    ContextuallyFixed,
}

/// The primitive a literal type widens to (`"a"` → `string`, `1` → `number`,
/// `true` → `boolean`, unique symbols → `symbol`), or `None` for non-literal
/// types. This is the candidate-kind key `isLiteralOfContextualType` matches
/// constraint constituents against.
pub(crate) fn literal_primitive_of(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return Some(TypeId::BOOLEAN);
    }
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(value)) => Some(value.primitive_type_id()),
        Some(TypeData::UniqueSymbol(_)) => Some(TypeId::SYMBOL),
        _ => None,
    }
}

const MAX_CONSTRAINT_DEPTH: u32 = 8;

/// Whether any per-index element of `constraint` at `index` preserves a
/// literal whose widened primitive is `prim` — tsc's
/// `isLiteralOfContextualType` over the base constraint element behind the
/// deferred indexed access `T[index]`.
///
/// `constraint` is the declared `extends` clause of the rest type parameter
/// (`string[]`, `[string, number]`, `Array<string>`, a union of those, …);
/// `count` is the packed tuple's element count, needed to map trailing
/// indices onto the fixed suffix of a variadic tuple constraint like
/// `[...T, string]`. Distributes over unions/intersections and type-parameter
/// constraint chains without materializing intermediate types.
fn constraint_preserves_literal_at(
    db: &dyn TypeDatabase,
    constraint: TypeId,
    index: usize,
    count: usize,
    prim: TypeId,
    mode: SpreadRestLiteralMode,
    depth: u32,
) -> bool {
    if depth > MAX_CONSTRAINT_DEPTH || constraint.is_intrinsic() {
        return false;
    }
    let Some(data) = db.lookup(constraint) else {
        return false;
    };
    match data {
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            constraint_preserves_literal_at(db, inner, index, count, prim, mode, depth + 1)
        }
        TypeData::Array(element) => element_preserves_literal(db, element, prim, mode, depth + 1),
        TypeData::Tuple(elems_id) => {
            let elems = db.tuple_list(elems_id);
            let Some(element) = tuple_constraint_element_at(db, &elems, index, count) else {
                return false;
            };
            element_preserves_literal(db, element, prim, mode, depth + 1)
        }
        // `Array<E>` / `ReadonlyArray<E>` style applications resolve through
        // the canonical array-element query; non-array generics never
        // preserve a bare literal element.
        TypeData::Application(_) => crate::type_queries::get_array_element_type(db, constraint)
            .is_some_and(|element| element_preserves_literal(db, element, prim, mode, depth + 1)),
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => db
            .type_list(list_id)
            .iter()
            .any(|&m| constraint_preserves_literal_at(db, m, index, count, prim, mode, depth + 1)),
        TypeData::TypeParameter(info) => info.constraint.is_some_and(|c| {
            constraint_preserves_literal_at(db, c, index, count, prim, mode, depth + 1)
        }),
        _ => false,
    }
}

/// The constraint tuple element a packed argument at `index` (of `count`
/// trailing arguments) is checked against: fixed prefix positionally, fixed
/// suffix from the end, the variadic span for everything in between.
fn tuple_constraint_element_at(
    db: &dyn TypeDatabase,
    elems: &[TupleElement],
    index: usize,
    count: usize,
) -> Option<TypeId> {
    let rest_pos = elems.iter().position(|elem| elem.rest);
    let Some(rest_pos) = rest_pos else {
        return elems.get(index).map(|elem| elem.type_id);
    };
    if index < rest_pos {
        return Some(elems[index].type_id);
    }
    let suffix = &elems[rest_pos + 1..];
    // `index < count` always (the packed tuple has `count` elements), so this
    // single guard also bounds `from_end` within the suffix.
    if index + suffix.len() >= count {
        let from_end = count - index;
        return Some(suffix[suffix.len() - from_end].type_id);
    }
    let rest_inner = elems[rest_pos].type_id;
    match db.lookup(rest_inner) {
        // `...E[]` — the element type; `...T` — the spread type itself so the
        // kind check can recurse its constraint.
        Some(TypeData::Array(element)) => Some(element),
        _ => Some(rest_inner),
    }
}

/// `isLiteralOfContextualType` for a single constraint *element* (the type
/// behind `T[i]`): does it preserve a literal whose widened primitive is
/// `prim`? Unlike [`constraint_preserves_literal_at`], containers like
/// `Array` are ordinary non-preserving types here.
fn element_preserves_literal(
    db: &dyn TypeDatabase,
    ctx: TypeId,
    prim: TypeId,
    mode: SpreadRestLiteralMode,
    depth: u32,
) -> bool {
    if depth > MAX_CONSTRAINT_DEPTH {
        return false;
    }
    // Bare primitive constituents preserve their literal kind only while the
    // indexed access stays instantiable (tsc's `InstantiableNonPrimitive`
    // branch); once instantiated to a concrete type they widen.
    if mode == SpreadRestLiteralMode::Unfixed && ctx == prim {
        return true;
    }
    // Literal-flavored constituents preserve in both modes (tsc's concrete
    // branch: literals, `Index`, `TemplateLiteral`, `StringMapping`).
    if let Some(ctx_prim) = literal_primitive_of(db, ctx) {
        return ctx_prim == prim;
    }
    if ctx.is_intrinsic() {
        return false;
    }
    match db.lookup(ctx) {
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => {
            prim == TypeId::STRING
        }
        Some(TypeData::KeyOf(operand)) => {
            // An unevaluated `keyof` carries tsc's `Index` flag, which
            // preserves string literals in both modes; that covers the
            // generic operand case (`(keyof T)[]`) without evaluation. A
            // concrete operand evaluates to its key union, whose
            // constituents decide instead — under `Unfixed` a `string`
            // member still preserves; once contextually fixed the bare
            // primitives widen, matching tsc.
            if matches!(
                db.lookup(operand),
                Some(TypeData::TypeParameter(_) | TypeData::IndexAccess(_, _))
            ) {
                return prim == TypeId::STRING;
            }
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, ctx);
            if evaluated == ctx {
                return prim == TypeId::STRING;
            }
            element_preserves_literal(db, evaluated, prim, mode, depth + 1)
        }
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&m| element_preserves_literal(db, m, prim, mode, depth + 1)),
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .is_some_and(|c| element_preserves_literal(db, c, prim, mode, depth + 1)),
        _ => false,
    }
}

/// Re-widen a spread-built rest tuple the way tsc's `getSpreadArgumentType` +
/// `getWidenedType` pipeline leaves it: literal elements whose per-index
/// constraint element preserves them stay literal; every other element widens
/// exactly as tsz's blanket inference widening did before.
///
/// `declared_constraint` is the rest type parameter's `extends` clause. With
/// no constraint every literal widens, preserving the previous behavior.
pub(crate) fn widen_spread_rest_tuple(
    db: &dyn TypeDatabase,
    tuple: TypeId,
    declared_constraint: Option<TypeId>,
    mode: SpreadRestLiteralMode,
) -> TypeId {
    let Some(TypeData::Tuple(elems_id)) = db.lookup(tuple) else {
        return crate::operations::widening::widen_type_for_inference(db, tuple);
    };
    let elems = db.tuple_list(elems_id);
    let count = elems.len();
    // Copy-on-first-change: the fully-preserved tuple (the feature's target
    // case) allocates nothing and returns its own id.
    let mut new_elems: Option<Vec<TupleElement>> = None;
    for (index, elem) in elems.iter().enumerate() {
        let widened = if elem.rest {
            // A spread segment (`f("a", ...rest)`) is pushed through
            // unchanged by tsc; its element types were never fresh literals.
            elem.type_id
        } else if let Some(prim) = literal_primitive_of(db, elem.type_id) {
            let preserved = declared_constraint.is_some_and(|constraint| {
                constraint_preserves_literal_at(db, constraint, index, count, prim, mode, 0)
            });
            if preserved {
                elem.type_id
            } else {
                crate::operations::widening::widen_literal_type(db, elem.type_id)
            }
        } else {
            // Non-literal elements (fresh object literals, arrays) keep the
            // deep widening tsc applies via `getWidenedType`.
            crate::operations::widening::widen_type_for_inference(db, elem.type_id)
        };
        if widened != elem.type_id && new_elems.is_none() {
            let mut prefix = Vec::with_capacity(count);
            prefix.extend_from_slice(&elems[..index]);
            new_elems = Some(prefix);
        }
        if let Some(rebuilt) = new_elems.as_mut() {
            let mut new_elem = *elem;
            new_elem.type_id = widened;
            rebuilt.push(new_elem);
        }
    }
    match new_elems {
        Some(rebuilt) => db.tuple(rebuilt),
        None => tuple,
    }
}
