//! String intrinsic type evaluation.
//!
//! Handles TypeScript's string manipulation intrinsics:
//! - Uppercase<T>
//! - Lowercase<T>
//! - Capitalize<T>
//! - Uncapitalize<T>

use crate::relations::subtype::TypeResolver;
use crate::types::{
    IntrinsicKind, LiteralValue, StringIntrinsicKind, TemplateLiteralId, TemplateSpan, TypeData,
    TypeId,
};

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate string manipulation intrinsic types (Uppercase, Lowercase, Capitalize, Uncapitalize)
    /// These distribute over unions and transform string literal types
    //
    // The two arms returning `string_intrinsic(kind, evaluated_arg)` are
    // intentionally kept separate: the deferred-types arm (TypeParameter,
    // Infer, KeyOf, IndexAccess) and the non-string-primitive arm
    // (Number/Bigint/Boolean) document distinct rationales (see in-arm
    // comments). Merging them collapses two semantically-different cases
    // into one, hurting readability of a tricky tsc-parity branch.
    #[allow(clippy::match_same_arms)]
    pub(crate) fn evaluate_string_intrinsic(
        &mut self,
        kind: StringIntrinsicKind,
        type_arg: TypeId,
    ) -> TypeId {
        // First evaluate the type argument
        let evaluated_arg = self.evaluate(type_arg);

        let key = match self.interner().lookup(evaluated_arg) {
            Some(k) => k,
            None => return TypeId::ERROR,
        };
        match key {
            // Handle unions - distribute the operation over each member
            // Use recurse_string_intrinsic to respect depth limits
            TypeData::Union(members) => {
                let members = self.interner().type_list(members);
                let transformed: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.recurse_string_intrinsic(kind, member))
                    .collect();
                self.interner().union(transformed)
            }

            // String literal types - apply the transformation
            TypeData::Literal(LiteralValue::String(atom)) => {
                let s = self.interner().resolve_atom_ref(atom);
                let transformed = match kind {
                    StringIntrinsicKind::Uppercase => s.to_uppercase(),
                    StringIntrinsicKind::Lowercase => s.to_lowercase(),
                    StringIntrinsicKind::Capitalize => {
                        if s.is_empty() {
                            s.to_string()
                        } else {
                            let mut chars = s.chars();
                            match chars.next() {
                                Some(first) => {
                                    let upper: String = first.to_uppercase().collect();
                                    upper + chars.as_str()
                                }
                                None => s.to_string(),
                            }
                        }
                    }
                    StringIntrinsicKind::Uncapitalize => {
                        if s.is_empty() {
                            s.to_string()
                        } else {
                            let mut chars = s.chars();
                            match chars.next() {
                                Some(first) => {
                                    let lower: String = first.to_lowercase().collect();
                                    lower + chars.as_str()
                                }
                                None => s.to_string(),
                            }
                        }
                    }
                };
                self.interner().literal_string(&transformed)
            }

            // Template literal types - apply the transformation
            TypeData::TemplateLiteral(spans) => {
                self.apply_string_intrinsic_to_template_literal(kind, spans)
            }

            // The intrinsic string type passes through unchanged but wrapped in the intrinsic
            // so we preserve the Uppercase/Lowercase constraint (e.g. `string extends Uppercase<string>` is false).
            TypeData::Intrinsic(IntrinsicKind::String) => {
                self.interner().string_intrinsic(kind, evaluated_arg)
            }

            // For type parameters and other deferred types, keep as StringIntrinsic.
            //
            // Also: for non-string primitive intrinsics that are pattern literal
            // placeholders (number, bigint, boolean), preserve the StringMapping
            // wrapping. tsc represents this as `Mapping<\`${T}\`>`, but storing
            // `Mapping<T>` directly works as long as downstream consumers treat
            // the type_arg as a stringification pattern. See `visit_literal` in
            // the subtype visitor for the matching assignability rule.
            //
            // Without the primitive case, evaluation collapses to TypeId::ERROR,
            // and downstream template-literal matching
            // (e.g., `"1" <: \`${Uppercase<number>}\``) silently returns false
            // instead of accepting the literal.
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::KeyOf(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Intrinsic(
                IntrinsicKind::Number | IntrinsicKind::Bigint | IntrinsicKind::Boolean,
            ) => self.interner().string_intrinsic(kind, evaluated_arg),

            // Handle chained string intrinsics: Uppercase<Lowercase<T>>
            // Same-kind string mappings are idempotent: Uppercase<Uppercase<T>> = Uppercase<T>.
            // Different kinds must remain nested because compositions like
            // Uppercase<Lowercase<T>> can denote a strictly smaller set.
            TypeData::StringIntrinsic {
                kind: inner_kind,
                type_arg: _inner_arg,
            } => {
                if kind == inner_kind {
                    evaluated_arg
                } else {
                    // Preserve cross-kind composition so later substitution can
                    // still distinguish sets like Uppercase<Lowercase<T>>.
                    self.interner().string_intrinsic(kind, evaluated_arg)
                }
            }

            // For all other types, return error
            _ => TypeId::ERROR,
        }
    }

    /// Helper to recursively evaluate string intrinsic while respecting depth limits.
    pub(crate) fn recurse_string_intrinsic(
        &mut self,
        kind: StringIntrinsicKind,
        type_arg: TypeId,
    ) -> TypeId {
        let string_intrinsic = self.interner().string_intrinsic(kind, type_arg);
        self.evaluate(string_intrinsic)
    }

    /// Apply a string intrinsic to a template literal type
    ///
    /// This handles cases like an `Uppercase` over a template literal with
    /// `${string}` placeholders: text spans are uppercased and the type
    /// placeholders are wrapped in `Uppercase<...>`.
    ///
    /// For template literals with type interpolations:
    /// - Text spans are transformed (uppercased, lowercased, etc.)
    /// - Type spans are wrapped in the same string intrinsic
    /// - For Capitalize/Uncapitalize, special handling for the first span
    /// - Non-string primitive type spans (`number`, `bigint`, `boolean`) are
    ///   first stringified by wrapping in a single-placeholder template
    ///   literal to mirror tsc's `getStringMappingType` canonicalization.
    ///   This keeps templates produced by applying the intrinsic structurally
    ///   aligned with the hand-written equivalents tsc uses, and prevents
    ///   spurious TS2322 between equivalent pattern-literal forms.
    pub(crate) fn apply_string_intrinsic_to_template_literal(
        &self,
        kind: StringIntrinsicKind,
        spans: TemplateLiteralId,
    ) -> TypeId {
        let span_list = self.interner().template_list(spans);

        // Check if all spans are Text (no type interpolation)
        let all_text = span_list
            .iter()
            .all(|span| matches!(span, TemplateSpan::Text(_)));

        if all_text {
            // All spans are text - we can concatenate and transform
            let mut result = String::new();
            for span in span_list.iter() {
                if let TemplateSpan::Text(atom) = span {
                    let text = self.interner().resolve_atom_ref(*atom);
                    result.push_str(&text);
                }
            }

            let transformed = self.apply_string_transform(kind, &result);
            return self.interner().literal_string(&transformed);
        }

        // Template literal with type interpolations
        // Transform text spans and wrap type spans in the intrinsic
        let mut new_spans: Vec<TemplateSpan> = Vec::with_capacity(span_list.len());
        let mut is_first_span = true;

        for span in span_list.iter() {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.interner().resolve_atom_ref(*atom);
                    let transformed = if is_first_span {
                        // For first text span, apply full transformation (including Capitalize/Uncapitalize)
                        self.apply_string_transform(kind, &text)
                    } else {
                        // For subsequent text spans, only apply Uppercase/Lowercase
                        // Capitalize/Uncapitalize only affect the first character
                        match kind {
                            StringIntrinsicKind::Uppercase => text.to_uppercase(),
                            StringIntrinsicKind::Lowercase => text.to_lowercase(),
                            StringIntrinsicKind::Capitalize | StringIntrinsicKind::Uncapitalize => {
                                text.to_string()
                            }
                        }
                    };
                    let new_atom = self.interner().intern_string(&transformed);
                    new_spans.push(TemplateSpan::Text(new_atom));
                    // After a non-empty text span, subsequent spans are not "first"
                    if !text.is_empty() {
                        is_first_span = false;
                    }
                }
                TemplateSpan::Type(type_id) => {
                    // Canonicalize non-string primitive type spans (number, bigint,
                    // boolean) into the `${T}` template-literal form before applying
                    // the intrinsic. tsc's `getStringMappingType` does this via
                    // `isPatternLiteralPlaceholderType`: the placeholder is wrapped in
                    // a single-span template literal, so `Uppercase<number>` and
                    // `Uppercase<\`${number}\`>` collapse to the same canonical
                    // representation. For boolean, this also triggers expansion to
                    // `"true" | "false"` via the template-literal interner, matching
                    // tsc's `${boolean}` -> `"false" | "true"` behavior.
                    let canonical_arg =
                        canonicalize_string_intrinsic_arg(self.interner(), *type_id);
                    // For type interpolations, wrap in the appropriate string intrinsic
                    // For Capitalize/Uncapitalize on non-first position, we don't wrap
                    // since those only affect the first character
                    let wrapped_type = if is_first_span {
                        // First position type: apply the intrinsic
                        self.interner().string_intrinsic(kind, canonical_arg)
                    } else {
                        // Non-first position: only Uppercase/Lowercase apply
                        match kind {
                            StringIntrinsicKind::Uppercase | StringIntrinsicKind::Lowercase => {
                                self.interner().string_intrinsic(kind, canonical_arg)
                            }
                            StringIntrinsicKind::Capitalize | StringIntrinsicKind::Uncapitalize => {
                                // Capitalize/Uncapitalize don't affect non-first positions
                                *type_id
                            }
                        }
                    };
                    new_spans.push(TemplateSpan::Type(wrapped_type));
                    // After a type span, we're definitely not first anymore
                    is_first_span = false;
                }
            }
        }

        self.interner().template_literal(new_spans)
    }

    /// Apply a string transformation to a string value
    pub(crate) fn apply_string_transform(&self, kind: StringIntrinsicKind, s: &str) -> String {
        match kind {
            StringIntrinsicKind::Uppercase => s.to_uppercase(),
            StringIntrinsicKind::Lowercase => s.to_lowercase(),
            StringIntrinsicKind::Capitalize => {
                if s.is_empty() {
                    s.to_string()
                } else {
                    let mut chars = s.chars();
                    match chars.next() {
                        Some(first) => {
                            let upper: String = first.to_uppercase().collect();
                            upper + chars.as_str()
                        }
                        None => s.to_string(),
                    }
                }
            }
            StringIntrinsicKind::Uncapitalize => {
                if s.is_empty() {
                    s.to_string()
                } else {
                    let mut chars = s.chars();
                    match chars.next() {
                        Some(first) => {
                            let lower: String = first.to_lowercase().collect();
                            lower + chars.as_str()
                        }
                        None => s.to_string(),
                    }
                }
            }
        }
    }
}

/// Canonicalize a `StringIntrinsic` type argument so that non-string primitive
/// placeholders (`number`, `bigint`, `boolean`) are wrapped in template
/// literal form, mirroring tsc's `getStringMappingType` /
/// `isPatternLiteralPlaceholderType` normalization.
///
/// Without this, the result of applying a string mapping to a template such
/// as `Uppercase<aA-then-number>` would keep the bare `number` placeholder,
/// while a hand-written `Uppercase<just-number>` produced via the
/// template-literal canonical form keeps the `${number}` wrapping, leaving
/// the two equivalent pattern-literal types structurally different and
/// emitting spurious TS2322 mismatches between them.
///
/// For boolean specifically, wrapping in a template literal triggers the
/// template-literal interner expansion to `"false" | "true"`, matching tsc's
/// `${boolean}` cross-product behaviour.
pub(crate) fn canonicalize_string_intrinsic_arg(
    interner: &dyn crate::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use crate::types::{IntrinsicKind, TemplateSpan};
    match interner.lookup(type_id) {
        Some(TypeData::Intrinsic(IntrinsicKind::Number))
        | Some(TypeData::Intrinsic(IntrinsicKind::Bigint))
        | Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => {
            let empty_atom = interner.intern_string("");
            interner.template_literal(vec![
                TemplateSpan::Text(empty_atom),
                TemplateSpan::Type(type_id),
                TemplateSpan::Text(empty_atom),
            ])
        }
        _ => type_id,
    }
}
