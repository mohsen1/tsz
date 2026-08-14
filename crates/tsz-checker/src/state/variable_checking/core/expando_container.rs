//! Declaration-kind check shared by the `TS2403` cross-file merge sites in
//! the parent module.
//!
//! Split out of the parent module to satisfy the source-file line cap.

use super::*;

impl<'a> CheckerState<'a> {
    /// Whether `kind` is one of the declaration kinds that merge instead of
    /// conflicting for `TS2403` purposes: namespace/module, enum, class,
    /// interface, function. A `var`/`let`/`const` initialized with a
    /// function/arrow/class expression does **not** get this exemption even
    /// once its name picks up a JS expando member assignment (`x.prop =
    /// ...`) — verified against `typescript@7.0.2`:
    /// `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`
    /// (`a.js`'s `var x = function foo() {}; x.a = function bar() {}` vs
    /// `b.ts`'s `var x = function () { return 1; }();`) still reports
    /// `TS2403` alongside `TS2339`.
    pub(in crate::state_domain::variable_checking) const fn is_mergeable_decl_kind(
        &self,
        kind: u16,
    ) -> bool {
        matches!(
            kind,
            syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
        )
    }
}
