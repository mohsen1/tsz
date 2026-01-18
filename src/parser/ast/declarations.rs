//! Abstract Syntax Tree node definitions for declarations.
//! 
//! This module defines the structure of various declarations such as functions,
//! classes, variables, and imports. It utilizes `ThinNode` wrappers to handle
//! recursion and heap allocation without the overhead of the legacy `FatNode`.

use crate::parser::ast::node::ThinNode;
use crate::parser::ast::types::TypeAnnotation;
use crate::parser::ast::expressions::Expression;
use crate::parser::ast::statements::Block;
use std::collections::HashMap;

/// Represents a specific type of declaration in the program.
#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    Variable(ThinNode<VariableDeclaration>),
    Function(ThinNode<FunctionDeclaration>),
    Class(ThinNode<ClassDeclaration>),
    Import(ThinNode<ImportDeclaration>),
    // Note: Empty variants or simplified placeholders are often used during migration
}

/// Represents the declaration of a variable (e.g., `let x: int = 5;`).
#[derive(Debug, Clone, PartialEq)]
pub struct VariableDeclaration {
    /// The identifier being declared.
    /// Replaced `FatNode<Identifier>` with `ThinNode<Identifier>`.
    pub identifier: ThinNode<Identifier>,
    
    /// An optional explicit type annotation.
    pub type_annotation: Option<ThinNode<TypeAnnotation>>,
    
    /// The initializing expression, if present.
    pub value: Option<ThinNode<Expression>>,
    
    /// Whether the variable is mutable (let vs const).
    pub is_mutable: bool,
}

/// Represents the declaration of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDeclaration {
    /// The name of the function.
    /// Replaced `FatNode<Identifier>` with `ThinNode<Identifier>`.
    pub identifier: ThinNode<Identifier>,
    
    /// Generic type parameters (e.g., `<T>`).
    pub generics: Vec<ThinNode<TypeAnnotation>>,
    
    /// The function parameters.
    /// Replaced `Vec<FatNode<Parameter>>` with `Vec<ThinNode<Parameter>>`.
    pub parameters: Vec<ThinNode<Parameter>>,
    
    /// The return type of the function.
    pub return_type: Option<ThinNode<TypeAnnotation>>,
    
    /// The body of the function.
    /// Replaced `FatNode<Block>` with `ThinNode<Block>`.
    pub body: ThinNode<Block>,
    
    /// Whether this is an async function.
    pub is_async: bool,
}

/// Represents a single parameter in a function signature.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// The parameter name.
    pub identifier: ThinNode<Identifier>,
    
    /// The type of the parameter.
    pub type_annotation: Option<ThinNode<TypeAnnotation>>,
    
    /// A default value for the parameter (optional).
    pub default_value: Option<ThinNode<Expression>>,
}

/// Represents the declaration of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDeclaration {
    /// The class name.
    /// Replaced `FatNode<Identifier>` with `ThinNode<Identifier>`.
    pub identifier: ThinNode<Identifier>,
    
    /// The class this class inherits from.
    pub superclass: Option<ThinNode<Identifier>>,
    
    /// Generic constraints for the class.
    pub generics: Vec<ThinNode<TypeAnnotation>>,
    
    /// Properties defined on the class.
    /// Replaced `Vec<FatNode<PropertyDeclaration>>` with `Vec<ThinNode<PropertyDeclaration>>`.
    pub properties: Vec<ThinNode<PropertyDeclaration>>,
    
    /// Methods defined on the class.
    /// Replaced `Vec<FatNode<MethodDeclaration>>` with `Vec<ThinNode<MethodDeclaration>>`.
    pub methods: Vec<ThinNode<MethodDeclaration>>,
    
    /// The constructor definition.
    pub constructor: Option<ThinNode<ConstructorDeclaration>>,
}

/// Represents a property declaration within a class.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDeclaration {
    /// The property name.
    pub identifier: ThinNode<Identifier>,
    
    /// The type of the property.
    pub type_annotation: Option<ThinNode<TypeAnnotation>>,
    
    /// The initial value, if any.
    pub value: Option<ThinNode<Expression>>,
    
    /// Visibility modifier.
    pub is_public: bool,
}

/// Represents a method declaration within a class.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodDeclaration {
    /// The method name.
    pub identifier: ThinNode<Identifier>,
    
    /// Parameters for the method.
    pub parameters: Vec<ThinNode<Parameter>>,
    
    /// Return type.
    pub return_type: Option<ThinNode<TypeAnnotation>>,
    
    /// The method body.
    pub body: ThinNode<Block>,
    
    /// Whether the method is static.
    pub is_static: bool,
}

/// Represents the constructor of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstructorDeclaration {
    /// Constructor parameters.
    pub parameters: Vec<ThinNode<Parameter>>,
    
    /// The constructor body.
    pub body: ThinNode<Block>,
}

/// Represents an import declaration (e.g., `import { foo } from 'bar';`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDeclaration {
    /// The module path/specifier.
    pub module_specifier: ThinNode<Expression>, // Often StringLiteral
    
    /// The specific imports.
    pub specifiers: Vec<ThinNode<ImportSpecifier>>,
}

/// Represents a specific specifier in an import statement.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSpecifier {
    /// The local name being imported.
    pub local: ThinNode<Identifier>,
    
    /// The name in the module (if aliased).
    pub imported: Option<ThinNode<Identifier>>,
}

/// A generic Identifier node.
/// Historically might have been wrapped in a FatNode, now used directly with ThinNode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub name: String,
    // Ideally, a Symbol ID would be here for semantic analysis, but for the AST, the string suffices.
}
```
