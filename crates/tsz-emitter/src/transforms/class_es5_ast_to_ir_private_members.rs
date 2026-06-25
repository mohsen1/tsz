//! Private-member storage maps for the ES5 class-to-IR converter.
//!
//! Resolves `this.#x` references inside member bodies to the
//! `__classPrivateFieldGet`/`Set` brand + kind (+ trailing function) form that
//! `tsc` emits at sub-ES2022 targets. Owns the read/write slot model consumed
//! by the expression converter.

use super::{AstToIr, PrivateMemberSlot};

/// Build the read/write slot for a private accessor. An instance accessor
/// brands against `_C_instances` and threads `func_var` (the getter/setter) as
/// the trailing helper argument; the legacy static-accessor shape brands
/// against `func_var` itself with no trailing reference.
fn accessor_slot(instance_brand: Option<&str>, func_var: &str) -> PrivateMemberSlot {
    match instance_brand {
        Some(brand) => PrivateMemberSlot {
            state_var: brand.to_string(),
            kind: "a",
            member_ref: Some(func_var.to_string()),
        },
        None => PrivateMemberSlot {
            state_var: func_var.to_string(),
            kind: "a",
            member_ref: None,
        },
    }
}

impl<'a> AstToIr<'a> {
    /// Provide private field, accessor, and method storage maps so that
    /// `this.#x` references inside member bodies lower to
    /// `__classPrivateFieldGet/Set` calls.
    ///
    /// Instance accessors and methods brand against `instances_weakset`
    /// (`_C_instances`) and carry the getter/setter/method function reference,
    /// matching tsc's 4-arg get / 5-arg set forms. Static accessors keep their
    /// historical lowering (the function var as the brand, no trailing
    /// reference); static methods are intentionally left to the existing
    /// fallthrough.
    pub fn with_private_member_maps(
        mut self,
        fields: &[crate::transforms::private_fields_es5::PrivateFieldInfo],
        accessors: &[crate::transforms::private_fields_es5::PrivateAccessorInfo],
        methods: &[crate::transforms::private_fields_es5::PrivateMethodInfo],
        instances_weakset: Option<&str>,
    ) -> Self {
        for field in fields {
            self.private_field_map
                .insert(field.name.clone(), field.weakmap_name.clone());
        }
        for accessor in accessors {
            // An instance accessor brands against `_C_instances` and passes the
            // getter/setter as the trailing helper argument. A static accessor
            // (or any accessor without an instance brand) retains the legacy
            // form where the function var itself is the brand and there is no
            // trailing reference.
            let instance_brand = (!accessor.is_static).then_some(instances_weakset).flatten();
            if let Some(ref get_var) = accessor.get_var_name {
                self.private_read_slots.insert(
                    accessor.name.clone(),
                    accessor_slot(instance_brand, get_var),
                );
            }
            if let Some(ref set_var) = accessor.set_var_name {
                self.private_write_slots.insert(
                    accessor.name.clone(),
                    accessor_slot(instance_brand, set_var),
                );
            }
        }
        for method in methods {
            // A private method is read as a function value branded against
            // `_C_instances`, then invoked with `.call`. Static private methods
            // brand against the class alias and are left to the fallthrough.
            if method.is_static {
                continue;
            }
            if let Some(instances) = instances_weakset {
                self.private_read_slots.insert(
                    method.name.clone(),
                    PrivateMemberSlot {
                        state_var: instances.to_string(),
                        kind: "m",
                        member_ref: Some(method.fn_var_name.clone()),
                    },
                );
            }
        }
        self
    }

    /// Look up private-member storage info for a read of `this.#name`. Returns
    /// `(brand_var, kind, member_ref)`: `member_ref` is the trailing
    /// getter/method function for the 4-arg `__classPrivateFieldGet` form, or
    /// `None` for a plain field (`kind == "f"`) and the legacy static-accessor
    /// shape where the function var is the brand.
    pub(super) fn private_read_info(
        &self,
        clean_name: &str,
    ) -> Option<(String, &'static str, Option<String>)> {
        if let Some(var) = self.private_field_map.get(clean_name) {
            return Some((var.clone(), "f", None));
        }
        if let Some(slot) = self.private_read_slots.get(clean_name) {
            return Some((slot.state_var.clone(), slot.kind, slot.member_ref.clone()));
        }
        None
    }

    /// Whether `this.#name` has a private read slot, without cloning it.
    pub(super) fn has_private_read_slot(&self, clean_name: &str) -> bool {
        self.private_field_map.contains_key(clean_name)
            || self.private_read_slots.contains_key(clean_name)
    }

    /// Look up private-member storage info for a write `this.#name = v`.
    /// Returns `(brand_var, kind, member_ref)`; see [`Self::private_read_info`].
    pub(super) fn private_write_info(
        &self,
        clean_name: &str,
    ) -> Option<(String, &'static str, Option<String>)> {
        if let Some(var) = self.private_field_map.get(clean_name) {
            return Some((var.clone(), "f", None));
        }
        if let Some(slot) = self.private_write_slots.get(clean_name) {
            return Some((slot.state_var.clone(), slot.kind, slot.member_ref.clone()));
        }
        None
    }
}
