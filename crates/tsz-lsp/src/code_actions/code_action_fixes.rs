use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CodeFixInfo {
    /// The internal name of the code fix (e.g., "spelling", "import", "unusedIdentifier").
    pub fix_name: String,
    /// Human-readable description of the fix.
    pub description: String,
    /// The file changes to apply.
    pub changes: Vec<CodeFixFileChange>,
    /// Optional commands to run after applying the fix.
    pub commands: Vec<serde_json::Value>,
    /// An identifier for fix-all support (e.g., "fixSpelling").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_id: Option<String>,
    /// Human-readable description of the fix-all action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_all_description: Option<String>,
}

/// A file change in a code fix.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeFixFileChange {
    pub file_name: String,
    pub text_changes: Vec<CodeFixTextChange>,
}

/// A text change within a file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeFixTextChange {
    pub start: CodeFixPosition,
    pub end: CodeFixPosition,
    pub new_text: String,
}

/// A position in the tsserver protocol (1-based line/offset).
#[derive(Debug, Clone, Serialize)]
pub struct CodeFixPosition {
    pub line: u32,
    pub offset: u32,
}

/// Mapping from TypeScript diagnostic error codes to code fix metadata.
/// This provides the `fix_name`, `fix_id`, and description templates for common fixes.
pub struct CodeFixRegistry;

impl CodeFixRegistry {
    /// Get code fixes applicable for a given error code.
    /// Returns a list of (`fix_name`, `fix_id`, `description_template`, `fix_all_description`) tuples.
    pub fn fixes_for_error_code(
        error_code: u32,
    ) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        match error_code {
            // === fixSpelling (shared with fixForgottenThisPropertyAccess) ===
            // Cannot find name '{0}'. Did you mean the instance member 'this.{0}'?
            // Cannot find name '{0}'. Did you mean the static member '{1}.{0}'?
            2663 | 2662 => {
                vec![
                    ("spelling", "fixSpelling", "Change spelling", "Fix all detected spelling errors"),
                    ("fixForgottenThisPropertyAccess", "forgottenThisPropertyAccess", "Add 'this.' to unresolved variable", "Add qualifier to all unresolved variables matching a member name"),
                ]
            }

            // === fixSpelling (standalone codes) ===
            // Property '{0}' does not exist on type '{1}'. Did you mean '{2}'?
            2551 |
            // Cannot find name '{0}'. Did you mean '{1}'?
            2552 |
            // Could not find name '{0}'. Did you mean '{1}'?
            2839 |
            // Cannot find namespace '{0}'. Did you mean '{1}'?
            2833 |
            // Property '{0}' may not exist on type '{1}'. Did you mean '{2}'?
            2568 |
            // This member cannot have an 'override' modifier because it is not declared in base class. Did you mean '{1}'?
            4117 |
            // This member cannot have a JSDoc @override tag because it is not declared in base class. Did you mean '{1}'?
            4123 |
            // '{0}' has no exported member named '{1}'. Did you mean '{2}'?
            2724 => {
                vec![("spelling", "fixSpelling", "Change spelling", "Fix all detected spelling errors")]
            }

            // === import (fixMissingImport) + addMissingConst + addMissingMember ===
            // Cannot find name '{0}'.
            2304 => {
                vec![
                    ("import", "fixMissingImport", "Add import", "Add all missing imports"),
                    ("addMissingMember", "fixMissingMember", "Add missing member", "Add all missing members"),
                    ("addMissingConst", "addMissingConst", "Add 'const' to unresolved variable", "Add 'const' to all unresolved variables"),
                    ("fixForgottenThisPropertyAccess", "forgottenThisPropertyAccess", "Add 'this.' to unresolved variable", "Add qualifier to all unresolved variables matching a member name"),
                ]
            }

            // === import (fixMissingImport) ===
            // Cannot find namespace '{0}'.
            2503 |
            // '{0}' only refers to a type, but is being used as a value here.
            2693 |
            // Cannot find name '{0}'. Do you need to change your target library?
            2583 => {
                vec![
                    ("import", "fixMissingImport", "Add import", "Add all missing imports"),
                    ("addMissingMember", "fixMissingMember", "Add missing member", "Add all missing members"),
                ]
            }

            // === fixUnusedIdentifier ===
            // '{0}' is declared but its value is never read.
            6133 |
            // '{0}' is declared but never used.
            6196 |
            // Property '{0}' is declared but its value is never read.
            6138 |
            // All imports in import declaration are unused.
            6192 |
            // All destructured elements are unused.
            6198 |
            // All variables are unused.
            6199 |
            // All type parameters are unused.
            6205 => {
                vec![("unusedIdentifier", "unusedIdentifier_delete", "Remove unused declaration", "Delete all unused declarations")]
            }

            // === fixAddMissingMember (shared with spelling, addMissingAwait) ===
            // Property '{0}' does not exist on type '{1}'.
            2339 => {
                vec![
                    ("addMissingMember", "fixMissingMember", "Add missing member", "Add all missing members"),
                    ("spelling", "fixSpelling", "Change spelling", "Fix all detected spelling errors"),
                    ("addMissingAwait", "addMissingAwait", "Add missing 'await'", "Add all missing 'await'"),
                ]
            }
            // Property '{0}' is missing in type '{1}' but required in type '{2}'.
            2741 |
            // Type '{0}' is missing the following properties from type '{1}': {2}
            2739 |
            // Type '{0}' is missing the following properties from type '{1}': {2}, and {3} more.
            2740 |
            // Type '{0}' does not satisfy the expected type '{1}'.
            1360 => {
                vec![
                    ("addMissingProperties", "fixMissingProperties", "Add missing properties", "Add all missing properties"),
                    ("addMissingMember", "fixMissingMember", "Add missing member", "Add all missing members"),
                    ("spelling", "fixSpelling", "Change spelling", "Fix all detected spelling errors"),
                ]
            }
            // Argument of type '{0}' is not assignable to parameter of type '{1}'.
            2345 => {
                vec![
                    ("addMissingMember", "fixMissingMember", "Add missing member", "Add all missing members"),
                    ("addMissingAwait", "addMissingAwait", "Add missing 'await'", "Add all missing 'await'"),
                    ("returnValueCorrect", "fixReturnValueCorrect", "Fix return value", "Fix all return values"),
                ]
            }

            // === fixAwaitInSyncFunction ===
            // 'await' expressions are only allowed within async functions...
            1308 |
            // Identifier expected. 'await' is a reserved word...
            1359 |
            // 'for await' loops are only allowed within async functions...
            1432 |
            // Cannot find name '{0}'. Did you mean to write this in an async function?
            2773 => {
                vec![("fixAwaitInSyncFunction", "fixAwaitInSyncFunction", "Add async modifier to containing function", "Add all missing async modifiers")]
            }

            // === fixOverrideModifier ===
            // This member cannot have an 'override' modifier because it is not declared in the base class
            4113 |
            // This member must have an 'override' modifier because it overrides a member in base class
            4114 |
            // This member cannot have an 'override' modifier because its containing class does not extend another class
            4112 |
            // This parameter property must have an 'override' modifier because it overrides a member in base class
            4115 |
            // This member must have an 'override' modifier because it overrides an abstract method
            4116 |
            // This member must have a JSDoc @override tag because it overrides a member in base class
            4119 |
            // This parameter property must have a JSDoc @override tag
            4120 |
            // This member cannot have a JSDoc @override tag because its containing class does not extend another class
            4121 |
            // This member cannot have a JSDoc @override tag because it is not declared in the base class
            4122 |
            // This member cannot have a JSDoc @override tag because its name is dynamic
            4128 => {
                vec![("fixOverrideModifier", "fixAddOverrideModifier", "Add 'override' modifier", "Add all missing 'override' modifiers")]
            }

            // === fixClassIncorrectlyImplementsInterface ===
            // Class '{0}' incorrectly implements interface '{1}'.
            2420 => {
                vec![("fixClassIncorrectlyImplementsInterface", "fixClassIncorrectlyImplementsInterface", "Implement interface", "Implement all unimplemented interfaces")]
            }

            // === fixClassDoesntImplementInheritedAbstractMember ===
            // Non-abstract class '{0}' does not implement inherited abstract member...
            2515 |
            // Non-abstract class is missing implementations for the following members
            2654 |
            // Non-abstract class expression does not implement inherited abstract member
            18052 |
            // Non-abstract class expression is missing implementations
            18053 => {
                vec![("fixClassDoesntImplementInheritedAbstractMember", "fixClassDoesntImplementInheritedAbstractMember", "Implement inherited abstract class", "Implement all inherited abstract classes")]
            }

            // === addMissingAsync ===
            // The return type of an async function must be Promise
            2705 => {
                vec![("addMissingAsync", "addMissingAsync", "Add async modifier", "Add all missing async modifiers")]
            }
            // Type 'X' is not assignable to type 'Y' (shared with addMissingAsync, returnValueCorrect)
            2322 => {
                vec![
                    ("addMissingAsync", "addMissingAsync", "Add async modifier", "Add all missing async modifiers"),
                    ("returnValueCorrect", "fixReturnValueCorrect", "Fix return value", "Fix all return values"),
                ]
            }

            // === fixReturnTypeInAsyncFunction ===
            // The return type of an async function or method must be the global Promise<T> type
            2697 |
            // The return type of an async function or method must be the global Promise<T> type. Did you mean to write 'Promise<{0}>'?
            1064 => {
                vec![("fixReturnTypeInAsyncFunction", "fixReturnTypeInAsyncFunction", "Fix return type", "Fix all incorrect return types")]
            }

            // === fixMissingCallParentheses ===
            // This condition will always return true since this function is always defined
            2774 => {
                vec![("fixMissingCallParentheses", "fixMissingCallParentheses", "Add missing call parentheses", "Add all missing call parentheses")]
            }

            // === fixConvertToMappedObjectType ===
            // An index signature parameter type cannot be a literal type or generic type.
            1337 => {
                vec![("fixConvertToMappedObjectType", "fixConvertToMappedObjectType", "Convert to mapped object type", "Convert all to mapped object types")]
            }

            // === fixStrictClassInitialization ===
            // Property '{0}' has no initializer and is not definitely assigned in the constructor.
            2564 => {
                vec![
                    ("addMissingPropertyDefiniteAssignmentAssertions", "addMissingPropertyDefiniteAssignmentAssertions", "Add definite assignment assertion", "Add all missing definite assignment assertions"),
                    ("addMissingPropertyUndefinedType", "addMissingPropertyUndefinedType", "Add undefined type", "Add undefined type to all missing properties"),
                    ("addMissingPropertyInitializer", "addMissingPropertyInitializer", "Add initializer", "Add initializers to all uninitialized properties"),
                ]
            }

            // === fixEnableExperimentalDecorators ===
            1219 => {
                vec![("fixEnableExperimentalDecorators", "fixEnableExperimentalDecorators", "Enable experimental decorators", "Enable experimental decorators")]
            }

            // === fixExpectedComma ===
            // ';' expected. (but actually a comma should be used)
            1005 => {
                vec![("fixExpectedComma", "fixExpectedComma", "Replace with comma", "Replace all expected commas")]
            }

            // === fixAddMissingConstraint ===
            // Type parameter '{0}' has a circular constraint
            2313 |
            // Type '{0}' is not assignable to type '{1}' with constraint '{2}'.
            2344 => {
                vec![("addMissingConstraint", "addMissingConstraint", "Add missing constraint", "Add all missing constraints")]
            }

            // === fixUnreachableCode ===
            // Unreachable code detected.
            7027 => {
                vec![("fixUnreachableCode", "fixUnreachableCode", "Remove unreachable code", "Remove all unreachable code")]
            }

            // === fixAddMissingNewOperator ===
            // Value of type '{0}' is not callable. Did you mean to include 'new'?
            2348 => {
                vec![("fixAddMissingNewOperator", "fixAddMissingNewOperator", "Add missing 'new' operator", "Add all missing 'new' operators")]
            }

            // === fixCannotFindModule ===
            // Cannot find module '{0}' or its corresponding type declarations.
            2307 => {
                vec![("fixCannotFindModule", "fixCannotFindModule", "Install missing types", "Install all missing type packages")]
            }

            // === fixNaNEquality ===
            // This condition will always return '{0}'.
            2845 => {
                vec![("fixNaNEquality", "fixNaNEquality", "Use Number.isNaN()", "Use Number.isNaN() in all comparisons")]
            }

            // === fixConstructorForDerivedNeedSuperCall ===
            // Constructors for derived classes must contain a 'super' call.
            2377 => {
                vec![("fixConstructorForDerivedNeedSuperCall", "fixConstructorForDerivedNeedSuperCall", "Add missing super call", "Add all missing super calls")]
            }

            // === fixClassSuperMustPrecedeThisAccess ===
            // 'super' must be called before accessing 'this' in the constructor of a derived class.
            17009 |
            // 'super' must be called before accessing a property of 'super' in the constructor of a derived class.
            17011 => {
                vec![("fixClassSuperMustPrecedeThisAccess", "fixClassSuperMustPrecedeThisAccess", "Move super call before this access", "Move all super calls before this access")]
            }

            // === addConvertToUnknownForNonOverlappingTypes ===
            // Conversion of type '{0}' to type '{1}' may be a mistake...
            2352 => {
                vec![("addConvertToUnknownForNonOverlappingTypes", "addConvertToUnknownForNonOverlappingTypes", "Add 'unknown' conversion for non-overlapping types", "Add 'unknown' to all conversions of non-overlapping types")]
            }

            // === fixForgottenThisPropertyAccess (standalone code) ===
            // Private identifiers are only allowed in class bodies
            1451 => {
                vec![("fixForgottenThisPropertyAccess", "forgottenThisPropertyAccess", "Add 'this.' to unresolved variable", "Add qualifier to all unresolved variables matching a member name")]
            }

            // === fixInvalidJsxCharacters ===
            // Unexpected token. Did you mean `{'}'}` or `&rbrace;`?
            1381 |
            // Unexpected token. Did you mean `{'>'}` or `&gt;`?
            1382 => {
                vec![
                    ("fixInvalidJsxCharacters_expression", "fixInvalidJsxCharacters_expression", "Wrap invalid character in an expression container", "Wrap all invalid characters in an expression container"),
                    ("fixInvalidJsxCharacters_htmlEntity", "fixInvalidJsxCharacters_htmlEntity", "Convert invalid character to its html entity code", "Convert all invalid characters to HTML entity code"),
                ]
            }

            // === fixUnusedLabel ===
            // Unused label.
            7028 => {
                vec![("fixUnusedLabel", "fixUnusedLabel", "Remove unused label", "Remove all unused labels")]
            }

            // === addMissingConst (standalone code) ===
            // No value exists in scope for the shorthand property '{0}'.
            18004 => {
                vec![("addMissingConst", "addMissingConst", "Add 'const' to unresolved variable", "Add 'const' to all unresolved variables")]
            }

            // === addMissingDeclareProperty ===
            // Property '{0}' will overwrite the base property in '{1}'.
            2612 => {
                vec![("addMissingDeclareProperty", "addMissingDeclareProperty", "Prefix with 'declare'", "Prefix all incorrect property declarations with 'declare'")]
            }

            // === addMissingTypeof (fixAddModuleReferTypeMissingTypeof) ===
            // Module '{0}' does not refer to a type, but is used as a type here. Did you mean 'typeof import('{0}')'?
            1340 => {
                vec![("fixAddModuleReferTypeMissingTypeof", "fixAddModuleReferTypeMissingTypeof", "Add missing 'typeof'", "Add missing 'typeof' everywhere")]
            }

            // === annotateWithTypeFromJSDoc ===
            // JSDoc types may be moved to TypeScript types.
            80004 => {
                vec![("annotateWithTypeFromJSDoc", "annotateWithTypeFromJSDoc", "Annotate with type from JSDoc", "Annotate all with types from JSDoc")]
            }

            // === convertToAsyncFunction ===
            // This may be converted to an async function.
            80006 => {
                vec![("convertToAsyncFunction", "convertToAsyncFunction", "Convert to async function", "Convert all to async functions")]
            }

            // === requireInTs ===
            // 'require' call may be converted to an import.
            80005 => {
                vec![("requireInTs", "requireInTs", "Convert require to import", "Convert all require to import")]
            }

            // === fixAddVoidToPromise ===
            // Expected 1 argument, but got 0. 'new Promise()' needs a JSDoc hint...
            2810 |
            // Expected {0} arguments, but got {1}. Did you forget to include 'void' in your type argument to 'Promise'?
            2794 => {
                vec![("fixAddVoidToPromise", "fixAddVoidToPromise", "Add void to Promise resolved without a value", "Add void to all Promises resolved without a value")]
            }

            // === addMissingAwait ===
            // An arithmetic operand must be of type 'any', 'number', 'bigint' or an enum type.
            2356 |
            // The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
            2362 |
            // The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.
            2363 |
            // Operator '{0}' cannot be applied to types '{1}' and '{2}'.
            2365 |
            // This comparison appears to be unintentional...
            2367 |
            // This condition will always return true since this '{0}' is always defined.
            2801 |
            // Type '{0}' is not an array type.
            2461 |
            // Type '{0}' is not an array type or a string type.
            2495 |
            // Type '{0}' can only be iterated through when using the '--downlevelIteration' flag...
            2802 |
            // Type '{0}' is not an array type or a string type or does not have a '[Symbol.iterator]()' method...
            2549 |
            // Type '{0}' is not an array type or does not have a '[Symbol.iterator]()' method...
            2548 |
            // Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.
            2488 |
            // Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.
            2504 |
            // Operator '{0}' cannot be applied to type '{1}'.
            2736 |
            // This expression is not callable.
            2349 |
            // This expression is not constructable.
            2351 => {
                vec![("addMissingAwait", "addMissingAwait", "Add missing 'await'", "Add all missing 'await'")]
            }

            // === fixExtendsInterfaceBecomesImplements ===
            // Cannot extend an interface '{0}'. Did you mean 'implements'?
            2689 => {
                vec![("fixExtendsInterfaceBecomesImplements", "fixExtendsInterfaceBecomesImplements", "Change 'extends' to 'implements'", "Change all 'extends' to 'implements'")]
            }

            // === fixEnableJsxFlag ===
            // Cannot use JSX unless the '--jsx' flag is provided.
            17004 => {
                vec![("fixEnableJsxFlag", "fixEnableJsxFlag", "Enable the '--jsx' flag", "Enable the '--jsx' flag")]
            }

            // === fixImplicitThis + inferFromUsage ===
            // 'this' implicitly has type 'any' because it does not have a type annotation.
            2683 => {
                vec![
                    ("fixImplicitThis", "fixImplicitThis", "Add 'this' parameter", "Add 'this' parameter to all functions"),
                    ("inferFromUsage", "inferFromUsage", "Infer type from usage", "Infer all types from usage"),
                ]
            }

            // === inferFromUsage ===
            // Variable '{0}' implicitly has type '{1}' in some locations where its type cannot be determined.
            7034 |
            // Variable '{0}' implicitly has an '{1}' type.
            7005 |
            // Parameter '{0}' implicitly has an '{1}' type.
            7006 |
            // Rest parameter '{0}' implicitly has an 'any[]' type.
            7019 |
            // Property '{0}' implicitly has type 'any', because its get accessor lacks a return type annotation.
            7033 |
            // '{0}', which lacks return-type annotation, implicitly has an '{1}' return type.
            7010 |
            // Property '{0}' implicitly has type 'any', because its set accessor lacks a parameter type annotation.
            7032 |
            // Member '{0}' implicitly has an '{1}' type.
            7008 |
            // Variable '{0}' implicitly has type '{1}' in some locations, but a better type may be inferred from usage.
            7046 |
            // Variable '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.
            7043 |
            // Parameter '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.
            7044 |
            // Rest parameter '{0}' implicitly has an 'any[]' type, but a better type may be inferred from usage.
            7047 |
            // Property '{0}' implicitly has type 'any', but a better type for its get accessor may be inferred from usage.
            7048 |
            // '{0}' implicitly has an '{1}' return type, but a better type may be inferred from usage.
            7050 |
            // Property '{0}' implicitly has type 'any', but a better type for its set accessor may be inferred from usage.
            7049 |
            // Member '{0}' implicitly has an '{1}' type, but a better type may be inferred from usage.
            7045 => {
                vec![("inferFromUsage", "inferFromUsage", "Infer type from usage", "Infer all types from usage")]
            }

            // === inferFromUsage + returnValueCorrect ===
            // Function lacks ending return statement and return type does not include 'undefined'.
            2366 => {
                vec![
                    ("inferFromUsage", "inferFromUsage", "Infer type from usage", "Infer all types from usage"),
                    ("returnValueCorrect", "fixReturnValueCorrect", "Fix return value", "Fix all return values"),
                ]
            }

            // === returnValueCorrect ===
            // A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.
            2355 => {
                vec![
                    ("addReturnStatement", "fixAddReturnStatement", "Add a return statement", "Add all missing return statements"),
                    ("removeBlockBodyBrace", "fixRemoveBlockBodyBrace", "Remove braces from arrow function body", "Remove braces from all arrow function bodies"),
                    ("wrapBodyWithParentheses", "fixWrapBodyWithParentheses", "Wrap the following body with parentheses which should be an object literal", "Wrap all object literal bodies with parentheses"),
                ]
            }

            // === fixPropertyAssignment ===
            // Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern.
            1312 => {
                vec![("fixPropertyAssignment", "fixPropertyAssignment", "Use ':' instead of '='", "Fix all property assignments")]
            }

            // === fixPropertyOverrideAccessor ===
            // '{0}' is defined as an accessor in class '{1}', but is overridden here in '{2}' as an instance property.
            2610 |
            // '{0}' is defined as a property in class '{1}', but is overridden here in '{2}' as an accessor.
            2611 => {
                vec![("fixPropertyOverrideAccessor", "fixPropertyOverrideAccessor", "Generate get and set accessors", "Generate get and set accessors for all overriding properties")]
            }

            // === fixNoPropertyAccessFromIndexSignature ===
            // Property '{0}' comes from an index signature, so it must be accessed with ['{0}'].
            4111 => {
                vec![("fixNoPropertyAccessFromIndexSignature", "fixNoPropertyAccessFromIndexSignature", "Use element access for index signature property", "Use element access for all index signature properties")]
            }

            // === fixIncorrectNamedTupleSyntax ===
            // A labeled tuple element is declared as optional with a question mark after the name and before the colon, rather than after the type.
            5086 |
            // A labeled tuple element is declared as rest with a '...' before the name, rather than before the type.
            5087 => {
                vec![("fixIncorrectNamedTupleSyntax", "fixIncorrectNamedTupleSyntax", "Fix incorrect named tuple syntax", "Fix all incorrect named tuple syntax")]
            }

            // === fixJSDocTypes ===
            // JSDoc types can only be used inside documentation comments.
            8020 |
            // '{0}' at the end of a type is not valid TypeScript syntax. Did you mean to write '{1}'?
            17019 |
            // '{0}' at the start of a type is not valid TypeScript syntax. Did you mean to write '{1}'?
            17020 => {
                vec![("fixJSDocTypes", "fixJSDocTypes", "Fix JSDoc type", "Fix all JSDoc types")]
            }

            // === addMissingInvocationForDecorator ===
            // '{0}' accepts too few arguments to be used as a decorator here. Did you mean to call it first and write '@{0}()'?
            1329 => {
                vec![("addMissingInvocationForDecorator", "addMissingInvocationForDecorator", "Add missing invocation for decorator", "Add all missing invocations for decorators")]
            }

            // === fixAddMissingParam ===
            // Expected {0} arguments, but got {1}.
            2554 => {
                vec![("fixAddMissingParam", "fixAddMissingParam", "Add missing parameter", "Add all missing parameters")]
            }

            // === removeUnnecessaryAwait ===
            // 'await' has no effect on the type of this expression.
            80007 => {
                vec![("removeUnnecessaryAwait", "removeUnnecessaryAwait", "Remove unnecessary 'await'", "Remove all unnecessary 'await'")]
            }

            // === removeAccidentalCallParentheses ===
            // This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?
            6234 => {
                vec![("removeAccidentalCallParentheses", "removeAccidentalCallParentheses", "Remove accidental call parentheses", "Remove all accidental call parentheses")]
            }

            // === useBigintLiteral ===
            // Numeric literals with absolute values equal to 2^53 or greater are too large to be represented accurately as integers.
            80008 => {
                vec![("useBigintLiteral", "useBigintLiteral", "Convert to a bigint numeric literal", "Convert all to bigint numeric literals")]
            }

            // === wrapJsxInFragment ===
            // JSX expressions must have one parent element.
            2657 => {
                vec![("wrapJsxInFragment", "wrapJsxInFragment", "Wrap in JSX fragment", "Wrap all JSX in fragments")]
            }

            // === convertConstToLet ===
            // Cannot assign to '{0}' because it is a constant.
            2588 => {
                vec![("convertConstToLet", "convertConstToLet", "Convert 'const' to 'let'", "Convert all 'const' to 'let'")]
            }

            // === useDefaultImport ===
            // Import may be converted to a default import.
            80003 => {
                vec![("useDefaultImport", "useDefaultImport", "Convert to default import", "Convert all to default imports")]
            }

            // === splitTypeOnlyImport ===
            // A type-only import can specify a default import or named bindings, but not both.
            1363 => {
                vec![("splitTypeOnlyImport", "splitTypeOnlyImport", "Split type-only import", "Split all type-only imports")]
            }

            // === convertToTypeOnlyImport ===
            // '{0}' is a type and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.
            1484 |
            // '{0}' resolves to a type-only declaration and must be imported using a type-only import when 'verbatimModuleSyntax' is enabled.
            1485 => {
                vec![("convertToTypeOnlyImport", "convertToTypeOnlyImport", "Convert to type-only import", "Convert all to type-only imports")]
            }

            // === convertToTypeOnlyExport ===
            // Re-exporting a type when '{0}' is enabled requires using 'export type'.
            1205 => {
                vec![("convertToTypeOnlyExport", "convertToTypeOnlyExport", "Convert to type-only export", "Convert all to type-only exports")]
            }

            // === addOptionalPropertyUndefined ===
            // Type '{0}' is not assignable to type '{1}' with exactOptionalPropertyTypes. Consider adding 'undefined' to the type of the target.
            2412 |
            // Type '{0}' is not assignable to type '{1}' with exactOptionalPropertyTypes. Consider adding 'undefined' to the types of the target's properties.
            2375 |
            // Argument of type '{0}' is not assignable to parameter of type '{1}' with exactOptionalPropertyTypes...
            2379 => {
                vec![("addOptionalPropertyUndefined", "addOptionalPropertyUndefined", "Add 'undefined' to optional property type", "Add 'undefined' to all optional property types")]
            }

            // === fixInvalidImportSyntax ===
            // '{0}' can only be imported by using a default import.
            1259 => {
                vec![("fixInvalidImportSyntax", "fixInvalidImportSyntax", "Fix invalid import syntax", "Fix all invalid import syntax")]
            }

            _ => vec![],
        }
    }

    /// Get all error codes that have registered code fixes.
    pub fn supported_error_codes() -> Vec<u32> {
        vec![
            2663, 2662, // fixSpelling + fixForgottenThisPropertyAccess
            2551, 2552, 2839, 2833, 2568, 4117, 4123, 2724, // fixSpelling
            2304, // import + addMissingConst + addMissingMember + forgottenThisPropertyAccess
            2503, 2693, 2583, // import
            6133, 6196, 6138, 6192, 6198, 6199, 6205, // fixUnusedIdentifier
            2339, // fixAddMissingMember + spelling + addMissingAwait
            2741, 2739, 2740, 1360, // fixAddMissingMember + spelling
            2345, // fixAddMissingMember + addMissingAwait + returnValueCorrect
            1308, 1359, 1432, 2773, // fixAwaitInSyncFunction
            4113, 4114, 4112, 4115, 4116, 4119, 4120, 4121, 4122, 4128, // fixOverrideModifier
            2420, // fixClassIncorrectlyImplementsInterface
            2515, 2654, 18052, 18053, // fixClassDoesntImplementInheritedAbstractMember
            2705,  // addMissingAsync
            2322,  // addMissingAsync + returnValueCorrect
            2697, 1064, // fixReturnTypeInAsyncFunction
            2774, // fixMissingCallParentheses
            1337, // fixConvertToMappedObjectType
            2564, // fixStrictClassInitialization
            1219, // fixEnableExperimentalDecorators
            1005, // fixExpectedComma
            2313, 2344, // fixAddMissingConstraint
            7027, // fixUnreachableCode
            2348, // fixAddMissingNewOperator
            2307, // fixCannotFindModule
            2845, // fixNaNEquality
            2377, // fixConstructorForDerivedNeedSuperCall
            17009, 17011, // fixClassSuperMustPrecedeThisAccess
            2352,  // addConvertToUnknownForNonOverlappingTypes
            1451,  // fixForgottenThisPropertyAccess
            1381, 1382,  // fixInvalidJsxCharacters
            7028,  // fixUnusedLabel
            18004, // addMissingConst
            2612,  // addMissingDeclareProperty
            1340,  // addMissingTypeof
            80004, // annotateWithTypeFromJSDoc
            80006, // convertToAsyncFunction
            80005, // requireInTs
            2810, 2794, // fixAddVoidToPromise
            2356, 2362, 2363, 2365, 2367, 2801, 2461, 2495, 2802, 2549, 2548, 2488, 2504, 2736,
            2349, 2351,  // addMissingAwait
            2689,  // fixExtendsInterfaceBecomesImplements
            17004, // fixEnableJsxFlag
            2683,  // fixImplicitThis + inferFromUsage
            7034, 7005, 7006, 7019, 7033, 7010, 7032, 7008, 7046, 7043, 7044, 7047, 7048, 7050,
            7049, 7045, // inferFromUsage
            2366, // inferFromUsage + returnValueCorrect
            2355, // returnValueCorrect
            1312, // fixPropertyAssignment
            2610, 2611, // fixPropertyOverrideAccessor
            4111, // fixNoPropertyAccessFromIndexSignature
            5086, 5087, // fixIncorrectNamedTupleSyntax
            8020, 17019, 17020, // fixJSDocTypes
            1329,  // addMissingInvocationForDecorator
            2554,  // fixAddMissingParam
            80007, // removeUnnecessaryAwait
            6234,  // removeAccidentalCallParentheses
            80008, // useBigintLiteral
            2657,  // wrapJsxInFragment
            2588,  // convertConstToLet
            80003, // useDefaultImport
            1363,  // splitTypeOnlyImport
            1484, 1485, // convertToTypeOnlyImport
            1205, // convertToTypeOnlyExport
            2412, 2375, 2379, // addOptionalPropertyUndefined
            1259, // fixInvalidImportSyntax
        ]
    }
}
