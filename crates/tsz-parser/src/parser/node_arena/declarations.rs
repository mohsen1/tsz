//! `NodeArena` constructors for top-level declarations:
//! functions, classes, interfaces, type aliases, enums, modules, and the
//! variable-statement / individual-variable-declaration pair.

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ClassData, EnumData, EnumMemberData, ExtendedNodeInfo, FunctionData, InterfaceData,
    ModuleBlockData, ModuleData, Node, NodeArena, TypeAliasData, VariableData,
    VariableDeclarationData,
};

impl NodeArena {
    /// Add a function node
    pub fn add_function(&mut self, kind: u16, pos: u32, end: u32, data: FunctionData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.functions.len());
        self.functions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a class node
    pub fn add_class(&mut self, kind: u16, pos: u32, end: u32, data: ClassData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.len_u32(self.classes.len());
        self.classes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(heritage_clauses.as_ref(), parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add an interface declaration node
    pub fn add_interface(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InterfaceData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let heritage_clauses = data.heritage_clauses.clone();
        let members = data.members.clone();

        let data_index = self.len_u32(self.interfaces.len());
        self.interfaces.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(heritage_clauses.as_ref(), parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add a type alias declaration node
    pub fn add_type_alias(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAliasData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let type_node = data.type_node;

        let data_index = self.len_u32(self.type_aliases.len());
        self.type_aliases.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent(type_node, parent);

        parent
    }

    /// Add an enum declaration node
    pub fn add_enum(&mut self, kind: u16, pos: u32, end: u32, data: EnumData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let members = data.members.clone();

        let data_index = self.len_u32(self.enums.len());
        self.enums.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_list(&members, parent);

        parent
    }

    /// Add an enum member node
    pub fn add_enum_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: EnumMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.enum_members.len());
        self.enum_members.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);

        parent
    }

    /// Add a module declaration node
    pub fn add_module(&mut self, kind: u16, pos: u32, end: u32, data: ModuleData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let body = data.body;

        let data_index = self.len_u32(self.modules.len());
        self.modules.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a module block node: { statements }
    pub fn add_module_block(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ModuleBlockData,
    ) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.len_u32(self.module_blocks.len());
        self.module_blocks.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(statements.as_ref(), parent);

        parent
    }

    /// Add a variable statement/declaration list node
    pub fn add_variable(&mut self, kind: u16, pos: u32, end: u32, data: VariableData) -> NodeIndex {
        self.add_variable_with_flags(kind, pos, end, data, 0)
    }

    /// Add a variable statement/declaration list node with flags
    pub fn add_variable_with_flags(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableData,
        flags: u16,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let declarations = data.declarations.clone();

        let data_index = self.len_u32(self.variables.len());
        self.variables.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes
            .push(Node::with_data_and_flags(kind, pos, end, data_index, flags));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_list(&declarations, parent);

        parent
    }

    /// Add a variable declaration node (individual)
    pub fn add_variable_declaration(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableDeclarationData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.variable_declarations.len());
        self.variable_declarations.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);

        parent
    }
}
