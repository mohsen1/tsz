//! Interning identity (`PartialEq`/`Eq`/`Hash`) for the interned shape types
//! whose field membership is decided by hand: `PropertyInfo`,
//! `IndexSignature`, `ObjectShape`, and `CallableShape`.
//!
//! Every impl exhaustively destructures `Self`, binding identity-exempt
//! (cosmetic/display-only) fields as `field: _` with the reason stated at the
//! destructuring site. Adding a field to any of these structs is therefore a
//! compile error here until the field gets an explicit identity decision
//! (#13099). No `..` rest patterns are allowed in this module.
//!
//! Display-preserving exceptions live at the shape level: index-signature
//! `param_name` and (under `ObjectFlags::PRESERVE_DECLARATION_ORDER`) property
//! `declaration_order` are deliberately identity-bearing for `ObjectShape` /
//! `CallableShape` so diagnostics keep printing source spellings.

use super::{CallableShape, IndexSignature, ObjectFlags, ObjectShape, PropertyInfo};

impl PartialEq for PropertyInfo {
    fn eq(&self, other: &Self) -> bool {
        // Exhaustive destructuring: adding a field to `PropertyInfo` fails to
        // compile here until that field gets an explicit identity decision.
        let Self {
            name,
            type_id,
            write_type,
            optional,
            readonly,
            is_method,
            // Identity-exempt: declaration-site metadata (spread exclusion), not structural.
            is_class_prototype: _,
            visibility,
            parent_id,
            // Identity-exempt: display/emit source ordering; `ObjectShape` re-adds it
            // under `PRESERVE_DECLARATION_ORDER`.
            declaration_order: _,
            is_string_named,
            is_symbol_named,
            // Identity-exempt: cosmetic quote style for `.d.ts` output.
            single_quoted_name: _,
            // Identity-bearing: distinguishes a non-widening (regular) literal
            // property preserved from `as const`/assertion sources from an
            // otherwise structurally identical fresh-literal property that must
            // still widen. tsc encodes this as a fresh-vs-regular literal-type
            // split; tsz carries it on the property so the two intern apart.
            non_widening,
        } = self;
        *name == other.name
            && *type_id == other.type_id
            && *write_type == other.write_type
            && *optional == other.optional
            && *readonly == other.readonly
            && *is_method == other.is_method
            && *visibility == other.visibility
            && *parent_id == other.parent_id
            && *is_string_named == other.is_string_named
            && *is_symbol_named == other.is_symbol_named
            && *non_widening == other.non_widening
    }
}

impl Eq for PropertyInfo {}

impl std::hash::Hash for PropertyInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Exhaustive destructuring: keep field membership in lockstep with
        // `PartialEq` above; adding a field fails to compile here.
        let Self {
            name,
            type_id,
            write_type,
            optional,
            readonly,
            is_method,
            // Identity-exempt: declaration-site metadata (spread exclusion), not structural.
            is_class_prototype: _,
            visibility,
            parent_id,
            // Identity-exempt: display/emit source ordering; `ObjectShape` re-adds it
            // under `PRESERVE_DECLARATION_ORDER`.
            declaration_order: _,
            is_string_named,
            is_symbol_named,
            // Identity-exempt: cosmetic quote style for `.d.ts` output.
            single_quoted_name: _,
            // Identity-bearing: see the `PartialEq` impl above.
            non_widening,
        } = self;
        name.hash(state);
        type_id.hash(state);
        write_type.hash(state);
        optional.hash(state);
        readonly.hash(state);
        is_method.hash(state);
        visibility.hash(state);
        parent_id.hash(state);
        is_string_named.hash(state);
        is_symbol_named.hash(state);
        non_widening.hash(state);
    }
}

impl PartialEq for IndexSignature {
    fn eq(&self, other: &Self) -> bool {
        // Exhaustive destructuring: adding a field to `IndexSignature` fails to
        // compile here until that field gets an explicit identity decision.
        let Self {
            key_type,
            value_type,
            readonly,
            // Identity-exempt here: cosmetic source parameter name. `ObjectShape` /
            // `CallableShape` re-add it via `index_signature_display_eq`, so display
            // identity is preserved at the shape level.
            param_name: _,
        } = self;
        *key_type == other.key_type
            && *value_type == other.value_type
            && *readonly == other.readonly
    }
}

impl Eq for IndexSignature {}

impl std::hash::Hash for IndexSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Exhaustive destructuring: keep field membership in lockstep with
        // `PartialEq` above; adding a field fails to compile here.
        let Self {
            key_type,
            value_type,
            readonly,
            // Identity-exempt here: cosmetic; re-added at the shape level by
            // `hash_index_signature_display`.
            param_name: _,
        } = self;
        key_type.hash(state);
        value_type.hash(state);
        readonly.hash(state);
    }
}

/// Shape-level index-signature equality that re-adds the display `param_name`
/// on top of `IndexSignature`'s cosmetic-exempt equality, so interned shapes
/// keep the source parameter name for diagnostics.
fn index_signature_display_eq(
    left: &Option<IndexSignature>,
    right: &Option<IndexSignature>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right && left.param_name == right.param_name,
        (None, None) => true,
        _ => false,
    }
}

/// Hash counterpart of [`index_signature_display_eq`].
fn hash_index_signature_display<H: std::hash::Hasher>(
    index: &Option<IndexSignature>,
    state: &mut H,
) {
    std::hash::Hash::hash(index, state);
    std::hash::Hash::hash(&index.as_ref().and_then(|idx| idx.param_name), state);
}

impl PartialEq for ObjectShape {
    fn eq(&self, other: &Self) -> bool {
        // Exhaustive destructuring: adding a field to `ObjectShape` fails to
        // compile here until that field gets an explicit identity decision.
        // Every current field is identity-bearing: `symbol` for nominal
        // discrimination (the Solver does structural subtyping explicitly, not
        // via `PartialEq`), index signatures including their display
        // `param_name`, and `declaration_order` under
        // `PRESERVE_DECLARATION_ORDER`.
        let Self {
            flags,
            properties,
            string_index,
            number_index,
            symbol,
        } = self;
        *flags == other.flags
            && *properties == other.properties
            && (!flags.contains(ObjectFlags::PRESERVE_DECLARATION_ORDER)
                || properties
                    .iter()
                    .zip(&other.properties)
                    .all(|(left, right)| left.declaration_order == right.declaration_order))
            && index_signature_display_eq(string_index, &other.string_index)
            && index_signature_display_eq(number_index, &other.number_index)
            && *symbol == other.symbol
    }
}

impl Eq for ObjectShape {}

impl std::hash::Hash for ObjectShape {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Exhaustive destructuring: keep field membership in lockstep with
        // `PartialEq` above; adding a field fails to compile here.
        let Self {
            flags,
            properties,
            string_index,
            number_index,
            symbol,
        } = self;
        flags.hash(state);
        properties.hash(state);
        if flags.contains(ObjectFlags::PRESERVE_DECLARATION_ORDER) {
            for prop in properties {
                prop.declaration_order.hash(state);
            }
        }
        hash_index_signature_display(string_index, state);
        hash_index_signature_display(number_index, state);
        symbol.hash(state);
    }
}

impl PartialEq for CallableShape {
    fn eq(&self, other: &Self) -> bool {
        // Exhaustive destructuring: adding a field to `CallableShape` fails to
        // compile here until that field gets an explicit identity decision.
        // Every current field is identity-bearing: `symbol` for nominal
        // discrimination (the Solver does structural subtyping explicitly, not
        // via `PartialEq`) and index signatures including their display
        // `param_name`.
        let Self {
            call_signatures,
            construct_signatures,
            properties,
            string_index,
            number_index,
            symbol,
            is_abstract,
        } = self;
        *call_signatures == other.call_signatures
            && *construct_signatures == other.construct_signatures
            && *properties == other.properties
            && index_signature_display_eq(string_index, &other.string_index)
            && index_signature_display_eq(number_index, &other.number_index)
            && *symbol == other.symbol
            && *is_abstract == other.is_abstract
    }
}

impl Eq for CallableShape {}

impl std::hash::Hash for CallableShape {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Exhaustive destructuring: keep field membership in lockstep with
        // `PartialEq` above; adding a field fails to compile here.
        let Self {
            call_signatures,
            construct_signatures,
            properties,
            string_index,
            number_index,
            symbol,
            is_abstract,
        } = self;
        call_signatures.hash(state);
        construct_signatures.hash(state);
        properties.hash(state);
        hash_index_signature_display(string_index, state);
        hash_index_signature_display(number_index, state);
        symbol.hash(state);
        is_abstract.hash(state);
    }
}
