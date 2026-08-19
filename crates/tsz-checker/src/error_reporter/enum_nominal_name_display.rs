//! Enum and nominal-type display names for diagnostics: qualified spelling
//! (`E.Member`, namespace-qualified `P.Q`), tsc's single-member-enum identity,
//! and same-name disambiguation via `import("module").Name` qualification.
use crate::state::CheckerState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn format_enum_member_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            return None;
        }
        self.format_qualified_enum_name_for_message(ty)
    }

    pub(super) fn format_qualified_enum_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        // tsc's default `typeToString` never namespace-qualifies an enum:
        // `namespace P { export enum Q {} }` renders `Q`, and a member renders
        // `Q.R`. The namespace-qualified spelling (`P.Q`) appears only through
        // `getTypeNameForErrorDisplay` (`TypeFormatFlags.UseFullyQualifiedType`),
        // which `reportRelationError` applies to a *generalized* literal-ish
        // source — see `format_fully_qualified_enum_name_for_message`.
        self.format_enum_name_for_message_internal(ty, false)
    }

    /// tsc `getTypeNameForErrorDisplay`: the enum naming with
    /// `UseFullyQualifiedType`, i.e. qualified through enclosing
    /// namespace/module declarations (`P.Q`). Reserved for the generalized
    /// relation-source display; every other message path uses the bare
    /// [`Self::format_qualified_enum_name_for_message`] spelling.
    pub(super) fn format_fully_qualified_enum_name_for_message(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        self.format_enum_name_for_message_internal(ty, true)
    }

    fn format_enum_name_for_message_internal(
        &mut self,
        ty: TypeId,
        fully_qualified: bool,
    ) -> Option<String> {
        // Accept both the evaluated `Enum` data and a still-deferred
        // `Lazy(DefId)` member ref (a type-position `E.X` annotation is
        // stabilized as a def whose binder symbol carries `ENUM_MEMBER`).
        // The lazy form is gated on the enum symbol flags below so ordinary
        // alias/interface refs never reach the enum naming.
        let enum_data_def = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty);
        let def_id = enum_data_def
            .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty))?;
        // Parent-edge path first: it covers member defs whose binder symbol is
        // not wired (`def_to_symbol_id_with_fallback` fails and the bare
        // member name would leak), and it encodes tsc's single-member
        // identity — a single-member enum's member type IS the enum type and
        // renders as the bare enum name. The environment lookup already falls
        // back to the shared definition store; the resolver's symbol-based
        // lookup covers canonicalized twins of the decl-site def that neither
        // map saw.
        if let Some(parent_id) = self
            .ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_enum_parent(def_id))
            .or_else(|| {
                tsz_solver::resolver::TypeResolver::get_enum_parent_def_id(&self.ctx, def_id)
            })
            && let Some(parent) = self.ctx.definition_store.get(parent_id)
        {
            let parent_name = self.ctx.types.resolve_atom_ref(parent.name).to_string();
            if parent.enum_members.len() == 1 {
                return Some(parent_name);
            }
            if let Some(def) = self.ctx.definition_store.get(def_id) {
                let member_name = self.ctx.types.resolve_atom_ref(def.name);
                return Some(format!("{parent_name}.{member_name}"));
            }
        }
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            let parent = self.ctx.binder.get_symbol(symbol.parent)?;
            // tsc single-member identity: the lone member's type IS the enum
            // type, so it displays as the bare enum name.
            if parent
                .exports
                .as_ref()
                .is_some_and(|members| members.len() == 1)
            {
                return Some(parent.escaped_name.clone());
            }
            return Some(format!("{}.{}", parent.escaped_name, symbol.escaped_name));
        }
        // A lazy ref that is not an enum member (interface/alias/namespace)
        // must not be renamed by the enum machinery.
        if enum_data_def.is_none() && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM) {
            return None;
        }
        if !fully_qualified {
            return Some(symbol.escaped_name.clone());
        }
        let mut parts = vec![symbol.escaped_name.clone()];
        let decl_idx = symbol.primary_declaration()?;
        let mut current = self.ctx.arena.get_extended(decl_idx)?.parent;

        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_decl) = self.ctx.arena.get_module(node)
                && let Some(name) = self.ctx.arena.get_identifier_text(module_decl.name)
            {
                parts.push(name.to_string());
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        if parts.len() == 1 {
            let mut current = symbol.parent;
            while current != tsz_binder::SymbolId::NONE {
                let parent = self.ctx.binder.get_symbol(current)?;
                if !parent.has_any_flags(
                    tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE
                        | tsz_binder::symbol_flags::ENUM,
                ) {
                    break;
                }
                parts.push(parent.escaped_name.clone());
                current = parent.parent;
            }
        }

        parts.reverse();
        Some(parts.join("."))
    }

    pub(super) fn format_disambiguated_enum_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.enum_symbol_from_enumish_type(ty)?;
        let other_sym = self.enum_symbol_from_enumish_type(other)?;
        if ty_sym == other_sym {
            return None;
        }

        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;

        if crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)
            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER))
        {
            return self.format_qualified_enum_name_for_message(ty);
        }

        if ty_symbol.escaped_name != other_symbol.escaped_name {
            return Some(ty_symbol.escaped_name.clone());
        }

        if self.is_exported_external_module_enum_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!(
                "import(\"{module_name}\").{}",
                ty_symbol.escaped_name
            ));
        }

        self.format_qualified_enum_name_for_message(ty)
    }

    pub(super) fn format_disambiguated_nominal_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.nominal_shape_symbol_for_display(ty)?;
        let other_sym = self.nominal_shape_symbol_for_display(other)?;
        if ty_sym == other_sym {
            return None;
        }
        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;
        if ty_symbol.escaped_name != other_symbol.escaped_name {
            return None;
        }
        if self.is_exported_external_module_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!(
                "import(\"{module_name}\").{}",
                ty_symbol.escaped_name
            ));
        }
        let qualified = self.qualified_symbol_name_for_message(ty_sym)?;
        if qualified == ty_symbol.escaped_name {
            return None;
        }
        Some(qualified)
    }

    fn nominal_shape_symbol_for_display(&mut self, ty: TypeId) -> Option<tsz_binder::SymbolId> {
        let resolved = self.evaluate_type_for_assignability(ty);
        [ty, resolved].into_iter().find_map(|candidate| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, candidate).or_else(
                || {
                    let def_id =
                        crate::query_boundaries::common::lazy_def_id(self.ctx.types, candidate)?;
                    self.ctx.def_to_symbol_id_with_fallback(def_id)
                },
            )
        })
    }

    pub(super) fn qualified_symbol_name_for_message(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut parts = vec![symbol.escaped_name.clone()];
        let mut current = symbol.parent;
        while current != tsz_binder::SymbolId::NONE {
            let parent = self.ctx.binder.get_symbol(current)?;
            if !parent.has_any_flags(
                tsz_binder::symbol_flags::NAMESPACE_MODULE
                    | tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::ENUM,
            ) {
                break;
            }
            parts.push(parent.escaped_name.clone());
            current = parent.parent;
        }
        parts.reverse();
        Some(parts.join("."))
    }

    fn is_exported_external_module_enum_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        self.is_exported_external_module_symbol(sym_id)
    }

    pub(super) fn is_exported_external_module_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.is_exported
            && symbol.decl_file_idx != u32::MAX
            && self
                .ctx
                .get_binder_for_file(symbol.decl_file_idx as usize)
                .is_some_and(tsz_binder::BinderState::is_external_module)
    }

    pub(super) fn module_specifier_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if let Some(specifier) = self.ctx.module_specifiers.get(&symbol.decl_file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }
}
