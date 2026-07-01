//! Private-member storage maps for the ES5 class-to-IR converter.
//!
//! Resolves `this.#x` references inside member bodies to the
//! `__classPrivateFieldGet`/`Set` brand + kind (+ trailing function) form that
//! `tsc` emits at sub-ES2022 targets. Owns the read/write slot model consumed
//! by the expression converter.

use super::{AstToIr, IRNode, PrivateMemberSlot};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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

impl AstToIr<'_> {
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

    /// Decompose a `recv.#name` property access into `(receiver_idx,
    /// clean_name)`, or `None` when `idx` is not a private-identifier property
    /// access. Pure AST shape — it does not consult the storage maps.
    pub(super) fn private_access_target(&self, idx: NodeIndex) -> Option<(NodeIndex, String)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let name_node = self.arena.get(access.name_or_argument)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        let ident = self.arena.get_identifier(name_node)?;
        let raw = &ident.escaped_text;
        let clean = raw.strip_prefix('#').unwrap_or(raw.as_str()).to_string();
        Some((access.expression, clean))
    }

    /// Clean name of a bare `PrivateIdentifier` operand (`#x` → `"x"`), or
    /// `None` when `idx` is not a private identifier. The `#name in obj` brand
    /// check's left operand is the private identifier itself, not a
    /// `recv.#name` property access, so it needs its own decomposition. Pure
    /// AST shape — it does not consult the storage maps.
    fn bare_private_identifier_name(&self, idx: NodeIndex) -> Option<&str> {
        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        let raw = &self.arena.get_identifier(node)?.escaped_text;
        Some(raw.strip_prefix('#').unwrap_or(raw.as_str()))
    }

    /// Brand var (the `__classPrivateFieldIn` "state" argument) for a private
    /// member: `_C_x` for a field, `_C_instances` for an instance method or
    /// accessor. A setter-only accessor has no read slot, so its brand is taken
    /// from the write slot; both slots carry the same brand for a given member.
    /// `None` when the name has no storage slot at all (e.g. a static private
    /// method, which the ES5 converter leaves on the existing fallthrough).
    fn private_brand_var(&self, clean_name: &str) -> Option<String> {
        self.private_read_info(clean_name)
            .or_else(|| self.private_write_info(clean_name))
            .map(|(brand_var, _, _)| brand_var)
    }

    /// Lower a private-in brand check `#name in obj` to
    /// `__classPrivateFieldIn(<brand>, obj)`, or `None` when `bin` is not an
    /// `in` expression with a bare private-identifier left operand that resolves
    /// to a storage slot.
    ///
    /// `tsc` downlevels this operator at every sub-ES2022 target; once the
    /// private member is lowered to a `WeakMap`/`WeakSet` the raw `#name in obj`
    /// form is invalid JavaScript. The rule keys on the operand being a
    /// `PrivateIdentifier` with a known storage slot, never on its spelling.
    pub(super) fn private_brand_in_ir(&self, bin: &BinaryExprData) -> Option<IRNode> {
        if bin.operator_token != SyntaxKind::InKeyword as u16 {
            return None;
        }
        let clean = self.bare_private_identifier_name(bin.left)?;
        let brand_var = self.private_brand_var(clean)?;
        let obj = self.convert_expression(bin.right);
        Some(IRNode::PrivateFieldIn {
            weakmap_name: std::borrow::Cow::Owned(brand_var),
            obj: Box::new(obj),
        })
    }
}
