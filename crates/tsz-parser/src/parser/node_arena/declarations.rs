//! `NodeArena` constructors for top-level declarations:
//! functions, classes, interfaces, type aliases, enums, modules, and the
//! variable-statement / individual-variable-declaration pair.

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ClassData, EnumData, EnumMemberData, FunctionData, InterfaceData, ModuleBlockData, ModuleData,
    NodeArenaInner, TypeAliasData, VariableData, VariableDeclarationData,
};

impl NodeArenaInner {
    /// Add a function node
    pub fn add_function(&mut self, kind: u16, pos: u32, end: u32, data: FunctionData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.body, parent);
        push_data_node!(self, parent, kind, pos, end, functions, data)
    }

    /// Add a class node
    pub fn add_class(&mut self, kind: u16, pos: u32, end: u32, data: ClassData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_opt_list(data.heritage_clauses.as_ref(), parent);
        self.set_parent_list(&data.members, parent);
        push_data_node!(self, parent, kind, pos, end, classes, data)
    }

    /// Add an interface declaration node
    pub fn add_interface(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InterfaceData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_opt_list(data.heritage_clauses.as_ref(), parent);
        self.set_parent_list(&data.members, parent);
        push_data_node!(self, parent, kind, pos, end, interfaces, data)
    }

    /// Add a type alias declaration node
    pub fn add_type_alias(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAliasData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, type_aliases, data)
    }

    /// Add an enum declaration node
    pub fn add_enum(&mut self, kind: u16, pos: u32, end: u32, data: EnumData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_list(&data.members, parent);
        push_data_node!(self, parent, kind, pos, end, enums, data)
    }

    /// Add an enum member node
    pub fn add_enum_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: EnumMemberData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, enum_members, data)
    }

    /// Add a module declaration node
    pub fn add_module(&mut self, kind: u16, pos: u32, end: u32, data: ModuleData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.body, parent);
        push_data_node!(self, parent, kind, pos, end, modules, data)
    }

    /// Add a module block node: { statements }
    pub fn add_module_block(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ModuleBlockData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.statements.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, module_blocks, data)
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
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent_list(&data.declarations, parent);
        push_data_node!(self, parent, kind, pos, end, variables, data, flags = flags)
    }

    /// Add a variable declaration node (individual)
    pub fn add_variable_declaration(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: VariableDeclarationData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, variable_declarations, data)
    }
}
