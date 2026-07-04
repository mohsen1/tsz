use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};
use crate::types::{CallSignature, CallableShape, FunctionShape, ObjectShape, TypeData, TypeId};

/// Which literal kinds an annotation-position display widening pass rewrites.
///
/// Mirrors the historical per-diagnostic display policies: most assignability
/// messages widen string/number/boolean literal annotations, the TS2345
/// generic-parameter display widens only strings and booleans, and the TS2820
/// target display widens only numbers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AnnotationLiteralWideningPolicy {
    /// Widen string literal annotations (`a: "x"` → `a: string`).
    pub widen_strings: bool,
    /// Widen number literal annotations (`a: 1` → `a: number`).
    pub widen_numbers: bool,
    /// Widen boolean literal annotations (`a: true` → `a: boolean`).
    pub widen_booleans: bool,
    /// Restrict widening to object shapes nested inside generic type
    /// applications (`Foo<{ a: "x" }>` → `Foo<{ a: string }>`); annotations
    /// outside an application's type arguments are preserved.
    pub inside_application_args_only: bool,
}

impl AnnotationLiteralWideningPolicy {
    /// Widen every literal annotation kind anywhere in the type.
    pub const ALL: Self = Self {
        widen_strings: true,
        widen_numbers: true,
        widen_booleans: true,
        inside_application_args_only: false,
    };

    /// Widen string/boolean literal annotations of objects nested inside
    /// generic type applications only (TS2345 generic parameter display).
    pub const STRINGS_AND_BOOLEANS_INSIDE_APPLICATION_ARGS: Self = Self {
        widen_strings: true,
        widen_numbers: false,
        widen_booleans: true,
        inside_application_args_only: true,
    };

    const fn widens(&self, value: &crate::LiteralValue) -> bool {
        match value {
            crate::LiteralValue::String(_) => self.widen_strings,
            crate::LiteralValue::Number(_) => self.widen_numbers,
            crate::LiteralValue::Boolean(_) => self.widen_booleans,
            crate::LiteralValue::BigInt(_) => false,
        }
    }

    fn widen_boolean_intrinsic(&self, type_id: TypeId) -> bool {
        self.widen_booleans && (type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE)
    }
}

/// Traversal mode for [`widen_annotation_literals_for_display`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum AnnotationWidenMode {
    /// Widening literal annotations in this region of the type.
    Active,
    /// Looking for a generic type application; nothing widens until one is
    /// entered (policy `inside_application_args_only`).
    SeekApplication,
    /// Inside a type application's arguments, looking for an object shape;
    /// widening activates inside the first object encountered.
    SeekObjectInArgs,
}

/// Result of [`widen_annotation_literals_for_display`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AnnotationWideningOutcome {
    /// The (possibly rebuilt) type whose plain rendering shows widened
    /// literal annotations.
    pub type_id: TypeId,
    /// `true` when some literal annotation spelling lives only in
    /// fresh-object-literal *display properties* (provenance keyed to a
    /// `TypeId` whose canonical shape is already widened, so no rebuild can
    /// change what it prints). The caller must render without display
    /// properties (e.g. `format_type_diagnostic_widened`) to show the
    /// widened form.
    pub display_residue: bool,
}

/// Traversal state for [`widen_annotation_literals_for_display`].
struct AnnotationWidenState<'e> {
    cache: rustc_hash::FxHashMap<(TypeId, AnnotationWidenMode), TypeId>,
    display_residue: bool,
    /// Display-time evaluation for leading annotation positions: a generic
    /// application can evaluate to a literal and then *render* as that
    /// literal, so the evaluated form is what a text rewrite saw. `None`
    /// (no resolver available) leaves such positions unchanged.
    evaluate_for_display: Option<&'e dyn Fn(TypeId) -> TypeId>,
}

/// Widen literal types that occur in *annotation positions* of a type's
/// rendered display — object property types, method return types, function
/// parameter and `this` annotations, index-signature value types, and labeled
/// tuple element types — to their primitive forms, returning a `TypeId` whose
/// plain rendering shows the widened annotations.
///
/// This is the type-level replacement for the checker's historical
/// byte-walking display rewriters, which scanned rendered diagnostic text for
/// `": <literal>"` sequences (issue #13075). Positions that render *without* a
/// leading colon — the top-level type itself, bare union/intersection members,
/// unlabeled tuple elements, bare application arguments, and non-method
/// function return types (`() => 1`) — are deliberately left unchanged, so the
/// reprinted display matches what positional text rewriting produced.
///
/// Display provenance: rebuilt unions keep their member display order, and
/// rebuilt compounds re-attach display aliases. Fresh-object-literal display
/// properties are widened alongside the canonical shape when the shape is
/// rebuilt; an object whose canonical shape is already fully widened is
/// returned unchanged (its display provenance belongs to the original
/// `TypeId` and must not be clobbered globally).
pub fn widen_annotation_literals_for_display(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
) -> AnnotationWideningOutcome {
    widen_annotation_literals_entry(db, type_id, policy, None)
}

/// Like [`widen_annotation_literals_for_display`], with a resolver for
/// display-time evaluation of leading annotation positions (generic
/// applications that evaluate to literals and render as such).
pub fn widen_annotation_literals_for_display_resolved<R: crate::def::resolver::TypeResolver>(
    db: &dyn crate::construction::TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
) -> AnnotationWideningOutcome {
    let evaluate = |t: TypeId| crate::diagnostics::reduce::deep_reduce_for_display(db, resolver, t);
    widen_annotation_literals_entry(db, type_id, policy, Some(&evaluate))
}

fn widen_annotation_literals_entry(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
    evaluate_for_display: Option<&dyn Fn(TypeId) -> TypeId>,
) -> AnnotationWideningOutcome {
    let mode = if policy.inside_application_args_only {
        AnnotationWidenMode::SeekApplication
    } else {
        AnnotationWidenMode::Active
    };
    let mut st = AnnotationWidenState {
        cache: rustc_hash::FxHashMap::default(),
        display_residue: false,
        evaluate_for_display,
    };
    let widened = widen_annotation_walk(db, type_id, mode, policy, &mut st);
    AnnotationWideningOutcome {
        type_id: widened,
        display_residue: st.display_residue,
    }
}

/// Widen one annotation-position type: a literal of an enabled kind widens to
/// its primitive; anything else keeps walking with widening active.
fn widen_annotation_position(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    mode: AnnotationWidenMode,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> TypeId {
    if mode != AnnotationWidenMode::Active {
        return widen_annotation_walk(db, type_id, mode, policy, st);
    }
    let leading = annotation_leading_literal_widen(db, type_id, policy, false, st);
    if leading != type_id {
        return leading;
    }
    let walked = widen_annotation_walk(db, type_id, mode, policy, st);
    widen_annotation_union_first_display_member(db, walked, policy, st)
}

/// Widen the literal that *leads* an annotation's rendered text.
///
/// The historical text rewrite consumed a quoted string unconditionally but
/// required a boundary byte (`;`, `,`, `}`, `>`, `)`, `|`, `&`, `]`, space)
/// after a number or `true`/`false`. An array render starts with its element
/// (`"no"[]` / `12[]`), and `[` is not a boundary: string-literal elements
/// widen (`string[]`) while number/boolean literal elements are preserved.
fn annotation_leading_literal_widen(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
    in_array: bool,
    st: &AnnotationWidenState<'_>,
) -> TypeId {
    if !in_array && policy.widen_boolean_intrinsic(type_id) {
        return TypeId::BOOLEAN;
    }
    if type_id.is_intrinsic() {
        return type_id;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(ref value)) if policy.widens(value) => {
            if in_array && !matches!(value, crate::LiteralValue::String(_)) {
                type_id
            } else {
                value.primitive_type_id()
            }
        }
        Some(TypeData::Array(elem)) => {
            let widened = annotation_leading_literal_widen(db, elem, policy, true, st);
            if widened == elem {
                type_id
            } else {
                db.array(widened)
            }
        }
        // A generic application can evaluate to a literal (e.g. a homomorphic
        // mapped alias over a literal argument) and then *render* as that
        // literal: widen what actually prints. Adopt the evaluated form only
        // when its leading literal widens, so non-literal evaluations leave
        // the original (alias-surfaced) type untouched.
        Some(TypeData::Application(_)) => {
            if let Some(evaluate) = st.evaluate_for_display {
                let evaluated = evaluate(type_id);
                if evaluated != type_id {
                    let widened =
                        annotation_leading_literal_widen(db, evaluated, policy, in_array, st);
                    if widened != evaluated {
                        return widened;
                    }
                }
            }
            type_id
        }
        _ => type_id,
    }
}

/// Widen the *leading rendered* member of a union in annotation position:
/// that member immediately follows the `": "` in the rendered text, so the
/// historical rewrite widened it (`12 | undefined` → `number | undefined`,
/// `"no"[] | undefined` → `string[] | undefined`) while later members kept
/// their literal spellings.
///
/// The formatter renders `null`/`undefined` members last, so the rule is
/// applied only in the unambiguous shape — exactly one non-nullish member
/// (the optional-property pattern) — where that member is certainly the
/// leading render. Unions with several non-nullish members are left
/// unchanged: their display order is owned by the formatter's tiered
/// ordering and cannot be predicted here.
///
/// The rebuilt union is adopted only when its canonical member *set* equals
/// the mapped set — no subsumption collapse — so the result renders as
/// intended without recording union-origin provenance on a possibly shared
/// `TypeId`. (Canonical member order may differ; the formatter owns union
/// display ordering.)
fn widen_annotation_union_first_display_member(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    policy: AnnotationLiteralWideningPolicy,
    st: &AnnotationWidenState<'_>,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
        return type_id;
    };
    let members = db.type_list(list_id);
    let mut non_nullish = members
        .iter()
        .copied()
        .filter(|&member| member != TypeId::UNDEFINED && member != TypeId::NULL);
    let Some(leading) = non_nullish.next() else {
        return type_id;
    };
    if non_nullish.next().is_some() {
        return type_id;
    }
    let widened_leading = annotation_leading_literal_widen(db, leading, policy, false, st);
    if widened_leading == leading {
        return type_id;
    }
    let mapped: Vec<TypeId> = members
        .iter()
        .map(|&member| {
            if member == leading {
                widened_leading
            } else {
                member
            }
        })
        .collect();
    let rebuilt = db.union_from_slice(&mapped);
    match db.lookup(rebuilt) {
        Some(TypeData::Union(new_list)) => {
            let new_members = db.type_list(new_list);
            let same_set = new_members.len() == mapped.len()
                && mapped.iter().all(|member| new_members.contains(member));
            if same_set { rebuilt } else { type_id }
        }
        _ => type_id,
    }
}

/// Propagate the display alias of `original` onto `widened`, widening the
/// alias surface itself: an alias application like `ListProps<{ a: "x" }>`
/// prints its own type arguments, so those must be widened alongside the
/// structural shape they label.
fn propagate_widened_annotation_alias(
    db: &dyn crate::construction::TypeDatabase,
    original: TypeId,
    widened: TypeId,
    mode: AnnotationWidenMode,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) {
    if original != widened
        && let Some(alias) = display_provenance::display_alias(db, original)
    {
        let alias_widened = widen_annotation_walk(db, alias, mode, policy, st);
        display_provenance::record_alias_application(
            db,
            AliasApplicationProvenance {
                evaluated: widened,
                application: alias_widened,
            },
            AliasApplicationPriority::PreserveExisting,
        );
    }
}

fn widen_annotation_object_shape(
    db: &dyn crate::construction::TypeDatabase,
    shape: &ObjectShape,
    mode: AnnotationWidenMode,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> Option<ObjectShape> {
    let mut new_shape: Option<ObjectShape> = None;
    for (index, prop) in shape.properties.iter().enumerate() {
        let (widened, widened_write) = if mode == AnnotationWidenMode::Active {
            (
                widen_annotation_property_type(db, prop.type_id, prop.is_method, policy, st),
                widen_annotation_property_type(db, prop.write_type, prop.is_method, policy, st),
            )
        } else {
            (
                widen_annotation_walk(db, prop.type_id, mode, policy, st),
                widen_annotation_walk(db, prop.write_type, mode, policy, st),
            )
        };
        if widened != prop.type_id || widened_write != prop.write_type {
            let target = new_shape.get_or_insert_with(|| shape.clone());
            target.properties[index].type_id = widened;
            target.properties[index].write_type = widened_write;
        }
    }

    if mode == AnnotationWidenMode::Active {
        if let Some(index) = shape.string_index {
            let widened = widen_annotation_position(db, index.value_type, mode, policy, st);
            if widened != index.value_type {
                let target = new_shape.get_or_insert_with(|| shape.clone());
                if let Some(target_index) = &mut target.string_index {
                    target_index.value_type = widened;
                }
            }
        }
        if let Some(index) = shape.number_index {
            let widened = widen_annotation_position(db, index.value_type, mode, policy, st);
            if widened != index.value_type {
                let target = new_shape.get_or_insert_with(|| shape.clone());
                if let Some(target_index) = &mut target.number_index {
                    target_index.value_type = widened;
                }
            }
        }
        if let Some(index) = shape.symbol_index {
            let widened = widen_annotation_position(db, index.value_type, mode, policy, st);
            if widened != index.value_type {
                let target = new_shape.get_or_insert_with(|| shape.clone());
                if let Some(target_index) = &mut target.symbol_index {
                    target_index.value_type = widened;
                }
            }
        }
    }

    new_shape
}

fn widen_annotation_signature_fields(
    db: &dyn crate::construction::TypeDatabase,
    params: &[crate::ParamInfo],
    this_type: Option<TypeId>,
    return_type: TypeId,
    return_is_method: bool,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> Option<(Vec<crate::ParamInfo>, Option<TypeId>, TypeId)> {
    let mode = AnnotationWidenMode::Active;
    let mut changed = false;
    let mut new_params: Option<Vec<crate::ParamInfo>> = None;
    for (index, param) in params.iter().enumerate() {
        let widened = widen_annotation_position(db, param.type_id, mode, policy, st);
        if widened != param.type_id {
            let target = new_params.get_or_insert_with(|| params.to_vec());
            target[index].type_id = widened;
            changed = true;
        }
    }

    let widened_this = if let Some(this_ty) = this_type {
        let widened = widen_annotation_position(db, this_ty, mode, policy, st);
        changed |= widened != this_ty;
        Some(widened)
    } else {
        None
    };
    let widened_return = if return_is_method {
        widen_annotation_position(db, return_type, mode, policy, st)
    } else {
        widen_annotation_walk(db, return_type, mode, policy, st)
    };
    changed |= widened_return != return_type;

    if changed {
        Some((
            new_params.unwrap_or_else(|| params.to_vec()),
            widened_this,
            widened_return,
        ))
    } else {
        None
    }
}

fn widen_annotation_function_shape(
    db: &dyn crate::construction::TypeDatabase,
    shape: &FunctionShape,
    return_is_method: bool,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> Option<FunctionShape> {
    widen_annotation_signature_fields(
        db,
        &shape.params,
        shape.this_type,
        shape.return_type,
        return_is_method,
        policy,
        st,
    )
    .map(|(params, this_type, return_type)| {
        let mut new_shape = shape.clone();
        new_shape.params = params;
        new_shape.this_type = this_type;
        new_shape.return_type = return_type;
        new_shape
    })
}

fn widen_annotation_call_signature(
    db: &dyn crate::construction::TypeDatabase,
    sig: &CallSignature,
    return_is_method: bool,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> Option<CallSignature> {
    widen_annotation_signature_fields(
        db,
        &sig.params,
        sig.this_type,
        sig.return_type,
        return_is_method,
        policy,
        st,
    )
    .map(|(params, this_type, return_type)| {
        let mut new_sig = sig.clone();
        new_sig.params = params;
        new_sig.this_type = this_type;
        new_sig.return_type = return_type;
        new_sig
    })
}

fn widen_annotation_callable_shape(
    db: &dyn crate::construction::TypeDatabase,
    shape: &CallableShape,
    force_method_returns: bool,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> Option<CallableShape> {
    let mut new_shape: Option<CallableShape> = None;
    for (index, sig) in shape.call_signatures.iter().enumerate() {
        let return_is_method = force_method_returns || sig.is_method;
        if let Some(widened) =
            widen_annotation_call_signature(db, sig, return_is_method, policy, st)
        {
            new_shape
                .get_or_insert_with(|| shape.clone())
                .call_signatures[index] = widened;
        }
    }
    for (index, sig) in shape.construct_signatures.iter().enumerate() {
        let return_is_method = force_method_returns || sig.is_method;
        if let Some(widened) =
            widen_annotation_call_signature(db, sig, return_is_method, policy, st)
        {
            new_shape
                .get_or_insert_with(|| shape.clone())
                .construct_signatures[index] = widened;
        }
    }
    for (index, prop) in shape.properties.iter().enumerate() {
        let widened = widen_annotation_property_type(db, prop.type_id, prop.is_method, policy, st);
        let widened_write =
            widen_annotation_property_type(db, prop.write_type, prop.is_method, policy, st);
        if widened != prop.type_id || widened_write != prop.write_type {
            let target = new_shape.get_or_insert_with(|| shape.clone());
            target.properties[index].type_id = widened;
            target.properties[index].write_type = widened_write;
        }
    }
    if let Some(index) = shape.string_index {
        let widened = widen_annotation_position(
            db,
            index.value_type,
            AnnotationWidenMode::Active,
            policy,
            st,
        );
        if widened != index.value_type {
            let target = new_shape.get_or_insert_with(|| shape.clone());
            if let Some(target_index) = &mut target.string_index {
                target_index.value_type = widened;
            }
        }
    }
    if let Some(index) = shape.number_index {
        let widened = widen_annotation_position(
            db,
            index.value_type,
            AnnotationWidenMode::Active,
            policy,
            st,
        );
        if widened != index.value_type {
            let target = new_shape.get_or_insert_with(|| shape.clone());
            if let Some(target_index) = &mut target.number_index {
                target_index.value_type = widened;
            }
        }
    }

    new_shape
}

/// Structural walk for [`widen_annotation_literals_for_display`]: descends
/// into compounds rebuilding only what changed, widening literals solely
/// through [`widen_annotation_position`].
fn widen_annotation_walk(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    mode: AnnotationWidenMode,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if let Some(&cached) = st.cache.get(&(type_id, mode)) {
        return cached;
    }
    // Cycle sentinel: a self-referential type widens to itself.
    st.cache.insert((type_id, mode), type_id);

    let result = match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let mode = match mode {
                AnnotationWidenMode::SeekObjectInArgs => AnnotationWidenMode::Active,
                other => other,
            };
            let shape = db.object_shape(shape_id);
            let new_shape = widen_annotation_object_shape(db, shape.as_ref(), mode, policy, st);
            // Fresh-object-literal *display properties* (literal spellings
            // recorded as provenance) print instead of the canonical shape,
            // so widen those spellings too.
            let widened_display = if mode == AnnotationWidenMode::Active {
                db.get_display_properties(type_id).map(|display_props| {
                    let mut widened_display = display_props.as_ref().clone();
                    let mut display_changed = false;
                    for prop in &mut widened_display {
                        let widened = widen_annotation_property_type(
                            db,
                            prop.type_id,
                            prop.is_method,
                            policy,
                            st,
                        );
                        let widened_write = widen_annotation_property_type(
                            db,
                            prop.write_type,
                            prop.is_method,
                            policy,
                            st,
                        );
                        display_changed |=
                            widened != prop.type_id || widened_write != prop.write_type;
                        prop.type_id = widened;
                        prop.write_type = widened_write;
                    }
                    (widened_display, display_changed)
                })
            } else {
                None
            };
            let display_changed = widened_display
                .as_ref()
                .is_some_and(|(_, display_changed)| *display_changed);
            if let Some(new_shape) = new_shape {
                let symbol = new_shape.symbol;
                let flags = new_shape.flags;
                let widened_id = if new_shape.string_index.is_some()
                    || new_shape.number_index.is_some()
                    || new_shape.symbol_index.is_some()
                {
                    db.object_with_index(new_shape)
                } else {
                    db.object_with_flags_and_symbol(new_shape.properties, flags, symbol)
                };
                if let Some((display_properties, _)) = widened_display {
                    display_provenance::record_fresh_object_literal_display(
                        db,
                        FreshObjectLiteralDisplayProvenance {
                            type_id: widened_id,
                            properties: display_properties,
                        },
                    );
                }
                widened_id
            } else {
                if display_changed {
                    // The canonical shape is already fully widened: the
                    // literal spellings live only in display provenance keyed
                    // to this `TypeId`, and rebuilding interns back to the
                    // same id. No type-level rewrite can change what this id
                    // prints; report the residue so the caller renders
                    // without display properties.
                    st.display_residue = true;
                }
                type_id
            }
        }

        // Arrow renders (`(x: "a") => 1`) put parameters and `this` in
        // annotation positions but the return type after `=>`, so bare
        // literal returns are preserved unless the shape is a method
        // (`m(): 1` renders with a colon).
        Some(TypeData::Function(shape_id)) if mode == AnnotationWidenMode::Active => {
            let shape = db.function_shape(shape_id);
            if let Some(new_shape) =
                widen_annotation_function_shape(db, shape.as_ref(), shape.is_method, policy, st)
            {
                db.function(new_shape)
            } else {
                type_id
            }
        }

        Some(TypeData::Callable(shape_id)) if mode == AnnotationWidenMode::Active => {
            let shape = db.callable_shape(shape_id);
            if let Some(new_shape) =
                widen_annotation_callable_shape(db, shape.as_ref(), false, policy, st)
            {
                db.callable(new_shape)
            } else {
                type_id
            }
        }

        // Bare union/intersection members render without a leading colon, so
        // member literals stay; only annotations nested inside members widen.
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let origin_members = db.get_union_origin(type_id);
            let display_members = origin_members
                .as_deref()
                .map_or(members.as_ref(), Vec::as_slice);
            let mapped: Vec<TypeId> = display_members
                .iter()
                .map(|&m| widen_annotation_walk(db, m, mode, policy, st))
                .collect();
            if mapped == display_members {
                type_id
            } else {
                let widened = db.union_from_slice(&mapped);
                display_provenance::record_union_origin(
                    db,
                    UnionOriginProvenance {
                        union_type_id: widened,
                        origin_members: mapped,
                    },
                );
                widened
            }
        }

        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members
                .iter()
                .map(|&m| widen_annotation_walk(db, m, mode, policy, st))
                .collect();
            if mapped.as_slice() == members.as_ref() {
                type_id
            } else {
                db.intersection(mapped)
            }
        }

        Some(TypeData::Array(element_type)) => {
            let widened = widen_annotation_walk(db, element_type, mode, policy, st);
            if widened == element_type {
                type_id
            } else {
                db.array(widened)
            }
        }

        Some(TypeData::Tuple(tuple_list_id)) => {
            let elements = db.tuple_list(tuple_list_id);
            let mut new_elements = Vec::with_capacity(elements.len());
            let mut changed = false;
            for elem in elements.iter() {
                // Labeled elements render as `[x: 1]` (annotation position);
                // unlabeled elements render bare (`[1, 2]`) and keep literals.
                let widened = if elem.name.is_some() {
                    widen_annotation_position(db, elem.type_id, mode, policy, st)
                } else {
                    widen_annotation_walk(db, elem.type_id, mode, policy, st)
                };
                changed |= widened != elem.type_id;
                let mut new_elem = *elem;
                new_elem.type_id = widened;
                new_elements.push(new_elem);
            }
            if changed {
                db.tuple(new_elements)
            } else {
                type_id
            }
        }

        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            let arg_mode = match mode {
                AnnotationWidenMode::SeekApplication => AnnotationWidenMode::SeekObjectInArgs,
                other => other,
            };
            let mapped: Vec<TypeId> = app
                .args
                .iter()
                .map(|&arg| widen_annotation_walk(db, arg, arg_mode, policy, st))
                .collect();
            if mapped == app.args {
                type_id
            } else {
                db.application(app.base, mapped)
            }
        }

        // Everything else (literals at non-annotation positions, lazy refs,
        // mapped/conditional/template forms, type parameters, enums, ...) is
        // preserved: either it renders without literal annotations or its
        // rendering is owned by a name, not its structure.
        _ => type_id,
    };

    let result = if result == type_id {
        // The structure did not change, but the type may still print through
        // a display-alias surface (e.g. an evaluated generic application)
        // whose rendered type arguments carry literal annotations. The alias
        // application is itself printable, so return it widened.
        match display_provenance::display_alias(db, type_id) {
            Some(alias) if alias != type_id => {
                let alias_widened = widen_annotation_walk(db, alias, mode, policy, st);
                if alias_widened == alias {
                    type_id
                } else {
                    alias_widened
                }
            }
            _ => type_id,
        }
    } else {
        propagate_widened_annotation_alias(db, type_id, result, mode, policy, st);
        result
    };

    st.cache.insert((type_id, mode), result);
    result
}

/// Widen an object property's type: the property annotation itself is an
/// annotation position; method properties additionally place their return
/// type in annotation position (`m(): R`).
fn widen_annotation_property_type(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    is_method_property: bool,
    policy: AnnotationLiteralWideningPolicy,
    st: &mut AnnotationWidenState<'_>,
) -> TypeId {
    if !is_method_property || type_id.is_intrinsic() {
        return widen_annotation_position(db, type_id, AnnotationWidenMode::Active, policy, st);
    }
    // Method property: force the method-return annotation rule regardless of
    // the inner shape's own `is_method` flag, mirroring the `m(): R` render
    // of method properties.
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            if let Some(new_shape) =
                widen_annotation_function_shape(db, shape.as_ref(), true, policy, st)
            {
                let widened_fn = db.function(new_shape);
                propagate_widened_annotation_alias(
                    db,
                    type_id,
                    widened_fn,
                    AnnotationWidenMode::Active,
                    policy,
                    st,
                );
                widened_fn
            } else {
                type_id
            }
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if let Some(new_shape) =
                widen_annotation_callable_shape(db, shape.as_ref(), true, policy, st)
            {
                let widened_callable = db.callable(new_shape);
                propagate_widened_annotation_alias(
                    db,
                    type_id,
                    widened_callable,
                    AnnotationWidenMode::Active,
                    policy,
                    st,
                );
                widened_callable
            } else {
                type_id
            }
        }
        _ => widen_annotation_position(db, type_id, AnnotationWidenMode::Active, policy, st),
    }
}
