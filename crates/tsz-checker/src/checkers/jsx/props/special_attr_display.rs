//! Display-target resolution for the JSX framework "special" attributes
//! (`key`/`ref`).
//!
//! `tsc` checks `key`/`ref` as members of the merged
//! `JSX.IntrinsicAttributes` / `JSX.IntrinsicClassAttributes` object, so a
//! `TS2322` against one of them keeps the property's full declared apparent
//! type — alias name intact and `| null | undefined` retained. Ordinary
//! write-position props strip nullish (and tsz already matches `tsc` there).
//! These helpers detect the framework attributes structurally (interface
//! membership, not the attribute spelling) and return the alias-preserved
//! declared property type to display.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// True when `attr_name` is a JSX framework "special" attribute — a member
    /// declared on (or inherited into) `JSX.IntrinsicAttributes` (React's `key`)
    /// or `JSX.IntrinsicClassAttributes` (React's `ref`). Detected structurally
    /// through the framework attribute interfaces and their declared heritage —
    /// not by matching the attribute spelling — so a project that renames, omits,
    /// or augments those interfaces is handled by membership rather than a name
    /// literal. Returns `false` when the JSX namespace exposes no such interface.
    pub(in crate::checkers_domain::jsx) fn jsx_attr_is_intrinsic_framework_attribute(
        &mut self,
        attr_name: &str,
    ) -> bool {
        if let Some(ia) = self.get_intrinsic_attributes_lazy_type() {
            let ia = self.normalize_jsx_required_props_target(ia);
            if self.jsx_type_or_heritage_declares_property(ia, attr_name) {
                return true;
            }
        }
        if let Some(ica) = self.get_intrinsic_class_attributes_lazy_type() {
            let ica = self.normalize_jsx_required_props_target(ica);
            if self.jsx_type_or_heritage_declares_property(ica, attr_name) {
                return true;
            }
        }
        false
    }

    /// `true` when `ty` carries `prop_name` either directly or through its
    /// declared interface heritage. Combines the structural property lookup with
    /// the AST heritage scan so inherited framework attributes (e.g. `key` from
    /// `React.Attributes`, `ref` from `React.ClassAttributes`) are recognized.
    fn jsx_type_or_heritage_declares_property(&mut self, ty: TypeId, prop_name: &str) -> bool {
        if self
            .jsx_lookup_declared_property_type(ty, prop_name)
            .is_some()
        {
            return true;
        }
        self.jsx_declared_interface_heritage_has_property(ty, prop_name)
    }

    /// Resolve `prop_name` on `ty`, returning the declared property type with its
    /// type-alias application preserved (e.g. `Key | null | undefined`,
    /// `LegacyRef<T> | undefined`) rather than the evaluated/expanded structural
    /// form. Returns `None` when the property is absent.
    fn jsx_lookup_declared_property_type(&mut self, ty: TypeId, prop_name: &str) -> Option<TypeId> {
        use crate::query_boundaries::property_access::PropertyAccessResult;
        match self.resolve_property_access_with_env(ty, prop_name) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }

    /// Pick the `TS2322` target type to DISPLAY for a JSX framework special
    /// attribute (`key`/`ref`).
    ///
    /// `tsc` checks `key`/`ref` as members of the merged
    /// `IntrinsicAttributes`/`IntrinsicClassAttributes` object, so its TS2322
    /// elaboration shows the property's full declared apparent type — alias name
    /// intact and `| null | undefined` retained (`Key | null | undefined`,
    /// `LegacyRef<HTMLDivElement> | undefined`). Ordinary write-position props are
    /// different: `tsc` strips `| null | undefined` there, and tsz already matches
    /// that (the `onClick?: string | undefined` control renders `string`).
    ///
    /// This returns the alias-preserved declared property type so the special
    /// attribute is displayed `tsc`-faithfully, or `None` when the attribute is
    /// not a framework special attribute reachable on those interfaces (so
    /// ordinary props keep their existing, correct stripped display). The
    /// returned type is for DISPLAY only; the assignability relation continues to
    /// use the write-position check type.
    pub(in crate::checkers_domain::jsx) fn jsx_special_attr_display_target_type(
        &mut self,
        attr_name: &str,
        props_type: TypeId,
        special_attr_component_type: Option<TypeId>,
    ) -> Option<TypeId> {
        if !self.jsx_attr_is_intrinsic_framework_attribute(attr_name) {
            return None;
        }
        // Intrinsic elements carry `key`/`ref` directly on their attribute object
        // (via the `DetailedHTMLProps` → `ClassAttributes` chain), so the declared
        // type is reachable on `props_type` with its alias intact.
        if let Some(declared) = self.jsx_lookup_declared_property_type(props_type, attr_name) {
            return Some(declared);
        }
        // Components merge `key`/`ref` from the framework attribute interfaces
        // rather than their own props object: resolve the declared type there.
        if let Some(ia) = self.get_intrinsic_attributes_type() {
            let ia = self.normalize_jsx_required_props_target(ia);
            if let Some(declared) = self.jsx_lookup_declared_property_type(ia, attr_name) {
                return Some(declared);
            }
        }
        if let Some(component_type) = special_attr_component_type
            && let Some(ica) =
                self.get_intrinsic_class_attributes_type_for_component(component_type)
        {
            let ica = self.normalize_jsx_required_props_target(ica);
            if let Some(declared) = self.jsx_lookup_declared_property_type(ica, attr_name) {
                return Some(declared);
            }
        }
        None
    }
}
