//! Synthesized iterator method types for array and tuple property access.

use crate::query_boundaries::common::{
    TypeSubstitution, array_element_type, get_tuple_element_type_union, instantiate_type,
    object_shape_for_type,
};
use crate::state::CheckerState;
use tsz_solver::{FunctionShape, ObjectShape, TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain) fn synthesized_array_iterator_method_type(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "values" | "keys" | "entries") {
            return None;
        }
        let element_type = array_element_type(self.ctx.types, object_type)
            .or_else(|| get_tuple_element_type_union(self.ctx.types, object_type))?;

        let return_arg = match property_name {
            "values" => element_type,
            "keys" => TypeId::NUMBER,
            "entries" => self.ctx.types.tuple(vec![
                TupleElement {
                    type_id: TypeId::NUMBER,
                    name: None,
                    optional: false,
                    rest: false,
                },
                TupleElement {
                    type_id: element_type,
                    name: None,
                    optional: false,
                    rest: false,
                },
            ]),
            _ => return None,
        };

        let return_type = self.synthesized_array_iterator_return_type(return_arg)?;

        Some(self.ctx.types.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }))
    }

    fn synthesized_array_iterator_return_type(&mut self, return_arg: TypeId) -> Option<TypeId> {
        // The canonical ArrayIterator lazy body can be populated from the
        // es2015-only declaration before the es2025 iterator-helper augmentation
        // is resolved. IteratorObject carries the helper members, so synthesize
        // from that resolved body and stamp the result with ArrayIterator for
        // display and assignability.
        let (iterator_name, iterator_type) =
            if let Some(iterator_type) = self.resolve_lib_type_by_name("IteratorObject") {
                ("IteratorObject", Some(iterator_type))
            } else if let Some(iterator_type) = self.resolve_lib_type_by_name("IterableIterator") {
                ("IterableIterator", Some(iterator_type))
            } else {
                ("IterableIterator", None)
            };

        if let Some(iterator_type) = iterator_type {
            let instantiated = self.instantiate_synthesized_iterator_type(
                iterator_name,
                iterator_type,
                return_arg,
            );
            if iterator_name == "IteratorObject"
                && let Some(array_iterator_sym) = self.ctx.binder.file_locals.get("ArrayIterator")
                && let Some(shape) = object_shape_for_type(self.ctx.types, instantiated)
            {
                let stamped = self.ctx.types.factory().object_with_index(ObjectShape {
                    flags: shape.flags,
                    properties: shape.properties.clone(),
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: Some(array_iterator_sym),
                });
                self.record_array_iterator_display_alias(stamped, return_arg);
                Some(stamped)
            } else {
                Some(instantiated)
            }
        } else {
            let iterator_base = self
                .resolve_entity_name_text_to_def_id_for_lowering(iterator_name)
                .map(|def_id| self.ctx.types.lazy(def_id))?;
            Some(self.ctx.types.application(iterator_base, vec![return_arg]))
        }
    }

    /// Record an `ArrayIterator<lead_arg, ...defaults>` display alias on the
    /// stamped synthesized iterator return type.
    ///
    /// The synthesizer builds the result by instantiating the *base* interface
    /// `IteratorObject<T, TReturn, TNext>` and re-stamping the resulting
    /// `ObjectWithIndex` with the `ArrayIterator` symbol so display and
    /// assignability prefer the derived name. The stamped shape carries the
    /// base's substitution — every inherited member signature mentions
    /// `IteratorObject<...>`, not `ArrayIterator<T>`.
    ///
    /// Without an explicit alias, the diagnostic formatter falls back to
    /// guessing the derived interface's type arguments from member signatures
    /// and picks up the inherited base instantiation, producing
    /// `ArrayIterator<IteratorObject<string, undefined, unknown>>` instead of
    /// `ArrayIterator<string>`. Recording the original `Application(Lazy(ArrayIterator), [lead_arg, ...defaults])`
    /// hands the formatter the derived-interface type-argument list directly
    /// so it renders the same form tsc does.
    fn record_array_iterator_display_alias(&mut self, stamped: TypeId, lead_arg: TypeId) {
        let Some(def_id) = self.resolve_entity_name_text_to_def_id_for_lowering("ArrayIterator")
        else {
            return;
        };
        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        if type_params.is_empty() {
            return;
        }
        let type_args: Vec<TypeId> = std::iter::once(lead_arg)
            .chain(
                type_params
                    .iter()
                    .skip(1)
                    .map(|p| p.default.or(p.constraint).unwrap_or(TypeId::UNKNOWN)),
            )
            .collect();
        let base = self.ctx.types.lazy(def_id);
        let application = self.ctx.types.application(base, type_args);
        self.ctx.types.store_display_alias(stamped, application);
    }

    fn instantiate_synthesized_iterator_type(
        &mut self,
        iterator_name: &str,
        iterator_type: TypeId,
        return_arg: TypeId,
    ) -> TypeId {
        let mut type_args = if iterator_name == "IteratorObject" {
            vec![
                return_arg,
                self.builtin_iterator_return_intrinsic_type(),
                TypeId::UNKNOWN,
            ]
        } else {
            vec![return_arg]
        };
        let type_params = self
            .ctx
            .binder
            .file_locals
            .get(iterator_name)
            .map(|sym_id| self.get_type_params_for_symbol(sym_id))
            .unwrap_or_default();
        for param in type_params.iter().skip(type_args.len()) {
            type_args.push(
                param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN),
            );
        }
        type_args.truncate(type_params.len());

        let substitution = TypeSubstitution::from_args(self.ctx.types, &type_params, &type_args);
        instantiate_type(self.ctx.types, iterator_type, &substitution)
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source_with_libs, load_default_lib_files};

    /// Find the TS2322 diagnostic whose message is being inspected.
    fn ts2322_message(src: &str) -> String {
        let libs = load_default_lib_files();
        let diags = check_source_with_libs(
            src,
            "test.ts",
            CheckerOptions {
                strict: true,
                ..CheckerOptions::default()
            },
            &libs,
        );
        diags
            .into_iter()
            .find(|d| d.code == 2322)
            .map(|d| d.message_text)
            .unwrap_or_else(|| panic!("expected TS2322 in:\n{src}"))
    }

    // Structural rule: when an array/tuple's `values/keys/entries` is
    // assigned to an incompatible type, the diagnostic displays the
    // synthesized iterator as `ArrayIterator<T>` (the derived interface name
    // with the derived interface's own lead type argument), not as
    // `ArrayIterator<IteratorObject<...>>` (the inherited base's
    // substitution).
    //
    // The rule is structural: the same display must hold regardless of
    // element type, alias chain, or `values`/`keys`/`entries` choice. The
    // tests below exercise three element shapes and all three methods.

    #[test]
    fn array_values_displays_array_iterator_with_element_type() {
        let msg = ts2322_message(
            "const a = [\"x\", \"y\"];\nconst it = a.values();\nconst bad: number = it;\n",
        );
        assert!(
            msg.contains("'ArrayIterator<string>'"),
            "expected 'ArrayIterator<string>' display, got: {msg}"
        );
        assert!(
            !msg.contains("IteratorObject<"),
            "diagnostic must not expose IteratorObject as the type argument, got: {msg}"
        );
    }

    #[test]
    fn array_keys_displays_array_iterator_of_number() {
        let msg = ts2322_message(
            "const a = [\"x\", \"y\"];\nconst it = a.keys();\nconst bad: number = it;\n",
        );
        assert!(
            msg.contains("'ArrayIterator<number>'"),
            "expected 'ArrayIterator<number>' display, got: {msg}"
        );
    }

    #[test]
    fn array_entries_displays_array_iterator_of_index_value_tuple() {
        let msg = ts2322_message(
            "const a = [\"x\", \"y\"];\nconst it = a.entries();\nconst bad: number = it;\n",
        );
        assert!(
            msg.contains("'ArrayIterator<[number, string]>'"),
            "expected 'ArrayIterator<[number, string]>' display, got: {msg}"
        );
    }

    #[test]
    fn renamed_variables_have_the_same_display() {
        // Vary names and identifier choice — the rule must be structural,
        // not keyed on any identifier spelling in the test fixture.
        let msg = ts2322_message(
            "const xs = [1, 2];\nconst itr = xs.values();\nconst sink: string = itr;\n",
        );
        assert!(
            msg.contains("'ArrayIterator<number>'"),
            "expected rename-invariant ArrayIterator<number> display, got: {msg}"
        );
    }

    #[test]
    fn tuple_values_displays_array_iterator_with_element_union() {
        let msg = ts2322_message(
            "const t: [string, number] = [\"x\", 1];\nconst it = t.values();\nconst bad: number = it;\n",
        );
        assert!(
            msg.contains("'ArrayIterator<string | number>'"),
            "expected 'ArrayIterator<string | number>' display, got: {msg}"
        );
        assert!(
            !msg.contains("IteratorObject<"),
            "diagnostic must not expose IteratorObject in tuple iterator display, got: {msg}"
        );
    }

    #[test]
    fn declared_array_iterator_alias_display_is_unaffected() {
        // Negative control: when the source type is the literal lib alias
        // `ArrayIterator<T>` (not the synthesized stamped form), the display
        // must remain `ArrayIterator<T>`. This guards against accidentally
        // repainting unrelated direct uses of the alias through the new
        // display-alias path.
        let msg =
            ts2322_message("declare const it: ArrayIterator<string>;\nconst bad: number = it;\n");
        assert!(
            msg.contains("'ArrayIterator<string>'"),
            "directly declared ArrayIterator<T> must still display as 'ArrayIterator<T>', got: {msg}"
        );
    }

    #[test]
    fn iterator_next_value_type_is_preserved() {
        // The structural type stays correct after the display fix: the
        // iterator's `next().value` is `string | undefined` (the element
        // type joined with the iterator-return intrinsic), not the
        // displayed-but-wrong `IteratorObject<...>` shape.
        let msg =
            ts2322_message("const a = [\"x\"];\nconst v: number = a.values().next().value;\n");
        assert!(
            msg.contains("'string | undefined'"),
            "expected next().value type 'string | undefined', got: {msg}"
        );
    }
}
