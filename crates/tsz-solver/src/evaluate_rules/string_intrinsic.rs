//! String intrinsic type evaluation.
//!
//! Handles TypeScript's string manipulation intrinsics:
//! - Uppercase<T>
//! - Lowercase<T>
//! - Capitalize<T>
//! - Uncapitalize<T>

use crate::subtype::TypeResolver;
use crate::types::*;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate string manipulation intrinsic types (Uppercase, Lowercase, Capitalize, Uncapitalize)
    /// These distribute over unions and transform string literal types
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
            TypeKey::Union(members) => {
                let members = self.interner().type_list(members);
                let transformed: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.recurse_string_intrinsic(kind, member))
                    .collect();
                self.interner().union(transformed)
            }

            // String literal types - apply the transformation
            TypeKey::Literal(LiteralValue::String(atom)) => {
                let s = self.interner().resolve_atom_ref(atom);
                let transformed = match kind {
                    StringIntrinsicKind::Uppercase => s.to_uppercase().to_string(),
                    StringIntrinsicKind::Lowercase => s.to_lowercase().to_string(),
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
            TypeKey::TemplateLiteral(spans) => {
                self.apply_string_intrinsic_to_template_literal(kind, spans)
            }

            // The intrinsic string type passes through unchanged
            TypeKey::Intrinsic(IntrinsicKind::String) => TypeId::STRING,

            // For type parameters and other deferred types, keep as StringIntrinsic
            TypeKey::TypeParameter(_)
            | TypeKey::Infer(_)
            | TypeKey::KeyOf(_)
            | TypeKey::IndexAccess(_, _) => self.interner().intern(TypeKey::StringIntrinsic {
                kind,
                type_arg: evaluated_arg,
            }),

            // Handle chained string intrinsics: Uppercase<Lowercase<T>>
            // The inner intrinsic already wraps the type, so wrap again with outer
            TypeKey::StringIntrinsic {
                kind: _inner_kind,
                type_arg: _inner_arg,
            } => {
                // Wrap the already-evaluated intrinsic with the outer one
                // This creates Uppercase<Lowercase<T>> structure which will be
                // evaluated layer by layer when the type parameter is substituted
                self.interner().intern(TypeKey::StringIntrinsic {
                    kind,
                    type_arg: evaluated_arg,
                })
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
        let string_intrinsic = self
            .interner()
            .intern(TypeKey::StringIntrinsic { kind, type_arg });
        self.evaluate(string_intrinsic)
    }

    /// Apply a string intrinsic to a template literal type
    ///
    /// This handles cases like `Uppercase<\`hello-${string}\`>` which should produce
    /// a template literal with uppercase text spans: `\`HELLO-${string}\``
    ///
    /// For template literals with type interpolations:
    /// - Text spans are transformed (uppercased, lowercased, etc.)
    /// - Type spans are wrapped in the same string intrinsic
    /// - For Capitalize/Uncapitalize, special handling for the first span
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
                    // For type interpolations, wrap in the appropriate string intrinsic
                    // For Capitalize/Uncapitalize on non-first position, we don't wrap
                    // since those only affect the first character
                    let wrapped_type = if is_first_span {
                        // First position type: apply the intrinsic
                        self.interner().intern(TypeKey::StringIntrinsic {
                            kind,
                            type_arg: *type_id,
                        })
                    } else {
                        // Non-first position: only Uppercase/Lowercase apply
                        match kind {
                            StringIntrinsicKind::Uppercase | StringIntrinsicKind::Lowercase => {
                                self.interner().intern(TypeKey::StringIntrinsic {
                                    kind,
                                    type_arg: *type_id,
                                })
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
