//! `NodeArena` constructors for top-level declarations:
//! functions, classes, interfaces, type aliases, enums, modules, and the
//! variable-statement / individual-variable-declaration pair.

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ClassData, EnumData, EnumMemberData, ExtendedNodeInfo, FunctionData, InterfaceData,
    ModuleBlockData, ModuleData, Node, NodeArenaInner, TypeAliasData, VariableData,
    VariableDeclarationData,
};

impl NodeArenaInner {
    /// Add a function node
    pub fn add_function(&mut self, kind: u16, pos: u32, end: u32, data: FunctionData) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let body = data.body;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        let data_index = self.len_u32(self.functions.len());
        self.functions.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        parent
    }

    /// Add a class node
    pub fn add_class(&mut self, kind: u16, pos: u32, end: u32, data: ClassData) -> NodeIndex {
        let name = data.name;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_opt_list(data.heritage_clauses.as_ref(), parent);
        self.set_parent_list(&data.members, parent);

        let data_index = self.len_u32(self.classes.len());
        self.classes.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

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
        let name = data.name;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_opt_list(data.heritage_clauses.as_ref(), parent);
        self.set_parent_list(&data.members, parent);

        let data_index = self.len_u32(self.interfaces.len());
        self.interfaces.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

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
        let name = data.name;
        let type_node = data.type_node;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent(type_node, parent);

        let data_index = self.len_u32(self.type_aliases.len());
        self.type_aliases.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        parent
    }

    /// Add an enum declaration node
    pub fn add_enum(&mut self, kind: u16, pos: u32, end: u32, data: EnumData) -> NodeIndex {
        let name = data.name;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_list(&data.members, parent);

        let data_index = self.len_u32(self.enums.len());
        self.enums.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

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
        let name = data.name;
        let body = data.body;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(body, parent);

        let data_index = self.len_u32(self.modules.len());
        self.modules.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

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
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.statements.as_ref(), parent);

        let data_index = self.len_u32(self.module_blocks.len());
        self.module_blocks.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

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
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent_list(&data.declarations, parent);

        let data_index = self.len_u32(self.variables.len());
        self.variables.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes
            .push(Node::with_data_and_flags(kind, pos, end, data_index, flags));
        self.extended_info.push(ExtendedNodeInfo::default());

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
