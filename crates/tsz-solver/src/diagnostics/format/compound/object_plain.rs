//! Plain object formatting helpers.

use super::super::TypeFormatter;
use super::super::needs_property_name_quotes;
use crate::types::{PropertyInfo, TypeData, TypeId};
use tsz_binder::SymbolId;

impl<'a> TypeFormatter<'a> {
    fn object_display_tail_index(&self, props: &[&PropertyInfo]) -> usize {
        props
            .iter()
            .rposition(|prop| {
                self.interner
                    .resolve_atom_ref(prop.name)
                    .as_ref()
                    .starts_with("[Symbol.")
            })
            .filter(|&idx| idx > 0)
            .unwrap_or(props.len() - 1)
    }

    fn format_large_object(&mut self, display_props: &[&PropertyInfo]) -> String {
        debug_assert!(display_props.len() >= 22);

        let is_string_apparent_member_list = display_props
            .first()
            .is_some_and(|prop| self.interner.resolve_atom_ref(prop.name).as_ref() == "toString")
            && display_props.iter().any(|prop| {
                self.interner.resolve_atom_ref(prop.name).as_ref() == "[Symbol.iterator]"
            });
        let max_head_parts = if is_string_apparent_member_list {
            1
        } else {
            17
        };
        self.format_large_object_with_prefix(Vec::new(), display_props, max_head_parts)
    }

    pub(super) fn format_large_object_with_prefix(
        &mut self,
        prefix_parts: Vec<String>,
        display_props: &[&PropertyInfo],
        max_head_parts: usize,
    ) -> String {
        let total = prefix_parts.len() + display_props.len();
        debug_assert!(total >= 22);

        const MAX_HEAD_CHARS: usize = 380;

        let tail_prop_index = self.object_display_tail_index(display_props);
        let tail_part_index = prefix_parts.len() + tail_prop_index;
        let tail = Self::collapse_truncated_tail_part(
            &self.format_property(display_props[tail_prop_index]),
        );
        let max_head_chars = if tail_part_index == total - 1 {
            MAX_HEAD_CHARS
        } else {
            255
        };
        let mut head_parts = Vec::new();
        let mut used_chars = 0usize;

        for idx in 0..tail_part_index {
            if head_parts.len() >= max_head_parts {
                break;
            }

            let part = if let Some(prefix_part) = prefix_parts.get(idx) {
                prefix_part.clone()
            } else {
                self.format_property(display_props[idx - prefix_parts.len()])
            };
            let part_cost = if head_parts.is_empty() {
                part.len()
            } else {
                part.len() + 2
            };
            let next_used = used_chars + part_cost;
            let remaining_after = total - (idx + 1) - 1;
            let omitted_digits = remaining_after.max(1).to_string().len();
            let reserve_for_marker = 2 + 4 + omitted_digits + 9;
            let reserve_for_tail = 2 + tail.len();

            if head_parts.len() >= 2
                && next_used + reserve_for_marker + reserve_for_tail > max_head_chars
            {
                break;
            }

            used_chars = next_used;
            head_parts.push(part);
        }

        if head_parts.is_empty() && tail_part_index > 0 {
            if let Some(prefix_part) = prefix_parts.first() {
                head_parts.push(prefix_part.clone());
            } else {
                head_parts.push(self.format_property(display_props[0]));
            }
        }

        let omitted = total.saturating_sub(head_parts.len() + 1);
        if omitted == 0 {
            let mut formatted = prefix_parts;
            formatted.extend(display_props.iter().map(|prop| self.format_property(prop)));
            return self.format_object_parts(formatted);
        }

        let mut parts = Vec::with_capacity(head_parts.len() + 2);
        parts.extend(head_parts);
        parts.push(format!("... {omitted} more ..."));
        parts.push(tail);
        format!("{{ {}; }}", parts.join("; "))
    }

    pub(super) fn visible_object_properties<'b>(
        &self,
        props: &'b [PropertyInfo],
    ) -> Vec<&'b PropertyInfo> {
        let default_name = self.interner.intern_string("default");
        let internal_default_name = self.interner.intern_string("_default");
        let default_prop = props.iter().find(|prop| prop.name == default_name);

        props
            .iter()
            .filter(|prop| {
                if prop.name != internal_default_name {
                    return true;
                }
                let Some(default_prop) = default_prop else {
                    return true;
                };

                // Some module export surfaces retain the local `_default` binding
                // alongside the real `default` export. tsc hides that duplicate
                // implementation detail in object displays.
                prop.type_id != default_prop.type_id
                    || prop.write_type != default_prop.write_type
                    || prop.optional != default_prop.optional
                    || prop.readonly != default_prop.readonly
                    || prop.is_method != default_prop.is_method
            })
            .collect()
    }

    /// Deterministic display ordering for a pair of object properties.
    ///
    /// Primary key is `declaration_order` — the tsc source-order signal — used
    /// only when BOTH properties carry a real (`> 0`) order, so the parity path
    /// (declared members rendered in source order) is unchanged. When the
    /// primary key does not decide (an unset or tied `declaration_order`, the
    /// case for synthesized objects that carry no source order), numeric keys
    /// sort numerically and ahead of string keys — tsc lists numeric index
    /// members first — and the final tiebreak is the property NAME string.
    ///
    /// The name tiebreak replaces a former reliance on the stable sort
    /// preserving the stored property order. Properties are stored sorted by
    /// `Atom` id for identity/hashing, and `Atom` ids are handed out in
    /// string-interning order by an interner shared across the parallel
    /// checker's worker threads — so that order is thread-schedule dependent,
    /// and the same synthesized object rendered its members in different orders
    /// run to run (#16309, evidence #3). Keying the last resort on the name
    /// string makes the rendered order a pure function of the type, independent
    /// of interning/allocation order.
    pub(super) fn compare_display_property_order(
        &self,
        a: &PropertyInfo,
        b: &PropertyInfo,
    ) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        // Primary: declaration_order (0 means unset, treated as equal).
        let ord = a.declaration_order.cmp(&b.declaration_order);
        if ord != Ordering::Equal && a.declaration_order > 0 && b.declaration_order > 0 {
            return ord;
        }
        // Tiebreak for properties with the same declaration_order: numeric keys
        // sort numerically and ahead of string keys, then a deterministic,
        // interning-order-independent name comparison.
        let a_name = self.interner.resolve_atom_ref(a.name);
        let b_name = self.interner.resolve_atom_ref(b.name);
        match (a_name.parse::<u64>(), b_name.parse::<u64>()) {
            (Ok(an), Ok(bn)) => an.cmp(&bn),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a_name.as_ref().cmp(b_name.as_ref()),
        }
    }

    pub(in crate::diagnostics::format) fn format_object(
        &mut self,
        props: &[PropertyInfo],
    ) -> String {
        if props.is_empty() {
            return "{}".to_string();
        }
        let mut display_props = self.visible_object_properties(props);
        // Sort properties for display. Use declaration_order as primary key when
        // available, with a deterministic content-based tiebreak (see
        // `compare_display_property_order`). Properties are stored sorted by
        // Atom ID for identity/hashing, so display order must be restored here.
        display_props.sort_by(|a, b| self.compare_display_property_order(a, b));
        if display_props.len() >= 22 {
            return self.format_large_object(&display_props);
        }

        let formatted: Vec<String> = display_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        self.format_object_parts(formatted)
    }

    /// A symbol-valued computed property key (`{ [sym]: T }`) is stored
    /// internally under the synthetic binding-identity atom `__unique_<SymbolId>`
    /// so that distinct unique symbols key distinct members. `tsc` never shows
    /// that internal atom; it displays the key as `[<symbolName>]` (e.g.
    /// `[sym]`). Recover that display form for symbol-named properties.
    ///
    /// Returns `None` (so the caller falls back to normal name rendering) when:
    /// - the property is not symbol-named (a user-authored string property that
    ///   merely *looks* like `"__unique_3"` must stay a string key), or
    /// - the key is a well-known symbol already stored in bracketed form
    ///   (`[Symbol.iterator]`), which the `__unique_` prefix check skips, or
    /// - the backing symbol cannot be resolved.
    fn symbol_keyed_property_display_name(&self, prop: &PropertyInfo) -> Option<String> {
        if !prop.is_symbol_named {
            return None;
        }
        let raw = self.interner.resolve_atom_ref(prop.name);
        let id: u32 = raw.as_ref().strip_prefix("__unique_")?.parse().ok()?;
        let symbol = self.symbol_arena?.get(SymbolId(id))?;
        Some(format!("[{}]", symbol.escaped_name))
    }

    pub(super) fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let name = if let Some(symbol_name) = self.symbol_keyed_property_display_name(prop) {
            symbol_name
        } else {
            let raw_name = self.atom(prop.name);
            if needs_property_name_quotes(&raw_name) {
                // tsc uses double quotes for JSX-specific property names
                // (namespace-prefixed like "ns:attr" and data attributes like "data-foo"),
                // for names starting with a digit, and for names containing any
                // character outside [a-zA-Z0-9_-] (e.g. "*"). Single quotes are used
                // for all other quoted property names (e.g. 'stage-0', '').
                let use_double = raw_name.contains(':')
                    || raw_name.starts_with("data-")
                    || raw_name.chars().next().is_some_and(|c| c.is_ascii_digit())
                    || raw_name
                        .chars()
                        .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'));
                if use_double {
                    let escaped = raw_name.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("\"{escaped}\"")
                } else {
                    let escaped = raw_name.replace('\\', "\\\\").replace('\'', "\\'");
                    format!("'{escaped}'")
                }
            } else {
                raw_name.to_string()
            }
        };

        // Method shorthand: `name(params): return_type` instead of `name: (params) => return_type`.
        //
        // tsc's node builder (`addPropertyToElementList`) only emits a method signature
        // for a method-flagged symbol when it is NOT readonly; a readonly method (e.g. a
        // method captured by `as const`, which freezes every member) falls to the
        // property-signature branch and renders as `readonly name: (params) => return_type`.
        // Mirror that here so diagnostic display matches tsc — and matches tsz's own
        // declaration emitter, which already prints the property form for readonly methods.
        if prop.is_method && !prop.readonly {
            match self.interner.lookup(prop.type_id) {
                Some(TypeData::Function(f_id)) => {
                    let shape = self.interner.function_shape(f_id);
                    let display_params =
                        self.display_params_for_function_shape(f_id, &shape.params);
                    let type_params = self.format_type_params(&shape.type_params);
                    let params = self.format_params(&display_params, shape.this_type);
                    let return_str = self.format(shape.return_type);
                    return format!(
                        "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                        params = params.join(", ")
                    );
                }
                Some(TypeData::Callable(callable_id)) => {
                    let shape = self.interner.callable_shape(callable_id);
                    if let Some(sig) = shape.call_signatures.first() {
                        let type_params = self.format_type_params(&sig.type_params);
                        let params = self.format_params(&sig.params, sig.this_type);
                        let return_str = self.format(sig.return_type);
                        return format!(
                            "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                            params = params.join(", ")
                        );
                    }
                }
                _ => {}
            }
        }

        // tsc displays optional object properties WITH `| undefined`:
        // `n?: number | undefined`. If the stored type doesn't already contain
        // undefined, we append it.
        let type_str: String = if prop.optional {
            if self.preserve_optional_property_surface_syntax {
                let surface_type = if prop.write_type != TypeId::NONE {
                    prop.write_type
                } else {
                    prop.type_id
                };
                self.format(surface_type).into_owned()
            } else if prop.type_id == TypeId::NEVER {
                // `never | undefined` simplifies to `undefined`; tsc displays just `undefined`
                "undefined".to_string()
            } else if !self.type_contains_undefined(prop.type_id) {
                let formatted = self.format(prop.type_id).into_owned();
                format!("{formatted} | undefined")
            } else {
                self.format(prop.type_id).into_owned()
            }
        } else {
            self.format(prop.type_id).into_owned()
        };
        format!("{readonly}{name}{optional}: {type_str}")
    }

    /// Check if a type already contains `undefined` (as a union member or is undefined itself).
    /// Also treats `any` and `unknown` as absorbing undefined, since `any | undefined` == `any`
    /// and `unknown | undefined` == `unknown` in tsc's display.
    pub(super) fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members
                .iter()
                .any(|&m| m == TypeId::UNDEFINED || m == TypeId::ANY || m == TypeId::UNKNOWN);
        }
        false
    }
}
