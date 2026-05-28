//! `NodeArena` constructors for import/export-related nodes (import and
//! export declarations, import clauses, named imports, specifiers,
//! export assignments, and import attributes).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ExportAssignmentData, ExportDeclData, ExtendedNodeInfo, ImportAttributeData,
    ImportAttributesData, ImportClauseData, ImportDeclData, NamedImportsData, Node, NodeArena,
    SpecifierData,
};

impl NodeArena {
    /// Add an import declaration node
    pub fn add_import_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let import_clause = data.import_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.len_u32(self.import_decls.len());
        self.import_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(import_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add an import clause node
    pub fn add_import_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportClauseData,
    ) -> NodeIndex {
        let name = data.name;
        let named_bindings = data.named_bindings;

        let data_index = self.len_u32(self.import_clauses.len());
        self.import_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(named_bindings, parent);
        parent
    }

    /// Add a namespace/named imports node
    pub fn add_named_imports(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedImportsData,
    ) -> NodeIndex {
        let name = data.name;
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.named_imports.len());
        self.named_imports.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import/export specifier node
    pub fn add_specifier(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SpecifierData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;

        let data_index = self.len_u32(self.specifiers.len());
        self.specifiers.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        parent
    }

    /// Add an export declaration node
    pub fn add_export_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let export_clause = data.export_clause;
        let module_specifier = data.module_specifier;
        let attributes = data.attributes;

        let data_index = self.len_u32(self.export_decls.len());
        self.export_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(export_clause, parent);
        self.set_parent(module_specifier, parent);
        self.set_parent(attributes, parent);
        parent
    }

    /// Add an export assignment node
    pub fn add_export_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExportAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let expression = data.expression;

        let data_index = self.len_u32(self.export_assignments.len());
        self.export_assignments.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(expression, parent);
        parent
    }

    /// Add an import attributes node
    pub fn add_import_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributesData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.import_attributes.len());
        self.import_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an import attribute node
    pub fn add_import_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ImportAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let value = data.value;

        let data_index = self.len_u32(self.import_attribute.len());
        self.import_attribute.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(value, parent);
        parent
    }
}
