//! Base-class substitution in the unmatched-property message, and the head
//! message that substitution selects.
//!
//! Structural rule (oracled against `typescript@7.0.2`, the conformance pin):
//! the unmatched-property line names an object type by the base **class** that
//! actually contributes the relevant shape, not by the relation endpoint, in
//! two independent positions —
//!
//! - *target side*: the class that DECLARES the missing property, when the
//!   target does not declare it itself (`class T extends B {}` against a
//!   source missing `B`'s `p` reads `required in type 'B'`);
//! - *source side*: the base class a source inherits its whole member surface
//!   from, when the source declares nothing of its own (`interface S extends
//!   A {}` reads `missing in type 'A'`).
//!
//! When either substitution fires, `tsc` keeps a top-level `TS2322`
//! (`Type 'S' is not assignable to type 'T'.`) naming the relation's own
//! endpoints and demotes the missing-property line to a nested elaboration.
//! The standalone `TS2741` survives only when both named types ARE the
//! endpoints.
//!
//! Base **interface** heritage does not substitute: `interface T extends B {}`
//! reads `required in type 'T'` and keeps the standalone `TS2741`. Visibility
//! is not the discriminator either — a `private`/`#private` member declared
//! directly on the target keeps `TS2741`, and a plain `public` member
//! inherited from a base class takes the `TS2322` head.
use super::*;

/// Which side of the unmatched-property message a base class replaced, and the
/// name it replaced it with. Both sides can substitute independently; either
/// one selects the `TS2322` head.
#[derive(Default)]
pub(super) struct MissingPropertyBaseClassNames {
    pub(super) source: Option<String>,
    pub(super) target: Option<String>,
}

/// The rendered names the two candidate messages are built from: the endpoint
/// pair the `TS2322` head reads, and the pair the nested missing-property line
/// reads when no base class replaced it.
pub(super) struct MissingPropertyMessageParts<'p> {
    pub(super) property: &'p str,
    pub(super) endpoint_source: &'p str,
    pub(super) endpoint_target: &'p str,
    pub(super) nested_source: &'p str,
    pub(super) nested_target: &'p str,
}

impl MissingPropertyBaseClassNames {
    pub(super) const fn substituted(&self) -> bool {
        self.source.is_some() || self.target.is_some()
    }
}

impl<'a> CheckerState<'a> {
    /// Resolve both base-class substitutions for one unmatched property.
    ///
    /// `target_candidates` is searched in order because the reason's recorded
    /// member type and the relation target can each carry the property shape
    /// depending on how the failure was produced; the first candidate that
    /// knows the property decides.
    pub(super) fn missing_property_base_class_names(
        &mut self,
        source: TypeId,
        target_candidates: &[TypeId],
        property_name: tsz_common::interner::Atom,
    ) -> MissingPropertyBaseClassNames {
        MissingPropertyBaseClassNames {
            source: self.source_shape_base_class_display(source),
            target: self
                .missing_property_owner_base_class_display(target_candidates, property_name),
        }
    }

    /// The class that declares `property_name` when the target itself does not.
    ///
    /// Returns `None` when the target declares the property (the endpoint name
    /// is correct then), when the declaring symbol is an interface rather than
    /// a class, or when no declaring symbol is recorded.
    fn missing_property_owner_base_class_display(
        &mut self,
        target_candidates: &[TypeId],
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        for &candidate in target_candidates {
            let Some(prop_info) = self.property_info_for_display(candidate, property_name) else {
                continue;
            };
            let Some(parent_id) = prop_info.parent_id else {
                continue;
            };
            if self.type_own_symbol_for_display(candidate) == Some(parent_id) {
                // Declared on the target itself: `tsc` names the endpoint.
                return None;
            }
            if !self.symbol_has_class_declaration(parent_id) {
                // Interface heritage is flattened into the endpoint's name.
                return None;
            }
            return self
                .ctx
                .binder
                .get_symbol(parent_id)
                .map(|symbol| symbol.escaped_name.clone());
        }
        None
    }

    /// The base class a source type inherits its whole member surface from.
    ///
    /// Only a source that declares nothing of its own substitutes: once it
    /// contributes a member, `tsc` names the source endpoint. The base must be
    /// a class — an `extends` clause naming an interface keeps the endpoint.
    ///
    /// "Declares nothing of its own" is read off the declaration's own member
    /// list, NOT off the resolved shape's property set: a method
    /// (`interface I extends C { other(x: any): any }`) is an own member that
    /// the resolved property set does not report as one, and treating such an
    /// interface as member-less both renamed the source to `C` and promoted the
    /// head, turning a correct `TS2741` into a false-positive `TS2322` on
    /// `compiler/interfaceExtendsClassWithPrivate1.ts`. Every declaration of a
    /// merged symbol must be empty for the symbol to count as member-less.
    fn source_shape_base_class_display(&mut self, source: TypeId) -> Option<String> {
        let shape = diagnostic_query::object_shape_for_type(self.ctx.types, source)?;
        let own_symbol = shape.symbol?;
        let symbol = self.ctx.binder.get_symbol(own_symbol)?;
        let declarations = symbol.declarations.clone();
        if declarations
            .iter()
            .any(|&decl_idx| self.declaration_has_own_members(decl_idx))
        {
            return None;
        }
        for decl_idx in declarations {
            if let Some(base_type) = self.heritage_base_class_instance_type(decl_idx) {
                return Some(self.format_type_diagnostic(base_type));
            }
        }
        None
    }

    /// Whether an interface or class declaration lists any member of its own.
    /// A declaration that is neither reports no members and cannot make its
    /// symbol non-member-less on its own.
    fn declaration_has_own_members(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if let Some(interface) = self.ctx.arena.get_interface(node) {
            return !interface.members.nodes.is_empty();
        }
        if let Some(class) = self.ctx.arena.get_class(node) {
            return !class.members.nodes.is_empty();
        }
        false
    }

    /// The instance type of the first `extends` base of `decl_idx` that
    /// resolves to a class declaration. Interface and class declarations both
    /// carry heritage clauses and both are walked.
    fn heritage_base_class_instance_type(&mut self, decl_idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(decl_idx)?;
        let heritage_clauses = self
            .ctx
            .arena
            .get_interface(node)
            .and_then(|interface| interface.heritage_clauses.clone())
            .or_else(|| {
                self.ctx
                    .arena
                    .get_class(node)
                    .and_then(|class| class.heritage_clauses.clone())
            })?;
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                if let Some(base_type) = self.heritage_entry_class_instance_type(type_idx) {
                    return Some(base_type);
                }
            }
        }
        None
    }

    /// Resolve one `extends` entry to the instance type of the class it names,
    /// or `None` when it names anything else.
    fn heritage_entry_class_instance_type(&mut self, type_idx: NodeIndex) -> Option<TypeId> {
        let type_node = self.ctx.arena.get(type_idx)?;
        let expr_idx = if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
            expr_type_args.expression
        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            self.ctx
                .arena
                .get_type_ref(type_node)
                .map_or(type_idx, |type_ref| type_ref.type_name)
        } else {
            type_idx
        };
        let base_sym_id = self.resolve_heritage_symbol(expr_idx)?;
        let base_symbol = self
            .get_cross_file_symbol(base_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(base_sym_id))?;
        for base_decl_idx in base_symbol.declarations.clone() {
            let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                continue;
            };
            let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                continue;
            };
            return Some(self.get_class_instance_type(base_decl_idx, base_class));
        }
        None
    }

    /// The symbol an object type is named by, across the shape kinds a
    /// diagnostic target can arrive as.
    fn type_own_symbol_for_display(&self, ty: TypeId) -> Option<tsz_binder::SymbolId> {
        diagnostic_query::object_shape_for_type(self.ctx.types, ty)
            .and_then(|shape| shape.symbol)
            .or_else(|| {
                diagnostic_query::callable_shape_for_type(self.ctx.types, ty)
                    .and_then(|shape| shape.symbol)
            })
            .or_else(|| {
                diagnostic_query::lazy_def_id(self.ctx.types, ty)
                    .and_then(|def_id| self.ctx.def_symbol_identity(def_id))
                    .map(|(sym_id, _)| sym_id)
            })
    }

    /// Whether any declaration of `symbol_id` is a class. A symbol merged from
    /// a class and an interface counts as a class: the class side owns the
    /// nominal shape the message names.
    fn symbol_has_class_declaration(&self, symbol_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self
            .get_cross_file_symbol(symbol_id)
            .or_else(|| self.ctx.binder.get_symbol(symbol_id))
        else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| self.ctx.arena.get_class(node).is_some())
        })
    }

    /// Build the unmatched-property diagnostic under the head the base-class
    /// substitution selects: a standalone `TS2741` when neither side
    /// substituted, otherwise a `TS2322` naming the endpoints with the
    /// missing-property line nested beneath it.
    pub(super) fn missing_property_diagnostic_with_base_class_head(
        &mut self,
        anchor: (String, u32, u32),
        names: &MissingPropertyBaseClassNames,
        parts: MissingPropertyMessageParts<'_>,
    ) -> Diagnostic {
        let (file_name, start, length) = anchor;
        let nested_source = names.source.as_deref().unwrap_or(parts.nested_source);
        let nested_target = names.target.as_deref().unwrap_or(parts.nested_target);
        let nested_message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[parts.property, nested_source, nested_target],
        );
        if !names.substituted() {
            return Diagnostic::error(
                file_name,
                start,
                length,
                nested_message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }
        let head_message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[parts.endpoint_source, parts.endpoint_target],
        );
        let mut diagnostic = Diagnostic::error(
            file_name,
            start,
            length,
            head_message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        diagnostic.push_elaboration_in_span(
            start,
            length,
            nested_message,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            0,
        );
        diagnostic
    }
}
