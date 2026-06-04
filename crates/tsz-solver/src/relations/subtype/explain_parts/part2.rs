impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Explain why an indexed object type assignment failed.
    fn explain_indexed_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target_shape: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        // First check properties
        if let Some(reason) = self.explain_object_failure(
            source,
            target,
            &source_shape.properties,
            source_shape_id,
            &target_shape.properties,
        ) {
            return Some(reason);
        }

        // Check string index signature
        if let Some(ref t_string_idx) = target_shape.string_index {
            match &source_shape.string_index {
                Some(s_string_idx) => {
                    if s_string_idx.readonly && !t_string_idx.readonly {
                        return Some(SubtypeFailureReason::TypeMismatch {
                            source_type: source,
                            target_type: target,
                        });
                    }
                    if !self
                        .check_subtype(s_string_idx.value_type, t_string_idx.value_type)
                        .is_true()
                    {
                        return self.make_index_sig_reason(
                            "string",
                            s_string_idx.value_type,
                            t_string_idx.value_type,
                        );
                    }
                }
                None => {
                    // Class/interface types must have an explicit string index
                    // signature — a number index alone is not enough (see
                    // check_string_index_compatibility for the full rationale).
                    if self.requires_explicit_declared_index_signature(source_shape) {
                        return Some(SubtypeFailureReason::MissingIndexSignature {
                            index_kind: "string",
                        });
                    }

                    for prop in &source_shape.properties {
                        // Strip `undefined` from optional property types when checking
                        // against index signatures, matching tsc behavior.
                        let prop_type = if prop.optional {
                            crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
                        } else {
                            prop.type_id
                        };
                        if !self
                            .check_subtype(prop_type, t_string_idx.value_type)
                            .is_true()
                        {
                            return self.make_index_sig_reason(
                                "string",
                                prop_type,
                                t_string_idx.value_type,
                            );
                        }
                    }
                }
            }
        }

        // Check number index signature
        if let Some(ref t_number_idx) = target_shape.number_index {
            if let Some(ref s_number_idx) = source_shape.number_index {
                if s_number_idx.readonly && !t_number_idx.readonly {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                if !self
                    .check_subtype(s_number_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return self.make_index_sig_reason(
                        "number",
                        s_number_idx.value_type,
                        t_number_idx.value_type,
                    );
                }
            } else if let Some(ref s_string_idx) = source_shape.string_index {
                if s_string_idx.readonly && !t_number_idx.readonly {
                    return Some(SubtypeFailureReason::TypeMismatch {
                        source_type: source,
                        target_type: target,
                    });
                }
                if !self
                    .check_subtype(s_string_idx.value_type, t_number_idx.value_type)
                    .is_true()
                {
                    return self.make_index_sig_reason(
                        "number",
                        s_string_idx.value_type,
                        t_number_idx.value_type,
                    );
                }
            } else if self.shape_or_type_requires_declared_index_signature(source_shape, source) {
                return Some(SubtypeFailureReason::MissingIndexSignature {
                    index_kind: "number",
                });
            }
        }

        if let Some(reason) =
            self.explain_properties_against_index_signatures(&source_shape.properties, target_shape)
        {
            return Some(reason);
        }

        None
    }

    fn explain_object_with_index_to_object_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: &ObjectShape,
        source_shape_id: ObjectShapeId,
        target_props: &[PropertyInfo],
    ) -> Option<SubtypeFailureReason> {
        for t_prop in target_props {
            if let Some(sp) =
                self.lookup_property(&source_shape.properties, Some(source_shape_id), t_prop.name)
            {
                // Check nominal identity for private/protected properties.
                // `private` requires the same declaration; `protected` is
                // hierarchical (a derived class may widen it to public) —
                // decided by the shared `nominal_member_origin_ok`.
                if t_prop.visibility != Visibility::Public {
                    if !self.nominal_member_origin_ok(
                        sp.parent_id,
                        t_prop.parent_id,
                        t_prop.visibility,
                    ) {
                        return Some(SubtypeFailureReason::PropertyNominalMismatch {
                            property_name: t_prop.name,
                        });
                    }
                }
                // Cannot assign private/protected source to public target
                else if sp.visibility != Visibility::Public {
                    return Some(SubtypeFailureReason::PropertyVisibilityMismatch {
                        property_name: t_prop.name,
                        source_visibility: sp.visibility,
                        target_visibility: t_prop.visibility,
                    });
                }

                // NOTE: TypeScript allows readonly source to satisfy mutable target
                // (readonly is a constraint on the reference, not structural compatibility)

                // Check property type compatibility before the optional/required
                // mismatch: TS2327 ("Property 'x' is optional ... but required ...")
                // only applies when the read types are compatible and optionality is
                // the sole failure. An incompatible read type must surface the
                // "Types of property 'x' are incompatible." chain instead.
                let source_type = self.optional_property_type(sp);
                let target_type = self.optional_property_type(t_prop);
                let allow_bivariant = sp.is_method || t_prop.is_method;
                if !self
                    .check_subtype_with_method_variance(source_type, target_type, allow_bivariant)
                    .is_true()
                {
                    let nested = self.explain_failure_with_method_variance(
                        source_type,
                        target_type,
                        allow_bivariant,
                    );
                    return Some(SubtypeFailureReason::PropertyTypeMismatch {
                        property_name: t_prop.name,
                        source_property_type: source_type,
                        target_property_type: target_type,
                        nested_reason: nested.map(Box::new),
                    });
                }

                if sp.optional && !t_prop.optional {
                    return Some(SubtypeFailureReason::OptionalPropertyRequired {
                        property_name: t_prop.name,
                    });
                }
                if !t_prop.readonly
                    && !sp.readonly
                    && (sp.has_split_accessor() || t_prop.has_split_accessor())
                {
                    let source_write = self.optional_property_write_type(sp);
                    let target_write = self.optional_property_write_type(t_prop);
                    if !self
                        .check_subtype_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        let nested = self.explain_failure_with_method_variance(
                            target_write,
                            source_write,
                            allow_bivariant,
                        );
                        return Some(SubtypeFailureReason::PropertyTypeMismatch {
                            property_name: t_prop.name,
                            source_property_type: source_write,
                            target_property_type: target_write,
                            nested_reason: nested.map(Box::new),
                        });
                    }
                }
                continue;
            }

            let mut checked = false;
            let target_type = self.optional_property_type(t_prop);

            if utils::is_numeric_property_name(self.interner, t_prop.name)
                && let Some(number_idx) = &source_shape.number_index
            {
                checked = true;
                if number_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        number_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    let nested_reason = self
                        .explain_failure_with_method_variance(
                            number_idx.value_type,
                            target_type,
                            t_prop.is_method,
                        )
                        .map(Box::new);
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "number",
                        source_value_type: number_idx.value_type,
                        target_value_type: target_type,
                        nested_reason,
                    });
                }
            }

            if let Some(string_idx) = &source_shape.string_index {
                checked = true;
                if string_idx.readonly && !t_prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: t_prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        string_idx.value_type,
                        target_type,
                        t_prop.is_method,
                    )
                    .is_true()
                {
                    let nested_reason = self
                        .explain_failure_with_method_variance(
                            string_idx.value_type,
                            target_type,
                            t_prop.is_method,
                        )
                        .map(Box::new);
                    return Some(SubtypeFailureReason::IndexSignatureMismatch {
                        index_kind: "string",
                        source_value_type: string_idx.value_type,
                        target_value_type: target_type,
                        nested_reason,
                    });
                }
            }

            if !checked && !t_prop.optional {
                return Some(SubtypeFailureReason::MissingProperty {
                    property_name: t_prop.name,
                    source_type: source,
                    target_type: target,
                });
            }
        }

        None
    }

    fn explain_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> Option<SubtypeFailureReason> {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return None;
        }

        for prop in source {
            // Strip `undefined` from optional property types when checking against
            // index signatures, matching tsc behavior.
            let prop_type = if prop.optional {
                crate::narrowing::utils::remove_undefined(self.interner, prop.type_id)
            } else {
                prop.type_id
            };
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                if is_numeric {
                    if !number_idx.readonly && prop.readonly {
                        return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                            property_name: prop.name,
                        });
                    }
                    if !self
                        .check_subtype_with_method_variance(
                            prop_type,
                            number_idx.value_type,
                            allow_bivariant,
                        )
                        .is_true()
                    {
                        return self.make_index_sig_reason(
                            "number",
                            prop_type,
                            number_idx.value_type,
                        );
                    }
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return Some(SubtypeFailureReason::ReadonlyPropertyMismatch {
                        property_name: prop.name,
                    });
                }
                if !self
                    .check_subtype_with_method_variance(
                        prop_type,
                        string_idx.value_type,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return self.make_index_sig_reason("string", prop_type, string_idx.value_type);
                }
            }
        }

        None
    }
}
