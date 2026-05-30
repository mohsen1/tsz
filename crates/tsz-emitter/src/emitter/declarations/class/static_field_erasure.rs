//! Structural predicate for `useDefineForClassFields` erasure of
//! no-initializer **static** class fields.
//!
//! `tsc`'s `transformPropertyWorker` (in `transformers/classFields.ts`) returns
//! no statement for a property when it has a `static` modifier (or is a private
//! name) *and* has no initializer:
//!
//! ```ts
//! if ((isPrivateIdentifier(name) || hasStaticModifier(property)) && !property.initializer) {
//!     return undefined;
//! }
//! ```
//!
//! So a static field declared without an initializer is never materialized as
//! `Object.defineProperty(C, <name>, { ... value: void 0 })`. Materializing one
//! produced a `defineProperty` descriptor with an empty `value:` (invalid JS)
//! and would clobber the constructor function's own non-writable slots
//! (`name`, `length`, `prototype`, `caller`, `arguments`).
//!
//! The rule keys purely on structural facts — `no initializer` + `define
//! semantics enabled`, evaluated at a call site that has already established the
//! `static`-modifier half of the condition — never on the chosen field name. A
//! no-initializer *instance* field is unaffected and still materializes with
//! `value: void 0`; an *initialized* static field is unaffected and still emits
//! its define. The computed-name temp hoisting for an erased field still happens
//! independently, matching `tsc`'s retained `_a = expr` capture.

/// Returns `true` when a static class field should be erased from
/// `useDefineForClassFields` runtime field-lowering, given:
///
/// * `no_initializer` — the property declaration has no initializer expression.
/// * `use_define_for_class_fields` — define-field semantics are enabled.
///
/// Callers invoke this only after establishing the field carries a `static`
/// modifier (the enclosing `static` branch), which is the other half of `tsc`'s
/// `(isPrivateIdentifier(name) || hasStaticModifier(property)) && !initializer`
/// condition.
pub(in crate::emitter::declarations::class) const fn static_no_init_field_is_erased(
    no_initializer: bool,
    use_define_for_class_fields: bool,
) -> bool {
    no_initializer && use_define_for_class_fields
}

#[cfg(test)]
mod tests {
    use super::static_no_init_field_is_erased;

    #[test]
    fn no_init_with_define_is_erased() {
        assert!(static_no_init_field_is_erased(true, true));
    }

    #[test]
    fn initialized_static_field_is_not_erased() {
        assert!(!static_no_init_field_is_erased(false, true));
    }

    #[test]
    fn no_init_without_define_is_not_erased() {
        // Without define semantics a bare typed static field has no runtime
        // form anyway; the caller filters it earlier, but the predicate must
        // not claim erasure on its own.
        assert!(!static_no_init_field_is_erased(true, false));
    }
}
