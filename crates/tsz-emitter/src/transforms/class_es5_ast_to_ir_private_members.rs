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

/// The brand a non-field private member is keyed by: the class-value alias
/// (`_a`, the `_a = C` binding) for a static member, the `_C_instances`
/// `WeakSet` for an instance member. `None` when the required brand var was not
/// allocated (an instance member with no `WeakSet`, or a static member with no
/// class alias), in which case the caller leaves the member on the existing
/// fallthrough.
const fn member_brand<'b>(
    is_static: bool,
    static_class_brand: Option<&'b str>,
    instances_weakset: Option<&'b str>,
) -> Option<&'b str> {
    if is_static {
        static_class_brand
    } else {
        instances_weakset
    }
}

/// Build a read/write slot that brands against `brand` and threads `storage`
/// (the getter/setter/method function, or a static field's `{ value }` box) as
/// the trailing helper argument:
/// `__classPrivateFieldGet(recv, brand, "<kind>", storage)` /
/// `__classPrivateFieldSet(recv, brand, value, "<kind>", storage)`. `brand` is
/// `_C_instances` for an instance accessor/method and the class-value alias
/// (`_a`) for any static member; `kind` is `"f"` (static field), `"a"`
/// (accessor), or `"m"` (method). Instance *fields* do not use this shape —
/// they brand against their own `WeakMap` with no trailing reference — so they
/// stay in `private_field_map`.
fn member_slot(brand: &str, kind: &'static str, storage: &str) -> PrivateMemberSlot {
    PrivateMemberSlot {
        state_var: brand.to_string(),
        kind,
        member_ref: Some(storage.to_string()),
    }
}

impl AstToIr<'_> {
    /// Provide private field, accessor, and method storage maps so that
    /// `this.#x` references inside member bodies lower to
    /// `__classPrivateFieldGet/Set` calls.
    ///
    /// Instance accessors and methods brand against `instances_weakset`
    /// (`_C_instances`) and carry the getter/setter/method function reference,
    /// matching tsc's 4-arg get / 5-arg set forms. Static fields, accessors, and
    /// methods brand against `static_class_brand` (`_a`, the `_a = C` class-value
    /// alias) with the storage variable threaded the same way, matching tsc's
    /// `__classPrivateFieldGet(recv, _a, "<kind>", <storage>)` static form; a
    /// static method read this way is invoked via `.call(recv)`, and its brand
    /// also serves the `#name in obj` check (via `private_brand_var`).
    pub fn with_private_member_maps(
        mut self,
        fields: &[crate::transforms::private_fields_es5::PrivateFieldInfo],
        accessors: &[crate::transforms::private_fields_es5::PrivateAccessorInfo],
        methods: &[crate::transforms::private_fields_es5::PrivateMethodInfo],
        instances_weakset: Option<&str>,
        static_class_brand: Option<&str>,
    ) -> Self {
        for field in fields {
            // A static private field brands against the class-value alias with
            // the `{ value }` storage box threaded as `f`
            // (`__classPrivateFieldGet(recv, _a, "f", _C_x)`), so it needs a
            // read/write slot rather than the instance `WeakMap` field map
            // (`__classPrivateFieldGet(recv, _C_x, "f")`).
            if field.is_static
                && let Some(brand) = static_class_brand
            {
                self.private_read_slots.insert(
                    field.name.clone(),
                    member_slot(brand, "f", &field.weakmap_name),
                );
                self.private_write_slots.insert(
                    field.name.clone(),
                    member_slot(brand, "f", &field.weakmap_name),
                );
                continue;
            }
            self.private_field_map
                .insert(field.name.clone(), field.weakmap_name.clone());
        }
        for accessor in accessors {
            // Instance accessors brand against `_C_instances`; static accessors
            // brand against the class-value alias `_a`. Either way the
            // getter/setter is threaded as the trailing helper argument.
            let Some(brand) =
                member_brand(accessor.is_static, static_class_brand, instances_weakset)
            else {
                continue;
            };
            if let Some(ref get_var) = accessor.get_var_name {
                self.private_read_slots
                    .insert(accessor.name.clone(), member_slot(brand, "a", get_var));
            }
            if let Some(ref set_var) = accessor.set_var_name {
                self.private_write_slots
                    .insert(accessor.name.clone(), member_slot(brand, "a", set_var));
            }
        }
        for method in methods {
            // A private method is read as a function value, then invoked with
            // `.call`. Instance methods brand against `_C_instances`; static
            // methods brand against the class-value alias `_a`.
            let Some(brand) = member_brand(method.is_static, static_class_brand, instances_weakset)
            else {
                continue;
            };
            self.private_read_slots.insert(
                method.name.clone(),
                member_slot(brand, "m", &method.fn_var_name),
            );
        }
        self
    }

    /// Look up private-member storage info for a read of `this.#name`. Returns
    /// `(brand_var, kind, member_ref)`: `member_ref` is the trailing
    /// getter/method/storage function for the 4-arg `__classPrivateFieldGet`
    /// form, or `None` for an instance field (`kind == "f"`, brand is the
    /// `WeakMap`).
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
