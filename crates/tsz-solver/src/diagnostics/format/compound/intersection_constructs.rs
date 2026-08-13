impl<'a> TypeFormatter<'a> {
    pub(super) fn format_intersection(&mut self, members: &[TypeId]) -> String {
        if let Some(display) = self.format_non_nullable_type_parameter_intersection(members) {
            return display;
        }

        // Preserve the member order as stored in the TypeListId.
        // For intersections containing Lazy types (type parameters, type aliases),
        // normalize_intersection skips sorting and preserves source/declaration order.
        // tsc also preserves the original declaration order, so displaying members
        // in their stored order matches tsc's behavior.
        //
        // Do NOT flatten `{ a } & { b }` into `{ a; b }` at the display layer.
        // tsc's `typeToString` preserves the intersection form (`A & B`); a merged
        // single-object display is only produced when the type is already stored
        // as a single object (e.g. via spread/apparent-type computation), not
        // when an IntersectionType is printed directly.

        let formatted: Vec<String> = members
            .iter()
            .map(|&m| self.format_intersection_member(m))
            .collect();
        let formatted = Self::remove_redundant_index_signature_intersection_displays(formatted);
        formatted.join(" & ")
    }

    fn remove_redundant_index_signature_intersection_displays(
        formatted: Vec<String>,
    ) -> Vec<String> {
        if formatted.len() <= 1 {
            return formatted;
        }

        let sole_index_members: Vec<Option<String>> = formatted
            .iter()
            .map(|display| Self::sole_index_signature_object_member(display).map(str::to_string))
            .collect();

        let keep: Vec<bool> = formatted
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                !sole_index_members[idx].as_deref().is_some_and(|index_sig| {
                    formatted
                        .iter()
                        .enumerate()
                        .any(|(other_idx, other_display)| {
                            other_idx != idx
                                && sole_index_members[other_idx].as_deref() != Some(index_sig)
                                && Self::object_display_contains_member(other_display, index_sig)
                        })
                })
            })
            .collect();

        formatted
            .into_iter()
            .zip(keep)
            .filter_map(|(display, keep)| keep.then_some(display))
            .collect()
    }

    fn sole_index_signature_object_member(display: &str) -> Option<&str> {
        let inner = Self::object_display_inner(display)?;
        let mut members = inner
            .split(';')
            .map(str::trim)
            .filter(|member| !member.is_empty());
        let member = members.next()?;
        if members.next().is_some() {
            return None;
        }
        Self::is_index_signature_display(member).then_some(member)
    }

    fn object_display_contains_member(display: &str, expected: &str) -> bool {
        Self::object_display_inner(display)
            .into_iter()
            .flat_map(|inner| inner.split(';'))
            .map(str::trim)
            .any(|member| member == expected)
    }

    fn object_display_inner(display: &str) -> Option<&str> {
        display
            .strip_prefix("{ ")
            .and_then(|inner| inner.strip_suffix(" }"))
    }

    fn is_index_signature_display(member: &str) -> bool {
        member.contains('[') && member.contains("]:")
    }

    fn format_non_nullable_type_parameter_intersection(
        &mut self,
        members: &[TypeId],
    ) -> Option<String> {
        if members.len() != 2 {
            return None;
        }

        let is_empty_object = |id| {
            matches!(
                self.interner.lookup(id),
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id))
                    if {
                        let shape = self.interner.object_shape(shape_id);
                        shape.properties.is_empty()
                            && shape.string_index.is_none()
                            && shape.number_index.is_none()
                    }
            )
        };
        let is_type_parameter_like = |id| {
            matches!(
                self.interner.lookup(id),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            )
        };

        let type_param = match members {
            [left, right] if is_type_parameter_like(*left) && is_empty_object(*right) => *left,
            [left, right] if is_empty_object(*left) && is_type_parameter_like(*right) => *right,
            _ => return None,
        };
        Some(format!("NonNullable<{}>", self.format(type_param)))
    }

    pub(super) fn format_intersection_with_display(
        &mut self,
        members: &[TypeId],
        display_props: &[PropertyInfo],
    ) -> Option<String> {
        let replacement_idx = members
            .iter()
            .position(|&member| self.is_anonymous_object_intersection_member(member))?;

        Some(
            members
                .iter()
                .enumerate()
                .map(|(idx, &member)| {
                    if idx == replacement_idx {
                        self.format_intersection_member_with_display_props(member, display_props)
                    } else {
                        self.format_intersection_member(member)
                    }
                })
                .collect::<Vec<_>>()
                .join(" & "),
        )
    }

    /// Format an intersection member, parenthesizing types that contain infix
    /// operators (`|`, `=>`) to maintain correct precedence in `A & B` display.
    fn format_intersection_member(&mut self, id: TypeId) -> String {
        // tsc displays primitive members of intersection types using their apparent
        // (boxed) names: `number` → `Number`, `string` → `String`, `boolean` → `Boolean`.
        if self.capitalize_primitive_intersection_members {
            if id == TypeId::NUMBER {
                return "Number".to_string();
            }
            if id == TypeId::STRING {
                return "String".to_string();
            }
            if id == TypeId::BOOLEAN {
                return "Boolean".to_string();
            }
        }
        let formatted = self.format(id);
        let needs_parens = match self.interner.lookup(id) {
            // Unions: `A | B & C` is ambiguous
            Some(TypeData::Union(_)) => formatted.contains(" | "),
            // Function/callable types: `(a: T) => R & S` is ambiguous —
            // `&` would parse as part of the return type
            Some(TypeData::Function(_) | TypeData::Callable(_)) => formatted.contains(" => "),
            _ => false,
        };
        if needs_parens {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn is_anonymous_object_intersection_member(&mut self, id: TypeId) -> bool {
        if id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                self.resolve_object_shape_name(&shape).is_none()
            }
            _ => false,
        }
    }

    fn format_intersection_member_with_display_props(
        &mut self,
        id: TypeId,
        display_props: &[PropertyInfo],
    ) -> String {
        match self.interner.lookup(id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if self.resolve_object_shape_name(&shape).is_none() {
                    return self.format_object(display_props);
                }
                self.format_intersection_member(id)
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if self.resolve_object_shape_name(&shape).is_none() {
                    let mut display_shape = shape.as_ref().clone();
                    display_shape.properties = display_props.to_vec();
                    return self.format_object_with_index(&display_shape);
                }
                self.format_intersection_member(id)
            }
            _ => self.format_intersection_member(id),
        }
    }

    pub(super) fn format_tuple(&mut self, elements: &[TupleElement]) -> String {
        // Normalize: a tuple with a single concrete rest element `[...T[]]`
        // displays as `T[]` to match tsc's display behavior. Type-parameter
        // spreads must keep their tuple wrapper (`[...T]`) so diagnostics can
        // distinguish the mutable tuple view from the bare type parameter `T`.
        if elements.len() == 1 && elements[0].rest {
            if matches!(
                self.interner.lookup(elements[0].type_id),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            ) {
                // Fall through to normal tuple formatting.
            } else {
                let inner = self.format(elements[0].type_id);
                return inner.into_owned();
            }
        }
        let spliced = self.splice_concrete_rest_elements_for_display(elements);
        let elements: &[TupleElement] = spliced.as_deref().unwrap_or(elements);
        // Format each element's type independently, then apply namespace
        // disambiguation across elements whose display names collide —
        // e.g. `[Foo.Yep, Bar.Yep]` instead of `[Yep, Yep]` when two different
        // named types share the same short name.
        let type_strs: Vec<String> = elements
            .iter()
            .map(|e| {
                if e.optional && !e.rest {
                    self.format_optional_tuple_element_type(e.type_id, e.name.is_some())
                } else {
                    self.format(e.type_id).into_owned()
                }
            })
            .collect();
        let type_ids: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
        let disambiguated = self.disambiguate_union_member_names(&type_ids, type_strs);

        let formatted: Vec<String> = elements
            .iter()
            .zip(disambiguated)
            .map(|(e, type_str)| {
                let rest = if e.rest { "..." } else { "" };
                let optional = if e.optional && !e.rest && e.name.is_some() {
                    "?"
                } else {
                    ""
                };
                if let Some(name_atom) = e.name {
                    let name = self.atom(name_atom);
                    format!("{rest}{name}{optional}: {type_str}")
                } else {
                    format!("{rest}{type_str}{optional}")
                }
            })
            .collect();
        format!("[{}]", formatted.join(", "))
    }

    pub fn format_tuple_elements_for_diagnostic(&mut self, elements: &[TupleElement]) -> String {
        self.format_tuple(elements)
    }

    /// Splice a rest element whose type *evaluates* to a concrete tuple
    /// inline, mirroring `splice_concrete_tuple_spreads` (intern-time
    /// construction) at display time.
    ///
    /// A recursive-conditional spread (`[H, ...Split]` where `Split<S> = S
    /// extends ... ? [H, ...Split<R>] : [S]`) interns its rest element while
    /// the nested `Split<R>` application is still unresolved, so the
    /// construction-time splice can't fire (matching its own documented
    /// caveat: a self-referential spread never resolves to a concrete tuple
    /// *at that point*). By the time the whole recursion bottoms out, the
    /// rest element's `type_id` now evaluates to a concrete tuple, but the
    /// outer `Tuple` was already interned — nothing re-visits it to splice.
    /// `tsc` never has this two-phase gap (its recursive instantiation
    /// resolves the spread before the tuple is ever materialized), so this
    /// re-checks the same splice rule here, using an evaluated read rather
    /// than a raw lookup. Returns `None` when nothing changes, so the caller
    /// can keep formatting the original slice without an extra allocation.
    ///
    /// A single pass only resolves one recursion level: splicing in a resolved
    /// inner tuple can itself introduce a fresh rest element that is *still*
    /// an unresolved `Application` one level deeper (`Split<"a.b.c">` resolves
    /// its immediate rest to `["b", ...Split<"c">]`, whose own rest hasn't
    /// been re-checked yet). Re-run the splice on the result until it stops
    /// changing, bounded the same way `reducing_application_display` bounds
    /// its own alias-chain walk.
    fn splice_concrete_rest_elements_for_display(
        &self,
        elements: &[TupleElement],
    ) -> Option<Vec<TupleElement>> {
        let mut current = self.splice_concrete_rest_elements_for_display_once(elements)?;
        for _ in 0..8 {
            match self.splice_concrete_rest_elements_for_display_once(&current) {
                Some(next) => current = next,
                None => break,
            }
        }
        Some(current)
    }

    fn splice_concrete_rest_elements_for_display_once(
        &self,
        elements: &[TupleElement],
    ) -> Option<Vec<TupleElement>> {
        let parent_rest_count = elements.iter().filter(|e| e.rest).count();
        if parent_rest_count == 0 {
            return None;
        }
        let has_spliceable = elements.iter().any(|e| {
            e.rest && !e.optional && self.resolve_display_concrete_tuple(e.type_id).is_some()
        });
        if !has_spliceable {
            return None;
        }
        let last_index = elements.len().saturating_sub(1);
        let mut out: Vec<TupleElement> = Vec::with_capacity(elements.len());
        for (index, elem) in elements.iter().enumerate() {
            if elem.rest
                && !elem.optional
                && let Some(inner_list) = self.resolve_display_concrete_tuple(elem.type_id)
            {
                let inner = self.interner.tuple_list(inner_list);
                let (inner_rest_count, inner_has_optional) =
                    inner.iter().fold((0usize, false), |(rests, optional), e| {
                        (rests + usize::from(e.rest), optional || e.optional)
                    });
                // Same shape guard as the construction-time splice: only
                // inline a middle-position variadic inner tuple when it
                // won't push an optional element ahead of a required suffix,
                // and only ever keep the result down to one rest element.
                let splice_middle = inner_rest_count == 1 && !inner_has_optional;
                let splice_variadic =
                    parent_rest_count == 1 && (index == last_index || splice_middle);
                if inner_rest_count == 0 || splice_variadic {
                    out.extend(inner.iter().copied());
                    continue;
                }
            }
            out.push(*elem);
        }
        Some(out)
    }

    /// Resolve `type_id` (through one `readonly` wrapper and, for an
    /// `Application`, the same alias-reduction chase the formatter already
    /// applies when rendering that node directly) to a concrete tuple's
    /// element list for display splicing, or `None` if it doesn't resolve to
    /// one. Read-only: never interns or mutates a type.
    ///
    /// A bare `evaluate_type` cannot do this on its own — the display-time
    /// evaluator has no def-store resolver (see `reducing_application_display`'s
    /// own doc comment), so an `Application` referencing a generic alias
    /// (`Split<R>`) evaluates to itself unchanged. `application_display_reduction`
    /// is the mechanism the formatter already uses to turn that same
    /// `Application` into its evaluated form when formatting it as a standalone
    /// type; reusing it here is what lets a self-referential recursive
    /// conditional's rest element (`[H, ...Split]`) resolve to a concrete tuple.
    fn resolve_display_concrete_tuple(&self, type_id: TypeId) -> Option<TupleListId> {
        let unwrap_readonly = |id: TypeId| match self.interner.lookup(id) {
            Some(TypeData::ReadonlyType(inner)) => inner,
            _ => id,
        };
        let peeled = unwrap_readonly(type_id);
        if let Some(TypeData::Tuple(list_id)) = self.interner.lookup(peeled) {
            return Some(list_id);
        }
        let Some(TypeData::Application(app_id)) = self.interner.lookup(peeled) else {
            return None;
        };
        let app = self.interner.type_application(app_id);
        let reduced = match self.application_display_reduction(peeled, &app)? {
            super::application_reduction::ApplicationDisplayReduction::Type(reduced) => reduced,
            _ => return None,
        };
        match self.interner.lookup(unwrap_readonly(reduced)) {
            Some(TypeData::Tuple(list_id)) => Some(list_id),
            _ => None,
        }
    }

    pub(super) fn format_function(
        &mut self,
        shape_id: crate::types::FunctionShapeId,
        shape: &FunctionShape,
    ) -> String {
        let params = self.display_params_for_function_shape(shape_id, &shape.params);
        self.format_signature_with_predicate(
            &shape.type_params,
            &params,
            shape.return_type,
            &SignatureFormatOpts {
                this_type: shape.this_type,
                type_predicate: shape.type_predicate.as_ref(),
                is_construct: shape.is_constructor,
                is_abstract: false,
                separator: " =>",
            },
        )
    }

    pub(super) fn format_callable(&mut self, shape: &CallableShape) -> String {
        if !shape.construct_signatures.is_empty()
            && let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            if let Some(arena) = self.symbol_arena
                && let Some(sym) = arena.get(sym_id)
            {
                use tsz_binder::symbol_flags;
                let is_namespace =
                    sym.has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE);
                let is_enum = sym.has_any_flags(symbol_flags::ENUM);
                let is_class = sym.has_flags(symbol_flags::CLASS);
                let is_interface = sym.has_any_flags(symbol_flags::INTERFACE);
                // Classes have both CLASS and INTERFACE flags; only skip typeof
                // for pure interfaces (no CLASS flag). Class constructors should
                // display as "typeof ClassName" to match tsc.
                if (is_interface && !is_class) || (!is_namespace && !is_enum && !is_class) {
                    return name;
                }
            }
            return format!("typeof {name}");
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();
        if !has_index && shape.properties.is_empty() {
            if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
                let sig = &shape.call_signatures[0];
                return self.format_signature_with_predicate(
                    &sig.type_params,
                    &sig.params,
                    sig.return_type,
                    &SignatureFormatOpts {
                        this_type: sig.this_type,
                        type_predicate: sig.type_predicate.as_ref(),
                        is_construct: false,
                        is_abstract: false,
                        separator: " =>",
                    },
                );
            }
            if shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty() {
                let sig = &shape.construct_signatures[0];
                return self.format_signature(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    true,
                    shape.is_abstract,
                    " =>",
                );
            }
        }

        let mut parts = Vec::new();
        let mut call_signatures: Vec<_> = shape.call_signatures.iter().collect();
        if call_signatures.iter().any(|sig| sig.params.is_empty())
            && call_signatures.iter().any(|sig| !sig.params.is_empty())
        {
            call_signatures.sort_by_key(|sig| sig.params.len());
        }
        for sig in call_signatures {
            parts.push(self.format_call_signature(sig, false, false));
        }
        for sig in &shape.construct_signatures {
            parts.push(self.format_call_signature(sig, true, shape.is_abstract));
        }
        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            let key_type_str = self.format(idx.key_type);
            let value_str = self.format(idx.value_type);
            parts.push(format!("{ro}[{key_name}: {key_type_str}]: {value_str}"));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            let key_type_str = self.format(idx.key_type);
            let value_str = self.format(idx.value_type);
            parts.push(format!("{ro}[{key_name}: {key_type_str}]: {value_str}"));
        }
        let mut sorted_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        // Sort by declaration_order with a deterministic content-based tiebreak
        // (shared with `format_object`; see `compare_display_property_order`).
        sorted_props.sort_by(|a, b| self.compare_display_property_order(a, b));
        let mut prop_parts = Vec::with_capacity(sorted_props.len());
        for prop in sorted_props {
            prop_parts.push(self.format_property(prop));
        }

        if parts.is_empty() && prop_parts.is_empty() {
            return "{}".to_string();
        }

        Self::render_callable_member_list(parts, prop_parts)
    }

    /// Join a callable's signature parts and property parts into `{ ...; }`
    /// form, truncating a long property list exactly as tsc does: keep a head
    /// prefix, insert `... N more ...`, then always show the final property.
    /// Signatures (call/construct/index) are never elided.
    ///
    /// tsc truncates when its `approximateLength` counter passes
    /// `defaultMaximumTruncationLength` (160), checked at the start of each
    /// property. `HEAD_BUDGET` is a character proxy for that counter measured
    /// over rendered member spellings; the crossover lands near 200 characters,
    /// reproducing both `{ p1..p17; ... 6 more ...; p24 }` (24 short members)
    /// and the expando-function witness `{ (): void; …10 props…; ... 12 more
    /// ...; fromForInBodyNested: number; }`.
    fn render_callable_member_list(sig_parts: Vec<String>, prop_parts: Vec<String>) -> String {
        const HEAD_BUDGET: usize = 200;

        let total_props = prop_parts.len();
        // Signatures are always shown; seed the running length with them so the
        // property budget accounts for the space they occupy, as tsc's
        // `approximateLength` does.
        let mut cum = 0usize;
        for (i, part) in sig_parts.iter().enumerate() {
            cum += if i == 0 { part.len() } else { part.len() + 2 };
        }

        let leading = sig_parts.len();
        let mut head_props = 0usize;
        for (i, part) in prop_parts.iter().enumerate() {
            // tsc checks the running length at the START of each property, so a
            // property that pushes the head just past the budget is still shown;
            // truncation begins at the following property. Only truncate when at
            // least two properties besides the preserved tail remain unshown
            // (tsc's `i + 2 < properties.length`).
            if cum > HEAD_BUDGET && i + 2 < total_props {
                break;
            }
            cum += if leading == 0 && i == 0 {
                part.len()
            } else {
                part.len() + 2
            };
            head_props += 1;
        }

        let mut out = sig_parts;
        if head_props < total_props {
            out.extend(prop_parts[..head_props].iter().cloned());
            let omitted = total_props - head_props - 1;
            out.push(format!("... {omitted} more ..."));
            out.push(prop_parts[total_props - 1].clone());
        } else {
            out.extend(prop_parts);
        }
        format!("{{ {}; }}", out.join("; "))
    }

    fn format_call_signature(
        &mut self,
        sig: &CallSignature,
        is_construct: bool,
        is_abstract: bool,
    ) -> String {
        self.format_signature_with_predicate(
            &sig.type_params,
            &sig.params,
            sig.return_type,
            &SignatureFormatOpts {
                this_type: sig.this_type,
                type_predicate: sig.type_predicate.as_ref(),
                is_construct,
                is_abstract,
                separator: ":",
            },
        )
    }

    /// Render a single-signature function/callable as tsc's `signatureToString`
    /// colon form (`(x: number): number`), used for the `TS2772`
    /// overload-elaboration header. tsc's `signatureToString` defaults to the
    /// call-signature spelling even for construct signatures, so `new`/`abstract`
    /// prefixes are intentionally omitted (`new (x: number): C` renders as
    /// `(x: number): C`). Returns `None` when the type is not a function/callable
    /// with at least one signature.
    pub fn format_overload_signature(&mut self, func_type: TypeId) -> Option<String> {
        match self.interner.lookup(func_type)? {
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let params = self.display_params_for_function_shape(shape_id, &shape.params);
                Some(self.format_signature_with_predicate(
                    &shape.type_params,
                    &params,
                    shape.return_type,
                    &SignatureFormatOpts {
                        this_type: shape.this_type,
                        type_predicate: shape.type_predicate.as_ref(),
                        is_construct: false,
                        is_abstract: false,
                        separator: ":",
                    },
                ))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let sig = shape
                    .call_signatures
                    .first()
                    .or_else(|| shape.construct_signatures.first())?;
                Some(self.format_call_signature(sig, false, false))
            }
            _ => None,
        }
    }

    pub(super) fn format_conditional(&mut self, cond: &ConditionalType) -> String {
        let prev = self.preserve_optional_property_surface_syntax;
        self.preserve_optional_property_surface_syntax = true;
        let extends_type = self.format(cond.extends_type).into_owned();
        self.preserve_optional_property_surface_syntax = prev;
        format!(
            "{} extends {} ? {} : {}",
            self.format(cond.check_type),
            extends_type,
            self.format_conditional_branch(cond.true_type),
            self.format_conditional_branch(cond.false_type)
        )
    }

    fn format_conditional_branch(&mut self, type_id: TypeId) -> String {
        if let Some(TypeData::Infer(info)) = self.interner.lookup(type_id) {
            return self.atom(info.name).to_string();
        }
        self.format(type_id).into_owned()
    }

    pub(super) fn format_mapped(&mut self, mapped: &MappedType) -> String {
        if let Some(index_signature) = self.try_format_mapped_as_index_signature(mapped) {
            return index_signature;
        }
        let param_name = self.atom(mapped.type_param.name);
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        let optional_suffix = match mapped.optional_modifier {
            Some(MappedModifier::Add) => "?",
            Some(MappedModifier::Remove) => "-?",
            None => "",
        };
        let template_str = self.format(mapped.template);
        // tsc displays optional mapped types with `| undefined` appended to the
        // template: `{ [P in keyof T]?: T[P] | undefined; }`. Only add when the
        // optional modifier is Add and the template doesn't already contain undefined.
        let needs_undefined = if mapped.optional_modifier == Some(MappedModifier::Add)
            && mapped.template != TypeId::UNDEFINED
            && mapped.template != TypeId::ANY
            && mapped.template != TypeId::UNKNOWN
        {
            // Check if the template type already contains undefined
            // (e.g., if it's a union that includes undefined, any, or unknown).
            if let Some(TypeData::Union(members)) = self.interner.lookup(mapped.template) {
                let list = self.interner.type_list(members);
                !list
                    .as_ref()
                    .iter()
                    .any(|&m| m == TypeId::UNDEFINED || m == TypeId::ANY || m == TypeId::UNKNOWN)
            } else {
                true
            }
        } else {
            false
        };
        let template_display = if needs_undefined {
            format!("{template_str} | undefined")
        } else {
            template_str.into_owned()
        };
        let constraint_str = self.format(mapped.constraint);
        let as_clause = if let Some(name_type) = mapped.name_type {
            format!(" as {}", self.format(name_type))
        } else {
            String::new()
        };
        format!(
            "{{ {readonly_prefix}[{param_name} in {constraint_str}{as_clause}]{optional_suffix}: {template_display}; }}"
        )
    }

    fn try_format_mapped_as_index_signature(&mut self, mapped: &MappedType) -> Option<String> {
        if mapped.name_type.is_some() || mapped.optional_modifier.is_some() {
            return None;
        }
        let key_kind = match mapped.constraint {
            TypeId::STRING => "string",
            TypeId::NUMBER => "number",
            _ => return None,
        };
        if crate::contains_type_parameter_named(
            self.interner,
            mapped.template,
            mapped.type_param.name,
        ) {
            return None;
        }
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        Some(format!(
            "{{ {readonly_prefix}[x: {key_kind}]: {}; }}",
            self.format(mapped.template)
        ))
    }

    fn template_literal_spans_for_interpolation_display(
        &self,
        type_id: TypeId,
    ) -> Option<Vec<TemplateSpan>> {
        if let Some(TypeData::TemplateLiteral(spans_id)) = self.interner.lookup(type_id) {
            return Some(
                self.interner
                    .template_list(spans_id)
                    .iter()
                    .cloned()
                    .collect(),
            );
        }

        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(type_id) else {
            return None;
        };
        let def_store = self.def_store?;
        let def = def_store.get(def_id)?;
        if def.kind != crate::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        let TypeData::TemplateLiteral(spans_id) = self.interner.lookup(body)? else {
            return None;
        };
        Some(
            self.interner
                .template_list(spans_id)
                .iter()
                .cloned()
                .collect(),
        )
    }

    fn push_template_literal_text(result: &mut String, text: &str) {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        result.push_str(&escaped);
    }

    fn push_template_literal_interpolation(&mut self, result: &mut String, type_id: TypeId) {
        if let Some(nested) = self.template_literal_spans_for_interpolation_display(type_id) {
            self.push_template_literal_spans(result, &nested);
            return;
        }

        if let Some(TypeData::Literal(literal)) = self.interner.lookup(type_id) {
            match literal {
                LiteralValue::String(atom) | LiteralValue::BigInt(atom) => {
                    let text = self.atom(atom);
                    Self::push_template_literal_text(result, &text);
                }
                LiteralValue::Number(number) => {
                    let text = crate::utils::js_number_to_string(number.0);
                    Self::push_template_literal_text(result, &text);
                }
                LiteralValue::Boolean(value) => {
                    result.push_str(if value { "true" } else { "false" });
                }
            }
            return;
        }

        let formatted = self.format(type_id);
        let formatted = formatted.as_ref();
        if formatted.len() >= 2
            && formatted.starts_with('"')
            && formatted.ends_with('"')
            && !formatted[1..formatted.len() - 1].contains('"')
        {
            Self::push_template_literal_text(result, &formatted[1..formatted.len() - 1]);
            return;
        }

        result.push_str("${");
        result.push_str(formatted);
        result.push('}');
    }

    fn push_template_literal_spans(&mut self, result: &mut String, spans: &[TemplateSpan]) {
        for span in spans {
            match span {
                TemplateSpan::Text(text) => {
                    let text = self.atom(*text);
                    Self::push_template_literal_text(result, &text);
                }
                TemplateSpan::Type(type_id) => {
                    self.push_template_literal_interpolation(result, *type_id);
                }
            }
        }
    }

    pub(super) fn format_template_literal(&mut self, spans: &[TemplateSpan]) -> String {
        let mut result = String::from("`");
        self.push_template_literal_spans(&mut result, spans);
        result.push('`');
        result
    }
}
