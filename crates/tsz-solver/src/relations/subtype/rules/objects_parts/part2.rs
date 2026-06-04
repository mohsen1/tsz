impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check that source properties are compatible with target index signatures.
    ///
    /// When a target has an index signature, all source properties must satisfy it:
    /// - String index: All string-named properties must be compatible with index type
    /// - Number index: All numerically-named properties must be compatible with index type
    pub(crate) fn check_properties_against_index_signatures(
        &mut self,
        source: &[PropertyInfo],
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        let string_index = target
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type != TypeId::SYMBOL);
        let symbol_index = target
            .string_index
            .as_ref()
            .filter(|idx| idx.key_type == TypeId::SYMBOL);
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() && symbol_index.is_none() {
            return SubtypeResult::True;
        }

        for prop in source {
            // If target declares this property explicitly, its compatibility is
            // checked via named-property rules. Don't also force it through the
            // index signature value type (tsc behavior for intersections like
            // `{ a: X } & { [k: string]: Y }` where `a` is validated against `X`).
            if target
                .properties
                .binary_search_by_key(&prop.name, |p| p.name)
                .is_ok()
            {
                continue;
            }

            // For NUMBER index signatures, optional properties carry an implicit
            // `| undefined` that must flow into the check (e.g. `{ 1?: string }`
            // vs `{ [k: number]: string }` fails on `string | undefined <: string`).
            // For STRING index signatures, tsc strips the implicit `| undefined`
            // so `{ b?: number }` is assignable to `{ [k: string]: number }`.
            //
            // But when the property type is itself `undefined` (e.g.
            // `k1?: undefined`), stripping yields `never`, which is
            // vacuously assignable to anything and silences a real
            // mismatch. Use the original property type in that case so
            // the check still fires (tsc emits TS2322 for
            // `{ k1?: undefined }` against `{ [key: string]: string }`).
            let string_prop_type = if prop.optional {
                let stripped =
                    crate::narrowing::utils::remove_undefined(self.interner, prop.type_id);
                if stripped == TypeId::NEVER {
                    prop.type_id
                } else {
                    stripped
                }
            } else {
                prop.type_id
            };
            let string_prop_type =
                self.bind_property_receiver_this(source_receiver, string_prop_type);
            let number_prop_type = if prop.optional {
                self.bind_property_receiver_this(source_receiver, self.optional_property_type(prop))
            } else {
                string_prop_type
            };
            let allow_bivariant = prop.is_method;

            if let Some(number_idx) = number_index {
                let is_numeric = utils::is_numeric_property_name(self.interner, prop.name);
                let target_value =
                    self.bind_property_receiver_this(target_receiver, number_idx.value_type);
                if is_numeric
                    && !self
                        .check_subtype_with_method_variance(
                            number_prop_type,
                            target_value,
                            allow_bivariant,
                        )
                        .is_true()
                {
                    return SubtypeResult::False;
                }
                // Note: tsc does NOT reject readonly properties against writable
                // number index targets during assignability checks.
            }

            if let Some(string_idx) = string_index {
                if self.is_symbol_named_property(prop.name) {
                    continue;
                }
                // Non-matching keys aren't constrained: `click` ∉ `on${string}`, so
                // `{ click: number }` is fine against `{ [k: on${string}]: () => void }`.
                if !self.property_name_matches_string_index_key(prop.name, string_idx.key_type) {
                    continue;
                }
                // Note: We do NOT reject readonly source properties against writable
                // string index targets. A source with readonly properties (e.g., enum
                // namespaces, frozen objects) IS assignable to a target with a writable
                // index signature — the readonly constraint means the property can't be
                // written through the source reference, but assignability only checks
                // value type compatibility. tsc allows this pattern.
                let target_value =
                    self.bind_property_receiver_this(target_receiver, string_idx.value_type);
                if !self
                    .check_subtype_with_method_variance(
                        string_prop_type,
                        target_value,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }

            if let Some(symbol_idx) = symbol_index
                && self.is_symbol_named_property(prop.name)
            {
                let target_value =
                    self.bind_property_receiver_this(target_receiver, symbol_idx.value_type);
                if !self
                    .check_subtype_with_method_variance(
                        string_prop_type,
                        target_value,
                        allow_bivariant,
                    )
                    .is_true()
                {
                    return SubtypeResult::False;
                }
            }
        }

        SubtypeResult::True
    }

    /// Check simple object to object with index signature.
    ///
    /// Validates that a source object with only named properties is a subtype of
    /// a target object with an index signature. This requires:
    /// 1. All target named properties must have compatible source properties
    /// 2. All source properties must be compatible with the index signature type
    pub(crate) fn check_object_to_indexed(
        &mut self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        source_receiver: Option<TypeId>,
        target: &ObjectShape,
        target_receiver: Option<TypeId>,
    ) -> SubtypeResult {
        // Preserve the original shape identity when available. Named interface/class
        // types follow different index-signature rules than anonymous object types,
        // and rebuilding them as anonymous shapes loses that distinction.
        let source_shape = source_shape_id
            .map(|id| self.interner.object_shape(id))
            .unwrap_or_else(|| {
                ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: source.to_vec(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                }
                .into()
            });
        let source_receiver = self
            .receiver_type_from_shape_symbol(&source_shape)
            .or(source_receiver);
        let target_receiver = self
            .receiver_type_from_shape_symbol(target)
            .or(target_receiver);
        if !self
            .check_object_subtype(
                &source_shape,
                source_shape_id,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }

        // Named class/interface types require an explicit string index signature to
        // satisfy a string-indexed target — compatible properties alone are not enough.
        // Symbol-keyed indices and any-value targets are exempted (same shortcircuits
        // as check_string_index_compatibility).
        if target.string_index.as_ref().is_some_and(|idx| {
            idx.key_type != TypeId::SYMBOL
                && (self.disable_method_bivariance || !idx.value_type.is_any())
        }) && self.requires_explicit_declared_index_signature(&source_shape)
        {
            return SubtypeResult::False;
        }

        // A target number index signature requires the source to provide
        // number-compatible indexing via a number or string index signature.
        // A plain object with only named properties cannot satisfy arbitrary
        // numeric index access.
        if !self
            .check_number_index_compatibility(
                &source_shape,
                source_receiver,
                target,
                target_receiver,
            )
            .is_true()
        {
            return SubtypeResult::False;
        }
        self.check_properties_against_index_signatures(
            source,
            source_receiver,
            target,
            target_receiver,
        )
    }

    /// Get the effective type of an optional property for reading.
    ///
    /// Optional properties in TypeScript can be undefined even if their type doesn't
    /// explicitly include undefined. This function adds undefined to the type unless
    /// exactOptionalPropertyTypes is enabled.
    pub(crate) fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional && !self.exact_optional_property_types && self.strict_null_checks {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    /// Get the effective write type of an optional property.
    /// Falls back to `type_id` when `write_type` is `NONE` (readonly sentinel).
    pub(crate) fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        let write = if prop.write_type == TypeId::NONE {
            prop.type_id
        } else {
            prop.write_type
        };
        if prop.optional && !self.exact_optional_property_types && self.strict_null_checks {
            self.interner.union2(write, TypeId::UNDEFINED)
        } else {
            write
        }
    }

    /// Check if an object shape is a "weak type": all properties are optional,
    /// there is at least one property, and there are no index signatures.
    /// Weak types trigger TS2559 when the source has no common properties.
    fn is_weak_type_shape(shape: &ObjectShape) -> bool {
        !shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.iter().all(|p| p.optional)
    }

    /// Check if an object shape is the global `Object` interface from lib.d.ts.
    ///
    /// The global `Object` type is exempt from weak type checks because in tsc,
    /// all object types implicitly inherit `Object`'s properties (`toString`,
    /// `valueOf`, `constructor`, etc.). When tsc checks `hasCommonProperties`
    /// for the weak type rule, the target type's apparent type includes these
    /// inherited members, so `Object` and any weak type always share common
    /// properties. Our shapes don't include inherited members, so we exempt
    /// `Object` explicitly to match tsc behavior (see TypeScript PR #16047).
    fn is_global_object_shape(&self, shape: &ObjectShape) -> bool {
        // Object interface has exactly 7 properties: constructor, toString,
        // toLocaleString, valueOf, hasOwnProperty, isPrototypeOf,
        // propertyIsEnumerable. Use a tight cap to avoid matching derived
        // types like Boolean (8+ props) or Number (~10 props).
        if shape.properties.len() > 7 {
            return false;
        }
        let constructor = self.interner.intern_string("constructor");
        let has_own = self.interner.intern_string("hasOwnProperty");
        let is_proto = self.interner.intern_string("isPrototypeOf");
        shape.properties.iter().any(|p| p.name == constructor)
            && shape.properties.iter().any(|p| p.name == has_own)
            && shape.properties.iter().any(|p| p.name == is_proto)
    }

    /// `ObjectWithIndex` source vs `Tuple` target.
    ///
    /// Matches tsc's behavior for array-like interfaces assigned to a tuple
    /// type, e.g.
    /// ```ts
    /// interface StrNum extends Array<string|number> {
    ///   0: string;
    ///   1: number;
    ///   length: 2;
    /// }
    /// declare let x: [string, number];
    /// declare let y: StrNum;
    /// x = y;  // OK
    /// ```
    ///
    /// Iterates the target tuple's elements and looks up each by its numeric
    /// property name (`"0"`, `"1"`, ...) on the source shape. Optional/rest
    /// elements use the source's number index signature as a fallback.
    /// `length` is also checked when the tuple has a fixed arity and the
    /// source declares a numeric `length`.
    pub(crate) fn check_object_with_index_to_tuple(
        &mut self,
        source: &ObjectShape,
        source_receiver: Option<TypeId>,
        t_list: crate::types::TupleListId,
        target_type: TypeId,
    ) -> SubtypeResult {
        use crate::types::PropertyInfo;
        let target_elems = self.interner.tuple_list(t_list);
        let source_receiver =
            source_receiver.or_else(|| self.receiver_type_from_shape_symbol(source));

        for (i, t_elem) in target_elems.iter().enumerate() {
            // Variadic / rest elements aren't structurally implementable by
            // a fixed-property interface — bail out conservatively.
            if t_elem.rest {
                return SubtypeResult::False;
            }
            let prop_name = self.interner.intern_string(&i.to_string());
            let s_prop_opt = PropertyInfo::find_in_slice(&source.properties, prop_name);

            // Optional tuple slot can be satisfied by either a (matching) source
            // property OR by the source's number index signature.
            let s_type = if let Some(sp) = s_prop_opt {
                self.bind_property_receiver_this(source_receiver, self.optional_property_type(sp))
            } else if let Some(idx) = &source.number_index {
                self.bind_property_receiver_this(source_receiver, idx.value_type)
            } else if t_elem.optional {
                continue;
            } else {
                return SubtypeResult::False;
            };

            let t_type = t_elem.type_id;
            if !self.check_subtype(s_type, t_type).is_true() {
                return SubtypeResult::False;
            }
        }

        // Length check: when the target tuple has a fixed arity (no rest), the
        // source's `length` property type must be assignable to the literal
        // target length. tsc applies this strictly — `length: 2` is not
        // assignable to `length: 1`, and `length: number` is not assignable
        // to `length: 1` either.
        let length_atom = self.interner.intern_string("length");
        if let Some(s_length) = PropertyInfo::find_in_slice(&source.properties, length_atom)
            && target_elems.iter().all(|e| !e.rest)
        {
            let s_length_type = self.bind_property_receiver_this(source_receiver, s_length.type_id);
            let target_len = target_elems.len();
            let target_len_type = self.interner.literal_number(target_len as f64);
            if !self.check_subtype(s_length_type, target_len_type).is_true() {
                return SubtypeResult::False;
            }
        }

        let _ = target_type;
        SubtypeResult::True
    }
}
