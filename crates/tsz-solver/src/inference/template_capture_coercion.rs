//! Constraint coercion for template-literal inference captures.
//!
//! Mirrors the constituent-coercion half of tsc's `inferToTemplateLiteralType`:
//! when a captured template segment flows into a type variable whose declared
//! constraint admits a non-string primitive, the capture is coerced to the
//! literal type of the highest-priority constituent it can inhabit. The
//! sibling rule for conditional-type `infer` patterns lives in
//! `evaluation::evaluate_rules::infer_pattern_template_match`; the number
//! round-trip comparison in both paths routes through the shared
//! `js_number_to_string` owner (the sibling intentionally accepts more on the
//! bigint side, mirroring tsc's `roundTripOnly = false` placeholder check).

use crate::types::{LiteralValue, TypeData, TypeId};

use super::infer::{InferenceContext, InferenceVar};

impl InferenceContext<'_> {
    /// Coerce a captured template segment to the declared constraint of its
    /// inference variable, mirroring tsc's `inferToTemplateLiteralType`.
    ///
    /// A `number` constituent captures the numeric literal only when the text
    /// round-trips through JS `Number::toString` (`isValidNumberString` with
    /// `roundTripOnly`), `bigint` similarly, and `boolean`/`null`/`undefined`
    /// match their exact intrinsic names. A `string` constituent (or no
    /// usable constituent) keeps the plain string-literal capture.
    pub(super) fn coerce_captured_template_segment(
        &mut self,
        infer_var: InferenceVar,
        captured: &str,
    ) -> TypeId {
        let Some(constraint) = self.get_declared_constraint(infer_var) else {
            return self.interner.literal_string(captured);
        };

        let union_list = crate::type_queries::get_union_members(self.interner, constraint);
        let members: &[TypeId] = union_list
            .as_deref()
            .unwrap_or(std::slice::from_ref(&constraint));

        // tsc skips the whole coercion when the constraint contains `string`
        // (`allTypeFlags & TypeFlags.String` in `inferToTemplateLiteralType`).
        if members.contains(&TypeId::STRING) {
            return self.interner.literal_string(captured);
        }

        // `isValidNumberString(captured, roundTripOnly)`.
        let round_trip_number = tsz_common::numeric::round_trip_js_number(captured);

        // tsc's reduceLeft chain expresses a fixed constituent priority;
        // lower rank wins. TemplateLiteral / StringMapping / Enum
        // constituents (and unresolved Lazy constraints) fall through to the
        // string capture, as does a text that matches no constituent.
        let mut best: Option<(u8, TypeId)> = None;
        for &member in members {
            let candidate: Option<(u8, TypeId)> = match member {
                TypeId::NUMBER => {
                    round_trip_number.map(|value| (1, self.interner.literal_number(value)))
                }
                TypeId::BIGINT => {
                    tsz_common::numeric::round_trip_js_bigint(captured).map(|(negative, digits)| {
                        (3, self.interner.literal_bigint_with_sign(negative, digits))
                    })
                }
                TypeId::BOOLEAN => Some((
                    5,
                    match captured {
                        "true" => TypeId::BOOLEAN_TRUE,
                        "false" => TypeId::BOOLEAN_FALSE,
                        _ => TypeId::BOOLEAN,
                    },
                )),
                TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE => {
                    let name = if member == TypeId::BOOLEAN_TRUE {
                        "true"
                    } else {
                        "false"
                    };
                    (captured == name).then_some((6, member))
                }
                TypeId::UNDEFINED => (captured == "undefined").then_some((7, member)),
                TypeId::NULL => (captured == "null").then_some((8, member)),
                _ => match self.interner.lookup(member) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        (self.interner.resolve_atom_ref(atom).as_ref() == captured)
                            .then_some((0, member))
                    }
                    Some(TypeData::Literal(LiteralValue::Number(n))) => {
                        (round_trip_number == Some(n.0)).then_some((2, member))
                    }
                    Some(TypeData::Literal(LiteralValue::BigInt(atom))) => {
                        (self.interner.resolve_atom_ref(atom).as_ref() == captured)
                            .then_some((4, member))
                    }
                    _ => None,
                },
            };
            if let Some((rank, result)) = candidate
                && best.is_none_or(|(existing, _)| rank < existing)
            {
                best = Some((rank, result));
            }
        }

        match best {
            Some((_, result)) => result,
            None => self.interner.literal_string(captured),
        }
    }
}
