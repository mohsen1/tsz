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
use crate::types::{LiteralValue, TupleElement, TypeData, TypeId};

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

/// The literal kind of a packed tuple element, mirroring the candidate kinds
/// `isLiteralOfContextualType` distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralKind {
    String,
    Number,
    BigInt,
    Boolean,
    UniqueSymbol,
}

fn literal_kind_of(db: &dyn TypeDatabase, type_id: TypeId) -> Option<LiteralKind> {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return Some(LiteralKind::Boolean);
    }
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(value)) => Some(match value {
            LiteralValue::String(_) => LiteralKind::String,
            LiteralValue::Number(_) => LiteralKind::Number,
            LiteralValue::BigInt(_) => LiteralKind::BigInt,
            LiteralValue::Boolean(_) => LiteralKind::Boolean,
        }),
        Some(TypeData::UniqueSymbol(_)) => Some(LiteralKind::UniqueSymbol),
        _ => None,
    }
}

const MAX_CONSTRAINT_DEPTH: u32 = 8;

/// The element of `constraint` a rest argument at position `index` is checked
/// against — the base constraint of the deferred indexed access `T[index]`.
///
/// `constraint` is the declared `extends` clause of the rest type parameter
/// (`string[]`, `[string, number]`, `Array<string>`, a union of those, …).
fn constraint_element_at(
    db: &dyn TypeDatabase,
    constraint: TypeId,
    index: usize,
    depth: u32,
) -> Option<TypeId> {
    if depth > MAX_CONSTRAINT_DEPTH || constraint.is_intrinsic() {
        return None;
    }
    match db.lookup(constraint)? {
        TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
            constraint_element_at(db, inner, index, depth + 1)
        }
        TypeData::Array(element) => Some(element),
        TypeData::Tuple(elems_id) => {
            let elems = db.tuple_list(elems_id);
            for (position, elem) in elems.iter().enumerate() {
                if elem.rest {
                    // The variadic span absorbs every remaining index; use its
                    // element type (`E` for `...E[]`, the spread type itself
                    // for `...T` so the kind check can recurse its constraint).
                    return match db.lookup(elem.type_id) {
                        Some(TypeData::Array(element)) => Some(element),
                        _ => Some(elem.type_id),
                    };
                }
                if position == index {
                    return Some(elem.type_id);
                }
            }
            None
        }
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            // `Array<E>` / `ReadonlyArray<E>` style applications: a
            // single-argument application over an array-like base is the only
            // shape whose element is recoverable without a resolver.
            crate::type_queries::get_array_element_type(db, constraint).or_else(|| {
                let args = &app.args;
                (args.len() == 1).then(|| args[0])
            })
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            // `maybeTypeOfKind` is an any-constituent test: build the union of
            // member elements and let the kind check scan its constituents.
            let members = db.type_list(list_id);
            let elements: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| constraint_element_at(db, m, index, depth + 1))
                .collect();
            match elements.len() {
                0 => None,
                1 => Some(elements[0]),
                _ => Some(db.union_from_slice(&elements)),
            }
        }
        TypeData::TypeParameter(info) => info
            .constraint
            .and_then(|c| constraint_element_at(db, c, index, depth + 1)),
        _ => None,
    }
}

/// `isLiteralOfContextualType` over the base constraint element (checker.ts):
/// does a literal of `kind` keep its literal type against contextual element
/// `ctx`?
fn constraint_element_preserves_literal(
    db: &dyn TypeDatabase,
    ctx: TypeId,
    kind: LiteralKind,
    mode: SpreadRestLiteralMode,
    depth: u32,
) -> bool {
    if depth > MAX_CONSTRAINT_DEPTH {
        return false;
    }
    // Bare primitive constituents preserve their literal kind only while the
    // indexed access stays instantiable (tsc's `InstantiableNonPrimitive`
    // branch); once instantiated to a concrete type they widen.
    if mode == SpreadRestLiteralMode::Unfixed {
        let primitive_matches = match kind {
            LiteralKind::String => ctx == TypeId::STRING,
            LiteralKind::Number => ctx == TypeId::NUMBER,
            LiteralKind::BigInt => ctx == TypeId::BIGINT,
            LiteralKind::Boolean => ctx == TypeId::BOOLEAN,
            LiteralKind::UniqueSymbol => ctx == TypeId::SYMBOL,
        };
        if primitive_matches {
            return true;
        }
    }
    if ctx == TypeId::BOOLEAN_TRUE || ctx == TypeId::BOOLEAN_FALSE {
        return kind == LiteralKind::Boolean;
    }
    if ctx.is_intrinsic() {
        return false;
    }
    match db.lookup(ctx) {
        // Literal-flavored constituents preserve in both modes (tsc's concrete
        // branch: StringLiteral | Index | TemplateLiteral | StringMapping, and
        // the sibling literal kinds).
        Some(TypeData::Literal(value)) => {
            kind == match value {
                LiteralValue::String(_) => LiteralKind::String,
                LiteralValue::Number(_) => LiteralKind::Number,
                LiteralValue::BigInt(_) => LiteralKind::BigInt,
                LiteralValue::Boolean(_) => LiteralKind::Boolean,
            }
        }
        Some(TypeData::UniqueSymbol(_)) => kind == LiteralKind::UniqueSymbol,
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => {
            kind == LiteralKind::String
        }
        Some(TypeData::KeyOf(_)) => {
            // An unevaluated `keyof` carries tsc's `Index` flag, which
            // preserves string literals in both modes. When it evaluates to a
            // concrete union (`keyof any` → `string | number | symbol`), the
            // evaluated constituents decide instead — under `Unfixed` the
            // `string` member still preserves; once contextually fixed the
            // bare primitives widen, matching tsc.
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, ctx);
            if evaluated == ctx {
                return kind == LiteralKind::String;
            }
            constraint_element_preserves_literal(db, evaluated, kind, mode, depth + 1)
        }
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&m| constraint_element_preserves_literal(db, m, kind, mode, depth + 1)),
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .is_some_and(|c| constraint_element_preserves_literal(db, c, kind, mode, depth + 1)),
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
    let mut new_elems: Vec<TupleElement> = Vec::with_capacity(elems.len());
    let mut changed = false;
    for (index, elem) in elems.iter().enumerate() {
        let widened = if elem.rest {
            // A spread segment (`f("a", ...rest)`) is pushed through
            // unchanged by tsc; its element types were never fresh literals.
            elem.type_id
        } else if let Some(kind) = literal_kind_of(db, elem.type_id) {
            let preserved = declared_constraint.is_some_and(|constraint| {
                constraint_element_at(db, constraint, index, 0)
                    .is_some_and(|ctx| constraint_element_preserves_literal(db, ctx, kind, mode, 0))
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
        changed |= widened != elem.type_id;
        let mut new_elem = *elem;
        new_elem.type_id = widened;
        new_elems.push(new_elem);
    }
    if changed { db.tuple(new_elems) } else { tuple }
}
