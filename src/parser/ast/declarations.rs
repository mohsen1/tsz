//! Declaration AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use serde::Serialize;

/// A function declaration.
#[derive(Clone, Debug, Serialize)]
pub struct FunctionDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub is_async: bool,       // Async function
    pub asterisk_token: bool, // Generator function
    pub name: NodeIndex,      // Identifier (optional for default exports)
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex, // Return type (optional)
    pub body: NodeIndex,            // Block (optional for overloads)
}

/// A class declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ClassDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex, // Identifier (optional for default exports)
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// An interface declaration.
#[derive(Clone, Debug, Serialize)]
pub struct InterfaceDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub heritage_clauses: Option<NodeList>,
    pub members: NodeList,
}

/// A property signature (in interface or type literal).
#[derive(Clone, Debug, Serialize)]
pub struct PropertySignature {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_annotation: NodeIndex, // Optional
    pub initializer: NodeIndex,     // Optional
}

/// A method signature (in interface or type literal).
#[derive(Clone, Debug, Serialize)]
pub struct MethodSignature {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex, // Optional
}

/// An index signature declaration (e.g., [key: string]: number)
#[derive(Clone, Debug, Serialize)]
pub struct IndexSignatureDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub parameters: NodeList,       // The index parameter(s)
    pub type_annotation: NodeIndex, // The value type
}

/// A call signature in a type literal or interface (e.g., `{ (): void }`)
#[derive(Clone, Debug, Serialize)]
pub struct CallSignature {
    pub base: NodeBase,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex, // Optional return type
}

/// A construct signature in a type literal or interface (e.g., `{ new(): Foo }`)
#[derive(Clone, Debug, Serialize)]
pub struct ConstructSignature {
    pub base: NodeBase,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex, // Optional return type
}

/// A type alias declaration.
#[derive(Clone, Debug, Serialize)]
pub struct TypeAliasDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub type_node: NodeIndex,
}

/// An enum declaration.
#[derive(Clone, Debug, Serialize)]
pub struct EnumDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub members: NodeList,
}

/// An enum member.
#[derive(Clone, Debug, Serialize)]
pub struct EnumMember {
    pub base: NodeBase,
    pub name: NodeIndex,
    pub initializer: NodeIndex, // Optional
}

/// A module/namespace declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ModuleDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex, // Identifier or StringLiteral
    pub body: NodeIndex, // ModuleBlock or ModuleDeclaration
}

/// A module block (the { } body of a module).
#[derive(Clone, Debug, Serialize)]
pub struct ModuleBlock {
    pub base: NodeBase,
    pub statements: NodeList,
}

/// A property declaration in a class.
#[derive(Clone, Debug, Serialize)]
pub struct PropertyDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub question_token: bool,
    pub exclamation_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// A method declaration.
#[derive(Clone, Debug, Serialize)]
pub struct MethodDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub asterisk_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// A constructor declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ConstructorDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub body: NodeIndex,
}

/// A get accessor declaration.
#[derive(Clone, Debug, Serialize)]
pub struct GetAccessorDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// A set accessor declaration.
#[derive(Clone, Debug, Serialize)]
pub struct SetAccessorDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub body: NodeIndex,
}

/// A parameter declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ParameterDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_annotation: NodeIndex,
    pub initializer: NodeIndex,
}

/// A type parameter declaration.
#[derive(Clone, Debug, Serialize)]
pub struct TypeParameterDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>, // in/out variance modifiers
    pub name: NodeIndex,
    pub constraint: NodeIndex, // Optional
    pub default: NodeIndex,    // Optional
}

/// A decorator.
#[derive(Clone, Debug, Serialize)]
pub struct Decorator {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A heritage clause (extends/implements).
#[derive(Clone, Debug, Serialize)]
pub struct HeritageClause {
    pub base: NodeBase,
    pub token: u16, // ExtendsKeyword or ImplementsKeyword
    pub types: NodeList,
}

/// Expression with type arguments (used in heritage clauses).
#[derive(Clone, Debug, Serialize)]
pub struct ExpressionWithTypeArguments {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// An import declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ImportDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub import_clause: NodeIndex,    // Optional
    pub module_specifier: NodeIndex, // StringLiteral
    pub attributes: NodeIndex,       // ImportAttributes (optional)
}

/// Import clause (the part between 'import' and 'from').
#[derive(Clone, Debug, Serialize)]
pub struct ImportClause {
    pub base: NodeBase,
    pub is_type_only: bool,
    pub name: NodeIndex,           // Identifier (optional - default import)
    pub named_bindings: NodeIndex, // NamespaceImport or NamedImports (optional)
}

/// Namespace import (* as name).
#[derive(Clone, Debug, Serialize)]
pub struct NamespaceImport {
    pub base: NodeBase,
    pub name: NodeIndex, // Identifier
}

/// Named imports ({ a, b as c }).
#[derive(Clone, Debug, Serialize)]
pub struct NamedImports {
    pub base: NodeBase,
    pub elements: NodeList, // ImportSpecifier[]
}

/// A single import specifier (a or a as b).
#[derive(Clone, Debug, Serialize)]
pub struct ImportSpecifier {
    pub base: NodeBase,
    pub is_type_only: bool,
    pub property_name: NodeIndex, // Optional (when using 'as')
    pub name: NodeIndex,          // Identifier
}

/// An export declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ExportDeclaration {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub is_type_only: bool,
    pub export_clause: NodeIndex, // NamedExports or NamespaceExport (optional)
    pub module_specifier: NodeIndex, // StringLiteral (optional)
    pub attributes: NodeIndex,    // ImportAttributes (optional)
}

/// Named exports ({ a, b as c }).
#[derive(Clone, Debug, Serialize)]
pub struct NamedExports {
    pub base: NodeBase,
    pub elements: NodeList, // ExportSpecifier[]
}

/// Namespace export (* as name).
#[derive(Clone, Debug, Serialize)]
pub struct NamespaceExport {
    pub base: NodeBase,
    pub name: NodeIndex, // Identifier
}

/// A single export specifier (a or a as b).
#[derive(Clone, Debug, Serialize)]
pub struct ExportSpecifier {
    pub base: NodeBase,
    pub is_type_only: bool,
    pub property_name: NodeIndex, // Optional (when using 'as')
    pub name: NodeIndex,          // Identifier
}

/// An export assignment (export = x or export default x).
#[derive(Clone, Debug, Serialize)]
pub struct ExportAssignment {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub is_export_equals: bool, // true for 'export =', false for 'export default'
    pub expression: NodeIndex,
}

/// Import attributes ({ with: { type: "json" } }).
#[derive(Clone, Debug, Serialize)]
pub struct ImportAttributes {
    pub base: NodeBase,
    pub token: u16,         // WithKeyword or AssertKeyword
    pub elements: NodeList, // ImportAttribute[]
    pub multi_line: bool,
}

/// A single import attribute.
#[derive(Clone, Debug, Serialize)]
pub struct ImportAttribute {
    pub base: NodeBase,
    pub name: NodeIndex,  // Identifier or StringLiteral
    pub value: NodeIndex, // Expression
}

/// An object binding pattern ({ a, b }).
#[derive(Clone, Debug, Serialize)]
pub struct ObjectBindingPattern {
    pub base: NodeBase,
    pub elements: NodeList,
}

/// An array binding pattern ([a, b]).
#[derive(Clone, Debug, Serialize)]
pub struct ArrayBindingPattern {
    pub base: NodeBase,
    pub elements: NodeList,
}

/// A binding element (a or a = default or ...rest).
#[derive(Clone, Debug, Serialize)]
pub struct BindingElement {
    pub base: NodeBase,
    pub dot_dot_dot_token: bool,
    pub property_name: NodeIndex, // Optional
    pub name: NodeIndex,
    pub initializer: NodeIndex, // Optional
}

/// A property assignment (a: value).
#[derive(Clone, Debug, Serialize)]
pub struct PropertyAssignment {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub initializer: NodeIndex,
}

/// A shorthand property assignment (a).
#[derive(Clone, Debug, Serialize)]
pub struct ShorthandPropertyAssignment {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub name: NodeIndex,
    pub equals_token: bool,
    pub object_assignment_initializer: NodeIndex, // Optional
}

/// A spread assignment (...x).
#[derive(Clone, Debug, Serialize)]
pub struct SpreadAssignment {
    pub base: NodeBase,
    pub expression: NodeIndex,
}
