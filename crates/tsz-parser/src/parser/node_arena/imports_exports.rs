//! `NodeArena` constructors for import/export-related nodes (import and
//! export declarations, import clauses, named imports, specifiers,
//! export assignments, and import attributes).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ExportAssignmentData, ExportDeclData, ImportAttributeData, ImportAttributesData,
    ImportClauseData, ImportDeclData, NamedImportsData, NodeArenaInner, SpecifierData,
};

impl NodeArenaInner {
    /// Add an import declaration node
    pub fn add_import_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportDeclData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.import_clause, parent);
        self.set_parent(data.module_specifier, parent);
        self.set_parent(data.attributes, parent);
        push_data_node!(self, parent, kind, pos, end, import_decls, data)
    }

    /// Add an import clause node
    pub fn add_import_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportClauseData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.named_bindings, parent);
        push_data_node!(self, parent, kind, pos, end, import_clauses, data)
    }

    /// Add a namespace/named imports node
    pub fn add_named_imports(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedImportsData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent_list(&data.elements, parent);
        push_data_node!(self, parent, kind, pos, end, named_imports, data)
    }

    /// Add an import/export specifier node
    pub fn add_specifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SpecifierData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.property_name, parent);
        self.set_parent(data.name, parent);
        push_data_node!(self, parent, kind, pos, end, specifiers, data)
    }

    /// Add an export declaration node
    pub fn add_export_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportDeclData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.export_clause, parent);
        self.set_parent(data.module_specifier, parent);
        self.set_parent(data.attributes, parent);
        push_data_node!(self, parent, kind, pos, end, export_decls, data)
    }

    /// Add an export assignment node
    pub fn add_export_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportAssignmentData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, export_assignments, data)
    }

    /// Add an import attributes node
    pub fn add_import_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributesData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.elements, parent);
        push_data_node!(self, parent, kind, pos, end, import_attributes, data)
    }

    /// Add an import attribute node
    pub fn add_import_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.value, parent);
        push_data_node!(self, parent, kind, pos, end, import_attribute, data)
    }
}
