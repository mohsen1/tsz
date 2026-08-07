//! Accessor declaration validation (abstract consistency, setter parameters).

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

// =============================================================================
// Accessor Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    pub(crate) fn paired_getter_member_for_setter(
        &self,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<NodeIndex> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        self.paired_getter_in_members(&class_info.member_nodes, setter_accessor)
    }

    /// Find the `get` accessor paired with `setter_accessor` among `members`.
    ///
    /// The class path passes the enclosing class's member nodes; the
    /// object-literal path passes the literal's elements. Pairing is by
    /// property name (so `get 'a'()` pairs with `set a()`, and `get 0x20()`
    /// with `set 3.2e1()`, exactly as `tsc` pairs them), falling back to
    /// computed-name symbol identity when the name is not a literal.
    pub(crate) fn paired_getter_in_members(
        &self,
        members: &[NodeIndex],
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<NodeIndex> {
        if let Some(setter_name) = self.get_property_name(setter_accessor.name) {
            for &member_idx in members {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(getter) = self.ctx.arena.get_accessor(member_node)
                    && let Some(getter_name) = self.get_property_name(getter.name)
                    && getter_name == setter_name
                {
                    return Some(member_idx);
                }
            }
            return None;
        }

        let setter_sym = self.resolve_computed_name_symbol(setter_accessor.name);
        setter_sym?;

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(getter) = self.ctx.arena.get_accessor(member_node)
                && self.resolve_computed_name_symbol(getter.name) == setter_sym
            {
                return Some(member_idx);
            }
        }

        None
    }

    pub(crate) fn contextual_setter_parameter_types_for_class_accessor(
        &mut self,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<Vec<Option<tsz_solver::TypeId>>> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let members = class_info.member_nodes.clone();
        self.contextual_setter_parameter_types_in_members(&members, setter_accessor)
    }

    /// The contextual types of `setter_accessor`'s parameters, given the
    /// accessor's sibling `members`.
    ///
    /// An unannotated `set` accessor parameter takes the paired `get`
    /// accessor's type — its return annotation when it has one, otherwise the
    /// type inferred from its body. Returns `None` when the parameter is
    /// annotated (the annotation wins) or when there is no paired getter (the
    /// parameter stays implicitly `any` and keeps its `TS7006`/`TS7032`).
    pub(crate) fn contextual_setter_parameter_types_in_members(
        &mut self,
        members: &[NodeIndex],
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<Vec<Option<tsz_solver::TypeId>>> {
        let &first_param_idx = setter_accessor.parameters.nodes.first()?;
        let param = self.ctx.arena.get_parameter_at(first_param_idx)?;
        if param.type_annotation.is_some() && !self.ctx.is_js_file() {
            return None;
        }

        let getter_member_idx = self.paired_getter_in_members(members, setter_accessor)?;
        let getter_node = self.ctx.arena.get(getter_member_idx)?;
        let getter = self.ctx.arena.get_accessor(getter_node)?;

        let getter_type = if getter.type_annotation.is_some() {
            self.get_type_from_type_node(getter.type_annotation)
        } else if getter.body.is_some() {
            self.infer_getter_return_type(getter.body)
        } else {
            return None;
        };

        let mut contextual_types = vec![None; setter_accessor.parameters.nodes.len()];
        contextual_types[0] = Some(getter_type);
        Some(contextual_types)
    }

    /// Find the `set` accessor paired with `getter_accessor` among `members`.
    ///
    /// The mirror of `paired_getter_in_members`: given a getter, find its
    /// setter by property name, falling back to computed-name symbol
    /// identity when the name is not a literal.
    /// The `set` accessor paired with `getter_accessor` in the enclosing class.
    ///
    /// Distinct from `contextual_getter_return_type_for_class_accessor`, which
    /// answers only for an *annotated* setter because it is looking for a type.
    /// This answers whether the pair exists at all, which is what decides where
    /// `tsc` anchors a missing property type: a getter with any paired setter is
    /// never the blame site.
    pub(crate) fn paired_setter_member_for_getter(
        &self,
        getter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<NodeIndex> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let members = class_info.member_nodes.clone();
        self.paired_setter_in_members(&members, getter_accessor)
    }

    /// Whether the `get` accessor paired with `setter_accessor` supplies the
    /// property's type — by a return-type annotation, or by a body to infer one
    /// from.
    ///
    /// This is deliberately *not* the same question as "does a paired getter
    /// exist". A paired getter always contextually types the setter's parameter
    /// (so it always suppresses TS7006), but it only supplies the *property's*
    /// type when it has an annotation or a body. A bodyless, unannotated getter
    /// — the ordinary shape in a `declare class` — supplies nothing, and `tsc`
    /// then reports TS7032 on the setter.
    pub(crate) fn paired_getter_supplies_property_type(
        &self,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> bool {
        let Some(getter_idx) = self.paired_getter_member_for_setter(setter_accessor) else {
            return false;
        };
        let Some(getter_node) = self.ctx.arena.get(getter_idx) else {
            return false;
        };
        let Some(getter) = self.ctx.arena.get_accessor(getter_node) else {
            return false;
        };
        getter.type_annotation.is_some() || getter.body.is_some()
    }

    pub(crate) fn paired_setter_in_members(
        &self,
        members: &[NodeIndex],
        getter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<NodeIndex> {
        if let Some(getter_name) = self.get_property_name(getter_accessor.name) {
            for &member_idx in members {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                    && let Some(setter) = self.ctx.arena.get_accessor(member_node)
                    && let Some(setter_name) = self.get_property_name(setter.name)
                    && setter_name == getter_name
                {
                    return Some(member_idx);
                }
            }
            return None;
        }

        let getter_sym = self.resolve_computed_name_symbol(getter_accessor.name);
        getter_sym?;

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(setter) = self.ctx.arena.get_accessor(member_node)
                && self.resolve_computed_name_symbol(setter.name) == getter_sym
            {
                return Some(member_idx);
            }
        }

        None
    }

    /// The `noImplicitAny` accessor family (`TS7033`/`TS7032`/`TS7006`) for
    /// *type members* — the members of an `interface` or of a type literal.
    ///
    /// Structural rule, identical to the class arm in
    /// `state_checking_members/ambient_signature_checks.rs`: a `get`/`set` pair
    /// shares **one** property type. It comes from the getter's return type —
    /// annotated, or inferred from a body — if there is one, else from the
    /// setter's parameter annotation. When nothing supplies it, `tsc` reports
    /// `TS7032` on the **setter**; the getter is the blame site (`TS7033`) only
    /// when it has no paired setter at all. tsz does this through the accessor
    /// checker, which already owns the same rule for class members and for
    /// object-literal accessor elements.
    ///
    /// Class members reach that rule as *declarations*
    /// (`check_accessor_declaration_with_request`, which also reasons about
    /// bodies, modifiers and enclosing-class ambientness). Interface and
    /// type-literal members are never declaration-checked, so they need this
    /// separate entry point — but they must not re-derive the rule, only the
    /// container-specific parts of it.
    ///
    /// Three properties of the rule that a matrix built one axis at a time
    /// misses, all of them load-bearing here:
    ///
    /// 1. The getter supplies the property type when it has an annotation
    ///    **or a body**. A type member can never have a body (`TS1183` claims
    ///    it), but the condition is written out rather than assumed away so the
    ///    two arms stay the same rule.
    /// 2. `TS7006` and `TS7032` need **separate** suppression flags. Any paired
    ///    getter contextually types the setter's *parameter* (suppressing
    ///    `TS7006`), but only an annotated-or-bodied one supplies the
    ///    *property's* type (gating `TS7032`). `interface I { get g(); set g(v); }`
    ///    is exactly the shape that separates them: `TS7032` alone, no `TS7006`.
    /// 3. It is the annotation's **presence**, not its type — `set g(v: any)`
    ///    is clean.
    pub(crate) fn check_type_member_accessor_implicit_any(
        &mut self,
        member_idx: NodeIndex,
        siblings: &[NodeIndex],
    ) {
        if !self.ctx.no_implicit_any() || self.is_js_file() {
            return;
        }
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };
        let kind = member_node.kind;
        if kind != syntax_kind_ext::GET_ACCESSOR && kind != syntax_kind_ext::SET_ACCESSOR {
            return;
        }
        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
            return;
        };

        if kind == syntax_kind_ext::GET_ACCESSOR {
            // TS7033: an unannotated, bodyless getter resolves to `any`. Any
            // paired setter moves the blame to the setter (see above), whether
            // or not that setter is itself annotated.
            if accessor.type_annotation.is_none()
                && accessor.body.is_none()
                && self.paired_setter_in_members(siblings, accessor).is_none()
                && let Some(accessor_name) = self.property_name_for_error(accessor.name)
            {
                self.error_at_node_msg(
                    accessor.name,
                    diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_GET_ACCESSOR_LACKS_A_RETURN_TYPE_AN,
                    &[&accessor_name],
                );
            }
            return;
        }

        let paired_getter = self.paired_getter_in_members(siblings, accessor);
        // Property (2): the *property* type comes from the getter only when the
        // getter names one. Reusing `paired_getter.is_some()` here would silence
        // TS7032 on every pair, which is the shape the issue reported.
        let paired_getter_supplies_type = paired_getter
            .and_then(|getter_idx| self.ctx.arena.get(getter_idx))
            .and_then(|getter_node| self.ctx.arena.get_accessor(getter_node))
            .is_some_and(|getter| getter.type_annotation.is_some() || getter.body.is_some());
        let accessor_name = accessor.name;

        if accessor.parameters.nodes.is_empty() {
            // A zero-parameter setter (`set a() {}` in an interface/type
            // literal) is grammatically invalid (`TS1049` fires separately)
            // but still "lacks a parameter type annotation" for `TS7032`
            // purposes — the loop below never runs for this shape.
            if !paired_getter_supplies_type
                && let Some(prop_name) = self.property_name_for_error(accessor_name)
            {
                let message = format!(
                    "Property '{prop_name}' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                );
                self.error_at_node(
                    accessor_name,
                    &message,
                    diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                );
            }
            return;
        }

        for (param_index, &param_idx) in accessor.parameters.nodes.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // TS7006 on the parameter: a paired getter contextually types it.
            self.maybe_report_implicit_any_parameter(param, paired_getter.is_some(), param_index);

            // TS7032 on the setter name: nothing gave the property a type.
            if param.type_annotation.is_none()
                && !paired_getter_supplies_type
                && let Some(prop_name) = self.property_name_for_error(accessor_name)
            {
                let message = format!(
                    "Property '{prop_name}' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                );
                self.error_at_node(
                    accessor_name,
                    &message,
                    diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                );
            }
        }
    }

    pub(crate) fn contextual_getter_return_type_for_class_accessor(
        &mut self,
        getter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<tsz_solver::TypeId> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let members = class_info.member_nodes.clone();
        self.contextual_getter_return_type_in_members(&members, getter_accessor)
    }

    /// The paired `set` accessor's annotated parameter type, given the
    /// accessor's sibling `members` — mirrors tsc's
    /// `isGetAccessorWithAnnotatedSetAccessor` → `getContextualReturnType`,
    /// which contextually types an unannotated getter's body from its paired
    /// setter's declared parameter type.
    ///
    /// Only an *annotated* setter parameter participates: an unannotated one
    /// is itself waiting on the getter's return type (see
    /// `contextual_setter_parameter_types_in_members`), so joining it here
    /// would recurse the pair back through the getter it is trying to type.
    pub(crate) fn contextual_getter_return_type_in_members(
        &mut self,
        members: &[NodeIndex],
        getter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<tsz_solver::TypeId> {
        let setter_member_idx = self.paired_setter_in_members(members, getter_accessor)?;
        let setter_node = self.ctx.arena.get(setter_member_idx)?;
        let setter = self.ctx.arena.get_accessor(setter_node)?;
        let &first_param_idx = setter.parameters.nodes.first()?;
        let param = self.ctx.arena.get_parameter_at(first_param_idx)?;
        if param.type_annotation.is_none() {
            return None;
        }
        Some(self.get_type_from_type_node(param.type_annotation))
    }

    /// The paired setter's annotated parameter type for the `get` accessor at
    /// `getter_idx`, resolved generically from the accessor's own enclosing
    /// container — a class (via `self.ctx.enclosing_class`) or an object
    /// literal (via the parent node's elements). Used to contextually type an
    /// unannotated getter's body without requiring the caller to already know
    /// which container it is checking.
    pub(crate) fn contextual_getter_return_type_from_pair(
        &mut self,
        getter_idx: NodeIndex,
        getter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> Option<tsz_solver::TypeId> {
        // Check the getter's own immediate parent first (not just whether
        // `enclosing_class` happens to be set): an object-literal getter
        // nested inside a class method body has a non-`None` enclosing
        // class, but its siblings are the literal's elements, not the
        // class's members. Getting this backwards could pair the getter
        // with an unrelated same-named setter on the surrounding class.
        let parent_idx = self.ctx.arena.get_extended(getter_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let elements = self
                .ctx
                .arena
                .get_literal_expr(parent_node)?
                .elements
                .nodes
                .clone();
            return self.contextual_getter_return_type_in_members(&elements, getter_accessor);
        }

        self.contextual_getter_return_type_for_class_accessor(getter_accessor)
    }

    // =========================================================================
    // Accessor Abstract Consistency
    // =========================================================================

    /// Check that accessor pairs (get/set) have consistent abstract modifiers.
    ///
    /// Validates that if a getter and setter for the same property both exist,
    /// they must both be abstract or both be non-abstract.
    /// Emits TS1044 on mismatched accessor abstract modifiers.
    ///
    /// ## Parameters:
    /// - `members`: Slice of class member node indices to check
    ///
    /// ## Validation:
    /// - Collects all getters and setters by property name
    /// - Checks for abstract/non-abstract mismatches
    /// - Reports TS1044 on both accessors if mismatch found
    pub(crate) fn check_accessor_abstract_consistency(&mut self, members: &[NodeIndex]) {
        // Collect getters and setters by name
        #[derive(Default)]
        struct AccessorPair {
            getter: Option<(NodeIndex, bool)>, // (name_node_idx, is_abstract)
            setter: Option<(NodeIndex, bool)>,
        }

        let mut accessors: FxHashMap<String, AccessorPair> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if (node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
            {
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                let name_node_idx = accessor.name;

                // Get accessor name (use resolved variant for computed names like [G.B])
                if let Some(name) = self.get_property_name_resolved(accessor.name) {
                    let pair = accessors.entry(name).or_default();
                    if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        pair.getter = Some((name_node_idx, is_abstract));
                    } else {
                        pair.setter = Some((name_node_idx, is_abstract));
                    }
                }
            }
        }

        // Check for abstract mismatch
        for (_, pair) in accessors {
            if let (
                Some((getter_name_idx, getter_abstract)),
                Some((setter_name_idx, setter_abstract)),
            ) = (pair.getter, pair.setter)
                && getter_abstract != setter_abstract
            {
                // Report error on accessor names (tsc points to the property name)
                self.error_at_node(
                    getter_name_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT,
                );
                self.error_at_node(
                    setter_name_idx,
                    "Accessors must both be abstract or non-abstract.",
                    diagnostic_codes::ACCESSORS_MUST_BOTH_BE_ABSTRACT_OR_NON_ABSTRACT,
                );
            }
        }
    }

    // =========================================================================
    // Setter Parameter Validation
    // =========================================================================

    /// The `set`-accessor *parameter grammar* arms tsz owns, for every
    /// container a `set` accessor can be written in.
    ///
    /// `tsc` runs these from `checkGrammarAccessor`, which is reached for a
    /// `set` accessor wherever one can be written — class member, object
    /// literal, interface member, type-literal member — because the rule reads
    /// the accessor's own signature and never the container.
    ///
    /// `checkGrammarAccessor` reports **at most one** diagnostic per accessor:
    /// it is a chain of early returns, in this order.
    ///
    /// | # | condition | code | emitted by |
    /// | --- | --- | --- | --- |
    /// | 1 | accessor has type parameters | `TS1094` | parser |
    /// | 2 | value-parameter count is not exactly 1 | `TS1049` | parser |
    /// | 3 | setter has a return type annotation | `TS1095` | parser |
    /// | 4 | parameter is a rest parameter | **`TS1053`** | **here** |
    /// | 5 | parameter is optional | `TS1051` | parser |
    /// | 6 | parameter has an initializer | **`TS1052`** | **here** |
    ///
    /// tsz splits that chain across two layers — rows 1/2/3/5 are emitted by
    /// the parser, rows 4 and 6 by this checker — so the ordering cannot be a
    /// local early return inside either one. This function therefore re-tests
    /// the *earlier* links' conditions before emitting, from the same
    /// structural facts the parser uses. Without that, `set p(...v: T[]): void`
    /// draws `TS1095` **and** `TS1053` where `tsc` reports `TS1095` alone.
    ///
    /// The value-parameter count in row 2 excludes a leading `this` parameter
    /// (`tsc`'s `getSetAccessorValueParameter`): `set p(this: C, ...v: T[])` is
    /// a one-value-parameter setter and still reaches row 4. A `this`
    /// parameter on an accessor is separately illegal (`TS2784`), which does
    /// not suppress this family.
    ///
    /// Split out of `check_setter_parameter` so the non-class containers reach
    /// the same rule rather than re-deriving it. `check_setter_parameter` stays
    /// the class-declaration entry point and additionally owns the
    /// `noImplicitAny` family (`TS7006`/`TS7032`), whose suppression *does*
    /// depend on the container and on a paired getter — which is why only the
    /// grammar half is shared.
    pub(crate) fn check_setter_parameter_grammar(&mut self, accessor_idx: NodeIndex) {
        let Some(accessor_node) = self.ctx.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.ctx.arena.get_accessor(accessor_node) else {
            return;
        };
        let accessor_name = accessor.name;
        let has_type_parameters = accessor
            .type_parameters
            .as_ref()
            .is_some_and(|list| !list.nodes.is_empty());
        let has_return_type = accessor.type_annotation.is_some();

        // Row 1: `TS1094` claims an accessor with type parameters.
        if has_type_parameters {
            return;
        }

        // Row 2: the value parameters are the declared ones minus a leading
        // `this` parameter, which is not a value parameter.
        let value_params: Vec<NodeIndex> = accessor
            .parameters
            .nodes
            .iter()
            .copied()
            .filter(|&param_idx| {
                self.ctx
                    .arena
                    .get(param_idx)
                    .and_then(|node| self.ctx.arena.get_parameter(node))
                    .is_none_or(|param| !self.is_this_parameter_name(param.name))
            })
            .collect();
        let [param_idx] = value_params[..] else {
            // `TS1049` claims any other count.
            return;
        };

        // Row 3: `TS1095` claims a setter with a return type annotation.
        if has_return_type {
            return;
        }

        let Some(param) = self
            .ctx
            .arena
            .get(param_idx)
            .and_then(|node| self.ctx.arena.get_parameter(node))
        else {
            return;
        };
        let (is_rest, is_optional, has_initializer, param_name) = (
            param.dot_dot_dot_token,
            param.question_token,
            param.initializer.is_some(),
            param.name,
        );

        // Row 4: rest parameter. tsc anchors TS1053 at the `...` token (the
        // parameter's start), not at the parameter name.
        if is_rest {
            // Raw parameter start: the span normalizer would re-anchor a
            // parameter at its name, which is exactly the divergence.
            if let Some(start) = self.ctx.arena.get(param_idx).map(|node| node.pos) {
                self.error_at_position(
                    start,
                    3,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER,
                );
            } else {
                self.error_at_node(
                    param_idx,
                    "A 'set' accessor cannot have rest parameter.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER,
                );
            }
            return;
        }

        // Row 5: `TS1051` claims an optional parameter.
        if is_optional {
            return;
        }

        // Row 6: initializer. tsc points at the accessor name (e.g. `X` in
        // `set X(v = 0)`), falling back to the parameter when the accessor has
        // no usable name node.
        if has_initializer {
            let error_node = if accessor_name.is_some() {
                accessor_name
            } else {
                param_name
            };
            self.error_at_node(
                error_node,
                "A 'set' accessor parameter cannot have an initializer.",
                diagnostic_codes::A_SET_ACCESSOR_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
            );
        }
    }

    /// Check setter parameter constraints (TS1052, TS1053, TS7006).
    ///
    /// This function validates that setter parameters comply with TypeScript rules:
    /// - TS1052: Setter parameters cannot have initializers
    /// - TS1053: Setter cannot have rest parameters
    /// - TS7006: Parameters without type annotations are implicitly 'any'
    ///
    /// When a setter has a paired getter, the setter parameter type is inferred
    /// from the getter return type, so TS7006 is suppressed.
    ///
    /// ## Error Messages:
    /// - TS1052: "A 'set' accessor parameter cannot have an initializer."
    /// - TS1053: "A 'set' accessor cannot have rest parameter."
    ///
    /// `paired_getter_supplies_type` is the TS7032 half of `has_paired_getter`:
    /// a paired getter always contextually types the parameter (TS7006), but
    /// only one with an annotation or a body gives the *property* a type
    /// (TS7032). See `paired_getter_supplies_property_type`.
    pub(crate) fn check_setter_parameter(
        &mut self,
        parameters: &[NodeIndex],
        has_paired_getter: bool,
        paired_getter_supplies_type: bool,
        accessor_jsdoc: Option<&str>,
        accessor_name: Option<NodeIndex>,
    ) {
        if parameters.is_empty() {
            // A zero-parameter setter (`set y() {}`) is grammatically invalid
            // (`TS1049` fires separately, from `check_setter_parameter_grammar`)
            // but `tsc` still reports `TS7032`: "lacks a parameter type
            // annotation" is true of a setter with no parameter at all, not
            // only of one whose sole parameter is unannotated. The loop below
            // never runs for this shape, so the check needs its own arm.
            let property_type_supplied = has_paired_getter && paired_getter_supplies_type;
            if !property_type_supplied
                && self.ctx.no_implicit_any()
                && let Some(name_idx) = accessor_name
            {
                let prop_name = self.parameter_name_for_error(name_idx);
                let message = format!(
                    "Property '{prop_name}' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                );
                self.error_at_node(
                    name_idx,
                    &message,
                    diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                );
            }
            return;
        }

        for &param_idx in parameters {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Check for implicit any (error 7006)
            // When a setter has a paired getter, the parameter type is inferred from
            // the getter return type, so it's contextually typed (suppress TS7006).
            // Also check for inline JSDoc @param/@type annotations and accessor-level
            // JSDoc @param annotations (e.g., `/** @param {string} value */ set p(value)`).
            let jsdoc_declares_type = self.param_has_inline_jsdoc_type(param_idx)
                || accessor_jsdoc.is_some_and(|jsdoc| {
                    let pname = self.parameter_name_for_error(param.name);
                    Self::jsdoc_has_param_type(jsdoc, &pname)
                        || Self::jsdoc_type_tag_declares_callable(jsdoc)
                });
            let has_jsdoc = has_paired_getter || jsdoc_declares_type;
            self.maybe_report_implicit_any_parameter(param, has_jsdoc, 0);

            // Also report TS7032 on the setter name if the parameter implicitly has type any.
            //
            // A paired getter suppresses this only when it actually supplies the
            // property's type. `declare class A { get g(); set g(v); }` has a
            // paired getter and still reports TS7032 on the setter, because
            // neither accessor names a type — the pair shares one property type
            // and nothing provides it. The TS7006 flag above cannot be reused
            // here: it folds in `has_paired_getter` unconditionally, which is
            // right for contextually typing the parameter and wrong for deciding
            // whether the property has a type at all.
            let property_type_supplied =
                jsdoc_declares_type || (has_paired_getter && paired_getter_supplies_type);
            if param.type_annotation.is_none()
                && !property_type_supplied
                && self.ctx.no_implicit_any()
                && let Some(name_idx) = accessor_name
            {
                let prop_name = self.parameter_name_for_error(name_idx);
                let message = format!(
                    "Property '{prop_name}' implicitly has type 'any', because its set accessor lacks a parameter type annotation."
                );
                self.error_at_node(
                        name_idx,
                        &message,
                        diagnostic_codes::PROPERTY_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_ITS_SET_ACCESSOR_LACKS_A_PARAMETER_TYPE,
                    );
            }
        }
    }

    // =========================================================================
    // Getter/Setter Type Compatibility (TS2322) — inferred types only
    // =========================================================================

    /// Check getter/setter type compatibility when the getter has no explicit
    /// return type annotation (its type is inferred from the body).
    ///
    /// Since TS 5.1, getters and setters may have completely unrelated types
    /// when **both** have explicit type annotations. However, when a getter's
    /// return type is *inferred*, it must still be compatible with the setter's
    /// explicit parameter type annotation.
    ///
    /// Example (error — getter type inferred):
    /// ```typescript
    /// class C {
    ///     get bar() { return 0; }      // TS2322: number not assignable to string
    ///     set bar(n: string) {}
    /// }
    /// ```
    ///
    /// Example (no error — both explicitly annotated, TS 5.1):
    /// ```typescript
    /// class C {
    ///     get x(): A<number> { return this.data; }
    ///     set x(v: A<string>) { this.data = v; }
    /// }
    /// ```
    pub(crate) fn check_accessor_type_compatibility(&mut self, members: &[NodeIndex]) {
        // In JS/checkJs, accessor pairs are co-inferred from the property shape and
        // backing writes. JSDoc on a setter can still affect emit/comments, but it
        // does not force the inferred getter type through this TS2322 check.
        if self.ctx.is_js_file() {
            return;
        }

        type GetterInfo = Option<(NodeIndex, NodeIndex, NodeIndex)>; // (name, body, type_ann)
        type SetterInfo = Option<(NodeIndex, NodeIndex)>; // (param_type_ann, param_idx)

        let mut pairs: FxHashMap<String, (GetterInfo, SetterInfo)> = FxHashMap::default();

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                continue;
            };

            // Use get_property_name_resolved to handle computed property names
            // that resolve to literals (e.g., const enum members like [G.B]).
            // tsc pairs accessor names via type-based resolution, not just syntax.
            let Some(name) = self.get_property_name_resolved(accessor.name) else {
                continue;
            };

            // tsc only pairs get/set accessors when the name is late-bindable
            // (a string/numeric-literal or unique-symbol computed name, or a
            // plain identifier). A computed name whose expression type is
            // non-literal (`[1 << 6]` -> `number`, `[s]` -> `string`) is *not*
            // late-bound, so tsc never merges the pair and runs no getter/setter
            // type-compatibility check. `is_late_bound_member_name` returns true
            // exactly for those non-determinable computed names; skip them so we
            // don't synthesize a spurious pair (and TS2322/TS2741).
            if self.is_late_bound_member_name(accessor.name) {
                continue;
            }

            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                pairs.entry(name).or_default().0 =
                    Some((accessor.name, accessor.body, accessor.type_annotation));
            } else if node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(&first_param) = accessor.parameters.nodes.first()
                && let Some(param_node) = self.ctx.arena.get(first_param)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                pairs.entry(name).or_default().1 = Some((param.type_annotation, first_param));
            }
        }

        for (_name, (getter, setter)) in pairs {
            let Some((getter_name, getter_body, getter_type_ann)) = getter else {
                continue;
            };
            let Some((setter_type_ann, _setter_param)) = setter else {
                continue;
            };
            // Only check when the setter has an explicit type annotation.
            // When the setter has no annotation, its type is inferred from the getter.
            if setter_type_ann == NodeIndex::NONE {
                continue;
            }
            // TS 5.1: when the getter ALSO has an explicit return type annotation,
            // unrelated types are allowed — skip the check.
            if getter_type_ann != NodeIndex::NONE {
                continue;
            }
            // Skip abstract accessors — no body to anchor the diagnostic.
            if getter_body == NodeIndex::NONE {
                continue;
            }

            let getter_return_type = self.infer_getter_return_type(getter_body);
            let setter_param_type = self.get_type_from_type_node(setter_type_ann);

            if getter_return_type != setter_param_type
                && getter_return_type != tsz_solver::TypeId::ANY
                && setter_param_type != tsz_solver::TypeId::ANY
            {
                let diag_idx = self
                    .find_first_return_in_block(getter_body)
                    .unwrap_or(getter_name);
                self.check_assignable_or_report_at(
                    getter_return_type,
                    setter_param_type,
                    diag_idx,
                    diag_idx,
                );
            }
        }
    }

    /// Find the first return statement inside a block body.
    fn find_first_return_in_block(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.ctx.arena.get(body_idx)?;
        let block = self.ctx.arena.get_block(body_node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            if stmt_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// Check getter/setter type compatibility for object literal accessors.
    ///
    /// When a getter in an object literal has no explicit return type annotation,
    /// its type is inferred from the body and must be compatible with the setter's
    /// explicit parameter type annotation. This mirrors `check_accessor_type_compatibility`
    /// but works on object literal element nodes instead of class members.
    pub(crate) fn check_object_literal_accessor_type_compatibility(
        &mut self,
        elements: &[NodeIndex],
    ) {
        // In JS/checkJs, accessor pairs are co-inferred from the property shape.
        if self.ctx.is_js_file() {
            return;
        }

        type GetterInfo = Option<(NodeIndex, NodeIndex, NodeIndex)>; // (name, body, type_ann)
        type SetterInfo = Option<(NodeIndex, NodeIndex)>; // (param_type_ann, param_idx)

        let mut pairs: FxHashMap<String, (GetterInfo, SetterInfo)> = FxHashMap::default();

        for &elem_idx in elements {
            let Some(node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                continue;
            };

            // Use get_property_name (not get_property_name_resolved) to avoid
            // pairing getter/setter on non-literal computed names like [0 + 1].
            // tsc only pairs accessors whose names are syntactically resolvable
            // (identifiers, string/number literals, well-known symbols).
            let Some(name) = self.get_property_name(accessor.name) else {
                continue;
            };

            if node.kind == syntax_kind_ext::GET_ACCESSOR {
                pairs.entry(name).or_default().0 =
                    Some((accessor.name, accessor.body, accessor.type_annotation));
            } else if node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(&first_param) = accessor.parameters.nodes.first()
                && let Some(param_node) = self.ctx.arena.get(first_param)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                pairs.entry(name).or_default().1 = Some((param.type_annotation, first_param));
            }
        }

        for (_name, (getter, setter)) in pairs {
            let Some((getter_name, getter_body, getter_type_ann)) = getter else {
                continue;
            };
            let Some((setter_type_ann, _setter_param)) = setter else {
                continue;
            };
            // Only check when the setter has an explicit type annotation.
            if setter_type_ann == NodeIndex::NONE {
                continue;
            }
            // TS 5.1: when the getter ALSO has an explicit return type annotation,
            // unrelated types are allowed — skip the check.
            if getter_type_ann != NodeIndex::NONE {
                continue;
            }
            // Skip abstract accessors — no body to anchor the diagnostic.
            if getter_body == NodeIndex::NONE {
                continue;
            }

            let getter_return_type = self.infer_getter_return_type(getter_body);
            let setter_param_type = self.get_type_from_type_node(setter_type_ann);

            if getter_return_type != setter_param_type
                && getter_return_type != tsz_solver::TypeId::ANY
                && setter_param_type != tsz_solver::TypeId::ANY
            {
                let diag_idx = self
                    .find_first_return_in_block(getter_body)
                    .unwrap_or(getter_name);
                self.check_assignable_or_report_at(
                    getter_return_type,
                    setter_param_type,
                    diag_idx,
                    diag_idx,
                );
            }
        }
    }
}
