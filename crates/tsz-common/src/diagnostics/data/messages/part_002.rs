//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
    DiagnosticMessage {
        code: 2376,
        category: DiagnosticCategory::Error,
        message: "A 'super' call must be the first statement in the constructor to refer to 'super' or 'this' when a derived class contains initialized properties, parameter properties, or private identifiers.",
    },
    DiagnosticMessage {
        code: 2377,
        category: DiagnosticCategory::Error,
        message: "Constructors for derived classes must contain a 'super' call.",
    },
    DiagnosticMessage {
        code: 2378,
        category: DiagnosticCategory::Error,
        message: "A 'get' accessor must return a value.",
    },
    DiagnosticMessage {
        code: 2379,
        category: DiagnosticCategory::Error,
        message: "Argument of type '{0}' is not assignable to parameter of type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties.",
    },
    DiagnosticMessage {
        code: 2383,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be exported or non-exported.",
    },
    DiagnosticMessage {
        code: 2384,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be ambient or non-ambient.",
    },
    DiagnosticMessage {
        code: 2385,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be public, private or protected.",
    },
    DiagnosticMessage {
        code: 2386,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be optional or required.",
    },
    DiagnosticMessage {
        code: 2387,
        category: DiagnosticCategory::Error,
        message: "Function overload must be static.",
    },
    DiagnosticMessage {
        code: 2388,
        category: DiagnosticCategory::Error,
        message: "Function overload must not be static.",
    },
    DiagnosticMessage {
        code: 2389,
        category: DiagnosticCategory::Error,
        message: "Function implementation name must be '{0}'.",
    },
    DiagnosticMessage {
        code: 2390,
        category: DiagnosticCategory::Error,
        message: "Constructor implementation is missing.",
    },
    DiagnosticMessage {
        code: 2391,
        category: DiagnosticCategory::Error,
        message: "Function implementation is missing or not immediately following the declaration.",
    },
    DiagnosticMessage {
        code: 2392,
        category: DiagnosticCategory::Error,
        message: "Multiple constructor implementations are not allowed.",
    },
    DiagnosticMessage {
        code: 2393,
        category: DiagnosticCategory::Error,
        message: "Duplicate function implementation.",
    },
    DiagnosticMessage {
        code: 2394,
        category: DiagnosticCategory::Error,
        message: "This overload signature is not compatible with its implementation signature.",
    },
    DiagnosticMessage {
        code: 2395,
        category: DiagnosticCategory::Error,
        message: "Individual declarations in merged declaration '{0}' must be all exported or all local.",
    },
    DiagnosticMessage {
        code: 2396,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier 'arguments'. Compiler uses 'arguments' to initialize rest parameters.",
    },
    DiagnosticMessage {
        code: 2397,
        category: DiagnosticCategory::Error,
        message: "Declaration name conflicts with built-in global identifier '{0}'.",
    },
    DiagnosticMessage {
        code: 2398,
        category: DiagnosticCategory::Error,
        message: "'constructor' cannot be used as a parameter property name.",
    },
    DiagnosticMessage {
        code: 2399,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '_this'. Compiler uses variable declaration '_this' to capture 'this' reference.",
    },
    DiagnosticMessage {
        code: 2400,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to variable declaration '_this' that compiler uses to capture 'this' reference.",
    },
    DiagnosticMessage {
        code: 2401,
        category: DiagnosticCategory::Error,
        message: "A 'super' call must be a root-level statement within a constructor of a derived class that contains initialized properties, parameter properties, or private identifiers.",
    },
    DiagnosticMessage {
        code: 2402,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to '_super' that compiler uses to capture base class reference.",
    },
    DiagnosticMessage {
        code: 2403,
        category: DiagnosticCategory::Error,
        message: "Subsequent variable declarations must have the same type.  Variable '{0}' must be of type '{1}', but here has type '{2}'.",
    },
    DiagnosticMessage {
        code: 2404,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot use a type annotation.",
    },
    DiagnosticMessage {
        code: 2405,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement must be of type 'string' or 'any'.",
    },
    DiagnosticMessage {
        code: 2406,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2407,
        category: DiagnosticCategory::Error,
        message: "The right-hand side of a 'for...in' statement must be of type 'any', an object type or a type parameter, but here has type '{0}'.",
    },
    DiagnosticMessage {
        code: 2408,
        category: DiagnosticCategory::Error,
        message: "Setters cannot return a value.",
    },
    DiagnosticMessage {
        code: 2409,
        category: DiagnosticCategory::Error,
        message: "Return type of constructor signature must be assignable to the instance type of the class.",
    },
    DiagnosticMessage {
        code: 2410,
        category: DiagnosticCategory::Error,
        message: "The 'with' statement is not supported. All symbols in a 'with' block will have type 'any'.",
    },
    DiagnosticMessage {
        code: 2411,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of type '{1}' is not assignable to '{2}' index type '{3}'.",
    },
    DiagnosticMessage {
        code: 2412,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target.",
    },
    DiagnosticMessage {
        code: 2413,
        category: DiagnosticCategory::Error,
        message: "'{0}' index type '{1}' is not assignable to '{2}' index type '{3}'.",
    },
    DiagnosticMessage {
        code: 2414,
        category: DiagnosticCategory::Error,
        message: "Class name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2415,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' incorrectly extends base class '{1}'.",
    },
    DiagnosticMessage {
        code: 2416,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' in type '{1}' is not assignable to the same property in base type '{2}'.",
    },
    DiagnosticMessage {
        code: 2417,
        category: DiagnosticCategory::Error,
        message: "Class static side '{0}' incorrectly extends base class static side '{1}'.",
    },
    DiagnosticMessage {
        code: 2418,
        category: DiagnosticCategory::Error,
        message: "Type of computed property's value is '{0}', which is not assignable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2419,
        category: DiagnosticCategory::Error,
        message: "Types of construct signatures are incompatible.",
    },
    DiagnosticMessage {
        code: 2420,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' incorrectly implements interface '{1}'.",
    },
    DiagnosticMessage {
        code: 2422,
        category: DiagnosticCategory::Error,
        message: "A class can only implement an object type or intersection of object types with statically known members.",
    },
    DiagnosticMessage {
        code: 2423,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member function '{1}', but extended class '{2}' defines it as instance member accessor.",
    },
    DiagnosticMessage {
        code: 2425,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member property '{1}', but extended class '{2}' defines it as instance member function.",
    },
    DiagnosticMessage {
        code: 2426,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' defines instance member accessor '{1}', but extended class '{2}' defines it as instance member function.",
    },
    DiagnosticMessage {
        code: 2427,
        category: DiagnosticCategory::Error,
        message: "Interface name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2428,
        category: DiagnosticCategory::Error,
        message: "All declarations of '{0}' must have identical type parameters.",
    },
    DiagnosticMessage {
        code: 2430,
        category: DiagnosticCategory::Error,
        message: "Interface '{0}' incorrectly extends interface '{1}'.",
    },
    DiagnosticMessage {
        code: 2431,
        category: DiagnosticCategory::Error,
        message: "Enum name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2432,
        category: DiagnosticCategory::Error,
        message: "In an enum with multiple declarations, only one declaration can omit an initializer for its first enum element.",
    },
    DiagnosticMessage {
        code: 2433,
        category: DiagnosticCategory::Error,
        message: "A namespace declaration cannot be in a different file from a class or function with which it is merged.",
    },
    DiagnosticMessage {
        code: 2434,
        category: DiagnosticCategory::Error,
        message: "A namespace declaration cannot be located prior to a class or function with which it is merged.",
    },
    DiagnosticMessage {
        code: 2435,
        category: DiagnosticCategory::Error,
        message: "Ambient modules cannot be nested in other modules or namespaces.",
    },
    DiagnosticMessage {
        code: 2436,
        category: DiagnosticCategory::Error,
        message: "Ambient module declaration cannot specify relative module name.",
    },
    DiagnosticMessage {
        code: 2437,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' is hidden by a local declaration with the same name.",
    },
    DiagnosticMessage {
        code: 2438,
        category: DiagnosticCategory::Error,
        message: "Import name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2439,
        category: DiagnosticCategory::Error,
        message: "Import or export declaration in an ambient module declaration cannot reference module through relative module name.",
    },
    DiagnosticMessage {
        code: 2440,
        category: DiagnosticCategory::Error,
        message: "Import declaration conflicts with local declaration of '{0}'.",
    },
    DiagnosticMessage {
        code: 2441,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module.",
    },
    DiagnosticMessage {
        code: 2442,
        category: DiagnosticCategory::Error,
        message: "Types have separate declarations of a private property '{0}'.",
    },
    DiagnosticMessage {
        code: 2443,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected but type '{1}' is not a class derived from '{2}'.",
    },
    DiagnosticMessage {
        code: 2444,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected in type '{1}' but public in type '{2}'.",
    },
    DiagnosticMessage {
        code: 2445,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected and only accessible within class '{1}' and its subclasses.",
    },
    DiagnosticMessage {
        code: 2446,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is protected and only accessible through an instance of class '{1}'. This is an instance of class '{2}'.",
    },
    DiagnosticMessage {
        code: 2447,
        category: DiagnosticCategory::Error,
        message: "The '{0}' operator is not allowed for boolean types. Consider using '{1}' instead.",
    },
    DiagnosticMessage {
        code: 2448,
        category: DiagnosticCategory::Error,
        message: "Block-scoped variable '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2449,
        category: DiagnosticCategory::Error,
        message: "Class '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2450,
        category: DiagnosticCategory::Error,
        message: "Enum '{0}' used before its declaration.",
    },
    DiagnosticMessage {
        code: 2451,
        category: DiagnosticCategory::Error,
        message: "Cannot redeclare block-scoped variable '{0}'.",
    },
    DiagnosticMessage {
        code: 2452,
        category: DiagnosticCategory::Error,
        message: "An enum member cannot have a numeric name.",
    },
    DiagnosticMessage {
        code: 2454,
        category: DiagnosticCategory::Error,
        message: "Variable '{0}' is used before being assigned.",
    },
    DiagnosticMessage {
        code: 2456,
        category: DiagnosticCategory::Error,
        message: "Type alias '{0}' circularly references itself.",
    },
    DiagnosticMessage {
        code: 2457,
        category: DiagnosticCategory::Error,
        message: "Type alias name cannot be '{0}'.",
    },
    DiagnosticMessage {
        code: 2458,
        category: DiagnosticCategory::Error,
        message: "An AMD module cannot have multiple name assignments.",
    },
    DiagnosticMessage {
        code: 2459,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' declares '{1}' locally, but it is not exported.",
    },
    DiagnosticMessage {
        code: 2460,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' declares '{1}' locally, but it is exported as '{2}'.",
    },
    DiagnosticMessage {
        code: 2461,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type.",
    },
    DiagnosticMessage {
        code: 2462,
        category: DiagnosticCategory::Error,
        message: "A rest element must be last in a destructuring pattern.",
    },
    DiagnosticMessage {
        code: 2463,
        category: DiagnosticCategory::Error,
        message: "A binding pattern parameter cannot be optional in an implementation signature.",
    },
    DiagnosticMessage {
        code: 2464,
        category: DiagnosticCategory::Error,
        message: "A computed property name must be of type 'string', 'number', 'symbol', or 'any'.",
    },
    DiagnosticMessage {
        code: 2465,
        category: DiagnosticCategory::Error,
        message: "'this' cannot be referenced in a computed property name.",
    },
    DiagnosticMessage {
        code: 2466,
        category: DiagnosticCategory::Error,
        message: "'super' cannot be referenced in a computed property name.",
    },
    DiagnosticMessage {
        code: 2467,
        category: DiagnosticCategory::Error,
        message: "A computed property name cannot reference a type parameter from its containing type.",
    },
    DiagnosticMessage {
        code: 2468,
        category: DiagnosticCategory::Error,
        message: "Cannot find global value '{0}'.",
    },
    DiagnosticMessage {
        code: 2469,
        category: DiagnosticCategory::Error,
        message: "The '{0}' operator cannot be applied to type 'symbol'.",
    },
    DiagnosticMessage {
        code: 2472,
        category: DiagnosticCategory::Error,
        message: "Spread operator in 'new' expressions is only available when targeting ECMAScript 5 and higher.",
    },
    DiagnosticMessage {
        code: 2473,
        category: DiagnosticCategory::Error,
        message: "Enum declarations must all be const or non-const.",
    },
    DiagnosticMessage {
        code: 2474,
        category: DiagnosticCategory::Error,
        message: "const enum member initializers must be constant expressions.",
    },
    DiagnosticMessage {
        code: 2475,
        category: DiagnosticCategory::Error,
        message: "'const' enums can only be used in property or index access expressions or the right hand side of an import declaration or export assignment or type query.",
    },
    DiagnosticMessage {
        code: 2476,
        category: DiagnosticCategory::Error,
        message: "A const enum member can only be accessed using a string literal.",
    },
    DiagnosticMessage {
        code: 2477,
        category: DiagnosticCategory::Error,
        message: "'const' enum member initializer was evaluated to a non-finite value.",
    },
    DiagnosticMessage {
        code: 2478,
        category: DiagnosticCategory::Error,
        message: "'const' enum member initializer was evaluated to disallowed value 'NaN'.",
    },
    DiagnosticMessage {
        code: 2480,
        category: DiagnosticCategory::Error,
        message: "'let' is not allowed to be used as a name in 'let' or 'const' declarations.",
    },
    DiagnosticMessage {
        code: 2481,
        category: DiagnosticCategory::Error,
        message: "Cannot initialize outer scoped variable '{0}' in the same scope as block scoped declaration '{1}'.",
    },
    DiagnosticMessage {
        code: 2483,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement cannot use a type annotation.",
    },
    DiagnosticMessage {
        code: 2484,
        category: DiagnosticCategory::Error,
        message: "Export declaration conflicts with exported declaration of '{0}'.",
    },
    DiagnosticMessage {
        code: 2487,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...of' statement must be a variable or a property access.",
    },
    DiagnosticMessage {
        code: 2488,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' must have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2489,
        category: DiagnosticCategory::Error,
        message: "An iterator must have a 'next()' method.",
    },
    DiagnosticMessage {
        code: 2490,
        category: DiagnosticCategory::Error,
        message: "The type returned by the '{0}()' method of an iterator must have a 'value' property.",
    },
    DiagnosticMessage {
        code: 2491,
        category: DiagnosticCategory::Error,
        message: "The left-hand side of a 'for...in' statement cannot be a destructuring pattern.",
    },
    DiagnosticMessage {
        code: 2492,
        category: DiagnosticCategory::Error,
        message: "Cannot redeclare identifier '{0}' in catch clause.",
    },
    DiagnosticMessage {
        code: 2493,
        category: DiagnosticCategory::Error,
        message: "Tuple type '{0}' of length '{1}' has no element at index '{2}'.",
    },
    DiagnosticMessage {
        code: 2494,
        category: DiagnosticCategory::Error,
        message: "Using a string in a 'for...of' statement is only supported in ECMAScript 5 and higher.",
    },
    DiagnosticMessage {
        code: 2495,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or a string type.",
    },
    DiagnosticMessage {
        code: 2496,
        category: DiagnosticCategory::Error,
        message: "The 'arguments' object cannot be referenced in an arrow function in ES5. Consider using a standard function expression.",
    },
    DiagnosticMessage {
        code: 2497,
        category: DiagnosticCategory::Error,
        message: "This module can only be referenced with ECMAScript imports/exports by turning on the '{0}' flag and referencing its default export.",
    },
    DiagnosticMessage {
        code: 2498,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' uses 'export =' and cannot be used with 'export *'.",
    },
    DiagnosticMessage {
        code: 2499,
        category: DiagnosticCategory::Error,
        message: "An interface can only extend an identifier/qualified-name with optional type arguments.",
    },
    DiagnosticMessage {
        code: 2500,
        category: DiagnosticCategory::Error,
        message: "A class can only implement an identifier/qualified-name with optional type arguments.",
    },
    DiagnosticMessage {
        code: 2501,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot contain a binding pattern.",
    },
    DiagnosticMessage {
        code: 2502,
        category: DiagnosticCategory::Error,
        message: "'{0}' is referenced directly or indirectly in its own type annotation.",
    },
    DiagnosticMessage {
        code: 2503,
        category: DiagnosticCategory::Error,
        message: "Cannot find namespace '{0}'.",
    },
    DiagnosticMessage {
        code: 2504,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' must have a '[Symbol.asyncIterator]()' method that returns an async iterator.",
    },
    DiagnosticMessage {
        code: 2505,
        category: DiagnosticCategory::Error,
        message: "A generator cannot have a 'void' type annotation.",
    },
    DiagnosticMessage {
        code: 2506,
        category: DiagnosticCategory::Error,
        message: "'{0}' is referenced directly or indirectly in its own base expression.",
    },
    DiagnosticMessage {
        code: 2507,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not a constructor function type.",
    },
    DiagnosticMessage {
        code: 2508,
        category: DiagnosticCategory::Error,
        message: "No base constructor has the specified number of type arguments.",
    },
    DiagnosticMessage {
        code: 2509,
        category: DiagnosticCategory::Error,
        message: "Base constructor return type '{0}' is not an object type or intersection of object types with statically known members.",
    },
    DiagnosticMessage {
        code: 2510,
        category: DiagnosticCategory::Error,
        message: "Base constructors must all have the same return type.",
    },
    DiagnosticMessage {
        code: 2511,
        category: DiagnosticCategory::Error,
        message: "Cannot create an instance of an abstract class.",
    },
    DiagnosticMessage {
        code: 2512,
        category: DiagnosticCategory::Error,
        message: "Overload signatures must all be abstract or non-abstract.",
    },
    DiagnosticMessage {
        code: 2513,
        category: DiagnosticCategory::Error,
        message: "Abstract method '{0}' in class '{1}' cannot be accessed via super expression.",
    },
    DiagnosticMessage {
        code: 2514,
        category: DiagnosticCategory::Error,
        message: "A tuple type cannot be indexed with a negative value.",
    },
    DiagnosticMessage {
        code: 2515,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' does not implement inherited abstract member {1} from class '{2}'.",
    },
    DiagnosticMessage {
        code: 2516,
        category: DiagnosticCategory::Error,
        message: "All declarations of an abstract method must be consecutive.",
    },
    DiagnosticMessage {
        code: 2517,
        category: DiagnosticCategory::Error,
        message: "Cannot assign an abstract constructor type to a non-abstract constructor type.",
    },
    DiagnosticMessage {
        code: 2518,
        category: DiagnosticCategory::Error,
        message: "A 'this'-based type guard is not compatible with a parameter-based type guard.",
    },
    DiagnosticMessage {
        code: 2519,
        category: DiagnosticCategory::Error,
        message: "An async iterator must have a 'next()' method.",
    },
    DiagnosticMessage {
        code: 2520,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler uses declaration '{1}' to support async functions.",
    },
    DiagnosticMessage {
        code: 2522,
        category: DiagnosticCategory::Error,
        message: "The 'arguments' object cannot be referenced in an async function or method in ES5. Consider using a standard function or method.",
    },
    DiagnosticMessage {
        code: 2523,
        category: DiagnosticCategory::Error,
        message: "'yield' expressions cannot be used in a parameter initializer.",
    },
    DiagnosticMessage {
        code: 2524,
        category: DiagnosticCategory::Error,
        message: "'await' expressions cannot be used in a parameter initializer.",
    },
    DiagnosticMessage {
        code: 2526,
        category: DiagnosticCategory::Error,
        message: "A 'this' type is available only in a non-static member of a class or interface.",
    },
    DiagnosticMessage {
        code: 2527,
        category: DiagnosticCategory::Error,
        message: "The inferred type of '{0}' references an inaccessible '{1}' type. A type annotation is necessary.",
    },
    DiagnosticMessage {
        code: 2528,
        category: DiagnosticCategory::Error,
        message: "A module cannot have multiple default exports.",
    },
    DiagnosticMessage {
        code: 2529,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '{0}'. Compiler reserves name '{1}' in top level scope of a module containing async functions.",
    },
    DiagnosticMessage {
        code: 2530,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is incompatible with index signature.",
    },
    DiagnosticMessage {
        code: 2531,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'null'.",
    },
    DiagnosticMessage {
        code: 2532,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'undefined'.",
    },
    DiagnosticMessage {
        code: 2533,
        category: DiagnosticCategory::Error,
        message: "Object is possibly 'null' or 'undefined'.",
    },
    DiagnosticMessage {
        code: 2534,
        category: DiagnosticCategory::Error,
        message: "A function returning 'never' cannot have a reachable end point.",
    },
    DiagnosticMessage {
        code: 2536,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' cannot be used to index type '{1}'.",
    },
    DiagnosticMessage {
        code: 2537,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no matching index signature for type '{1}'.",
    },
    DiagnosticMessage {
        code: 2538,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' cannot be used as an index type.",
    },
    DiagnosticMessage {
        code: 2539,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is not a variable.",
    },
    DiagnosticMessage {
        code: 2540,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a read-only property.",
    },
    DiagnosticMessage {
        code: 2542,
        category: DiagnosticCategory::Error,
        message: "Index signature in type '{0}' only permits reading.",
    },
    DiagnosticMessage {
        code: 2543,
        category: DiagnosticCategory::Error,
        message: "Duplicate identifier '_newTarget'. Compiler uses variable declaration '_newTarget' to capture 'new.target' meta-property reference.",
    },
    DiagnosticMessage {
        code: 2544,
        category: DiagnosticCategory::Error,
        message: "Expression resolves to variable declaration '_newTarget' that compiler uses to capture 'new.target' meta-property reference.",
    },
    DiagnosticMessage {
        code: 2545,
        category: DiagnosticCategory::Error,
        message: "A mixin class must have a constructor with a single rest parameter of type 'any[]'.",
    },
    DiagnosticMessage {
        code: 2547,
        category: DiagnosticCategory::Error,
        message: "The type returned by the '{0}()' method of an async iterator must be a promise for a type with a 'value' property.",
    },
    DiagnosticMessage {
        code: 2548,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or does not have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2549,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not an array type or a string type or does not have a '[Symbol.iterator]()' method that returns an iterator.",
    },
    DiagnosticMessage {
        code: 2550,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{2}' or later.",
    },
    DiagnosticMessage {
        code: 2551,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2552,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2553,
        category: DiagnosticCategory::Error,
        message: "Computed values are not permitted in an enum with string valued members.",
    },
    DiagnosticMessage {
        code: 2554,
        category: DiagnosticCategory::Error,
        message: "Expected {0} arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2555,
        category: DiagnosticCategory::Error,
        message: "Expected at least {0} arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2556,
        category: DiagnosticCategory::Error,
        message: "A spread argument must either have a tuple type or be passed to a rest parameter.",
    },
    DiagnosticMessage {
        code: 2558,
        category: DiagnosticCategory::Error,
        message: "Expected {0} type arguments, but got {1}.",
    },
    DiagnosticMessage {
        code: 2559,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no properties in common with type '{1}'.",
    },
    DiagnosticMessage {
        code: 2560,
        category: DiagnosticCategory::Error,
        message: "Value of type '{0}' has no properties in common with type '{1}'. Did you mean to call it?",
    },
    DiagnosticMessage {
        code: 2561,
        category: DiagnosticCategory::Error,
        message: "Object literal may only specify known properties, but '{0}' does not exist in type '{1}'. Did you mean to write '{2}'?",
    },
    DiagnosticMessage {
        code: 2562,
        category: DiagnosticCategory::Error,
        message: "Base class expressions cannot reference class type parameters.",
    },
    DiagnosticMessage {
        code: 2563,
        category: DiagnosticCategory::Error,
        message: "The containing function or module body is too large for control flow analysis.",
    },
    DiagnosticMessage {
        code: 2564,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' has no initializer and is not definitely assigned in the constructor.",
    },
    DiagnosticMessage {
        code: 2565,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' is used before being assigned.",
    },
    DiagnosticMessage {
        code: 2566,
        category: DiagnosticCategory::Error,
        message: "A rest element cannot have a property name.",
    },
    DiagnosticMessage {
        code: 2567,
        category: DiagnosticCategory::Error,
        message: "Enum declarations can only merge with namespace or other enum declarations.",
    },
    DiagnosticMessage {
        code: 2568,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' may not exist on type '{1}'. Did you mean '{2}'?",
    },
    DiagnosticMessage {
        code: 2570,
        category: DiagnosticCategory::Error,
        message: "Could not find name '{0}'. Did you mean '{1}'?",
    },
    DiagnosticMessage {
        code: 2571,
        category: DiagnosticCategory::Error,
        message: "Object is of type 'unknown'.",
    },
    DiagnosticMessage {
        code: 2574,
        category: DiagnosticCategory::Error,
        message: "A rest element type must be an array type.",
    },
    DiagnosticMessage {
        code: 2575,
        category: DiagnosticCategory::Error,
        message: "No overload expects {0} arguments, but overloads do exist that expect either {1} or {2} arguments.",
    },
    DiagnosticMessage {
        code: 2576,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' does not exist on type '{1}'. Did you mean to access the static member '{2}' instead?",
    },
    DiagnosticMessage {
        code: 2577,
        category: DiagnosticCategory::Error,
        message: "Return type annotation circularly references itself.",
    },
    DiagnosticMessage {
        code: 2578,
        category: DiagnosticCategory::Error,
        message: "Unused '@ts-expect-error' directive.",
    },
    DiagnosticMessage {
        code: 2580,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node`.",
    },
    DiagnosticMessage {
        code: 2581,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery`.",
    },
    DiagnosticMessage {
        code: 2582,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha`.",
    },
    DiagnosticMessage {
        code: 2583,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to '{1}' or later.",
    },
    DiagnosticMessage {
        code: 2584,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to change your target library? Try changing the 'lib' compiler option to include 'dom'.",
    },
    DiagnosticMessage {
        code: 2585,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a value here. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.",
    },
    DiagnosticMessage {
        code: 2588,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a constant.",
    },
    DiagnosticMessage {
        code: 2589,
        category: DiagnosticCategory::Error,
        message: "Type instantiation is excessively deep and possibly infinite.",
    },
    DiagnosticMessage {
        code: 2590,
        category: DiagnosticCategory::Error,
        message: "Expression produces a union type that is too complex to represent.",
    },
    DiagnosticMessage {
        code: 2591,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2592,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for jQuery? Try `npm i --save-dev @types/jquery` and then add 'jquery' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2593,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Do you need to install type definitions for a test runner? Try `npm i --save-dev @types/jest` or `npm i --save-dev @types/mocha` and then add 'jest' or 'mocha' to the types field in your tsconfig.",
    },
    DiagnosticMessage {
        code: 2594,
        category: DiagnosticCategory::Error,
        message: "This module is declared with 'export =', and can only be used with a default import when using the '{0}' flag.",
    },
    DiagnosticMessage {
        code: 2595,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a default import.",
    },
    DiagnosticMessage {
        code: 2596,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2597,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a 'require' call or by using a default import.",
    },
    DiagnosticMessage {
        code: 2598,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using a 'require' call or by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2602,
        category: DiagnosticCategory::Error,
        message: "JSX element implicitly has type 'any' because the global type 'JSX.Element' does not exist.",
    },
    DiagnosticMessage {
        code: 2603,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' in type '{1}' is not assignable to type '{2}'.",
    },
    DiagnosticMessage {
        code: 2604,
        category: DiagnosticCategory::Error,
        message: "JSX element type '{0}' does not have any construct or call signatures.",
    },
    DiagnosticMessage {
        code: 2606,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' of JSX spread attribute is not assignable to target property.",
    },
    DiagnosticMessage {
        code: 2607,
        category: DiagnosticCategory::Error,
        message: "JSX element class does not support attributes because it does not have a '{0}' property.",
    },
    DiagnosticMessage {
        code: 2608,
        category: DiagnosticCategory::Error,
        message: "The global type 'JSX.{0}' may not have more than one property.",
    },
    DiagnosticMessage {
        code: 2609,
        category: DiagnosticCategory::Error,
        message: "JSX spread child must be an array type.",
    },
    DiagnosticMessage {
        code: 2610,
        category: DiagnosticCategory::Error,
        message: "'{0}' is defined as an accessor in class '{1}', but is overridden here in '{2}' as an instance property.",
    },
    DiagnosticMessage {
        code: 2611,
        category: DiagnosticCategory::Error,
        message: "'{0}' is defined as a property in class '{1}', but is overridden here in '{2}' as an accessor.",
    },
    DiagnosticMessage {
        code: 2612,
        category: DiagnosticCategory::Error,
        message: "Property '{0}' will overwrite the base property in '{1}'. If this is intentional, add an initializer. Otherwise, add a 'declare' modifier or remove the redundant declaration.",
    },
    DiagnosticMessage {
        code: 2613,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no default export. Did you mean to use 'import { {1} } from {0}' instead?",
    },
    DiagnosticMessage {
        code: 2614,
        category: DiagnosticCategory::Error,
        message: "Module '{0}' has no exported member '{1}'. Did you mean to use 'import {1} from {0}' instead?",
    },
    DiagnosticMessage {
        code: 2615,
        category: DiagnosticCategory::Error,
        message: "Type of property '{0}' circularly references itself in mapped type '{1}'.",
    },
    DiagnosticMessage {
        code: 2616,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using 'import {1} = require({2})' or a default import.",
    },
    DiagnosticMessage {
        code: 2617,
        category: DiagnosticCategory::Error,
        message: "'{0}' can only be imported by using 'import {1} = require({2})' or by turning on the 'esModuleInterop' flag and using a default import.",
    },
    DiagnosticMessage {
        code: 2618,
        category: DiagnosticCategory::Error,
        message: "Source has {0} element(s) but target requires {1}.",
    },
    DiagnosticMessage {
        code: 2619,
        category: DiagnosticCategory::Error,
        message: "Source has {0} element(s) but target allows only {1}.",
    },
    DiagnosticMessage {
        code: 2620,
        category: DiagnosticCategory::Error,
        message: "Target requires {0} element(s) but source may have fewer.",
    },
    DiagnosticMessage {
        code: 2621,
        category: DiagnosticCategory::Error,
        message: "Target allows only {0} element(s) but source may have more.",
    },
    DiagnosticMessage {
        code: 2623,
        category: DiagnosticCategory::Error,
        message: "Source provides no match for required element at position {0} in target.",
    },
    DiagnosticMessage {
        code: 2624,
        category: DiagnosticCategory::Error,
        message: "Source provides no match for variadic element at position {0} in target.",
    },
    DiagnosticMessage {
        code: 2625,
        category: DiagnosticCategory::Error,
        message: "Variadic element at position {0} in source does not match element at position {1} in target.",
    },
    DiagnosticMessage {
        code: 2626,
        category: DiagnosticCategory::Error,
        message: "Type at position {0} in source is not compatible with type at position {1} in target.",
    },
    DiagnosticMessage {
        code: 2627,
        category: DiagnosticCategory::Error,
        message: "Type at positions {0} through {1} in source is not compatible with type at position {2} in target.",
    },
    DiagnosticMessage {
        code: 2628,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is an enum.",
    },
    DiagnosticMessage {
        code: 2629,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a class.",
    },
    DiagnosticMessage {
        code: 2630,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a function.",
    },
    DiagnosticMessage {
        code: 2631,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is a namespace.",
    },
    DiagnosticMessage {
        code: 2632,
        category: DiagnosticCategory::Error,
        message: "Cannot assign to '{0}' because it is an import.",
    },
    DiagnosticMessage {
        code: 2633,
        category: DiagnosticCategory::Error,
        message: "JSX property access expressions cannot include JSX namespace names",
    },
    DiagnosticMessage {
        code: 2634,
        category: DiagnosticCategory::Error,
        message: "'{0}' index signatures are incompatible.",
    },
    DiagnosticMessage {
        code: 2635,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' has no signatures for which the type argument list is applicable.",
    },
    DiagnosticMessage {
        code: 2636,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not assignable to type '{1}' as implied by variance annotation.",
    },
    DiagnosticMessage {
        code: 2637,
        category: DiagnosticCategory::Error,
        message: "Variance annotations are only supported in type aliases for object, function, constructor, and mapped types.",
    },
    DiagnosticMessage {
        code: 2638,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' may represent a primitive value, which is not permitted as the right operand of the 'in' operator.",
    },
    DiagnosticMessage {
        code: 2639,
        category: DiagnosticCategory::Error,
        message: "React components cannot include JSX namespace names",
    },
    DiagnosticMessage {
        code: 2649,
        category: DiagnosticCategory::Error,
        message: "Cannot augment module '{0}' with value exports because it resolves to a non-module entity.",
    },
    DiagnosticMessage {
        code: 2650,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression is missing implementations for the following members of '{0}': {1} and {2} more.",
    },
    DiagnosticMessage {
        code: 2651,
        category: DiagnosticCategory::Error,
        message: "A member initializer in a enum declaration cannot reference members declared after it, including members defined in other enums.",
    },
    DiagnosticMessage {
        code: 2652,
        category: DiagnosticCategory::Error,
        message: "Merged declaration '{0}' cannot include a default export declaration. Consider adding a separate 'export default {0}' declaration instead.",
    },
    DiagnosticMessage {
        code: 2653,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression does not implement inherited abstract member '{0}' from class '{1}'.",
    },
    DiagnosticMessage {
        code: 2654,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2}.",
    },
    DiagnosticMessage {
        code: 2655,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class '{0}' is missing implementations for the following members of '{1}': {2} and {3} more.",
    },
    DiagnosticMessage {
        code: 2656,
        category: DiagnosticCategory::Error,
        message: "Non-abstract class expression is missing implementations for the following members of '{0}': {1}.",
    },
    DiagnosticMessage {
        code: 2657,
        category: DiagnosticCategory::Error,
        message: "JSX expressions must have one parent element.",
    },
    DiagnosticMessage {
        code: 2658,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' provides no match for the signature '{1}'.",
    },
    DiagnosticMessage {
        code: 2659,
        category: DiagnosticCategory::Error,
        message: "'super' is only allowed in members of object literal expressions when option 'target' is 'ES2015' or higher.",
    },
    DiagnosticMessage {
        code: 2660,
        category: DiagnosticCategory::Error,
        message: "'super' can only be referenced in members of derived classes or object literal expressions.",
    },
    DiagnosticMessage {
        code: 2661,
        category: DiagnosticCategory::Error,
        message: "Cannot export '{0}'. Only local declarations can be exported from a module.",
    },
    DiagnosticMessage {
        code: 2662,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean the static member '{1}.{0}'?",
    },
    DiagnosticMessage {
        code: 2663,
        category: DiagnosticCategory::Error,
        message: "Cannot find name '{0}'. Did you mean the instance member 'this.{0}'?",
    },
    DiagnosticMessage {
        code: 2664,
        category: DiagnosticCategory::Error,
        message: "Invalid module name in augmentation, module '{0}' cannot be found.",
    },
    DiagnosticMessage {
        code: 2665,
        category: DiagnosticCategory::Error,
        message: "Invalid module name in augmentation. Module '{0}' resolves to an untyped module at '{1}', which cannot be augmented.",
    },
    DiagnosticMessage {
        code: 2666,
        category: DiagnosticCategory::Error,
        message: "Exports and export assignments are not permitted in module augmentations.",
    },
    DiagnosticMessage {
        code: 2667,
        category: DiagnosticCategory::Error,
        message: "Imports are not permitted in module augmentations. Consider moving them to the enclosing external module.",
    },
    DiagnosticMessage {
        code: 2668,
        category: DiagnosticCategory::Error,
        message: "'export' modifier cannot be applied to ambient modules and module augmentations since they are always visible.",
    },
    DiagnosticMessage {
        code: 2669,
        category: DiagnosticCategory::Error,
        message: "Augmentations for the global scope can only be directly nested in external modules or ambient module declarations.",
    },
    DiagnosticMessage {
        code: 2670,
        category: DiagnosticCategory::Error,
        message: "Augmentations for the global scope should have 'declare' modifier unless they appear in already ambient context.",
    },
    DiagnosticMessage {
        code: 2671,
        category: DiagnosticCategory::Error,
        message: "Cannot augment module '{0}' because it resolves to a non-module entity.",
    },
    DiagnosticMessage {
        code: 2672,
        category: DiagnosticCategory::Error,
        message: "Cannot assign a '{0}' constructor type to a '{1}' constructor type.",
    },
    DiagnosticMessage {
        code: 2673,
        category: DiagnosticCategory::Error,
        message: "Constructor of class '{0}' is private and only accessible within the class declaration.",
    },
    DiagnosticMessage {
        code: 2674,
        category: DiagnosticCategory::Error,
        message: "Constructor of class '{0}' is protected and only accessible within the class declaration.",
    },
    DiagnosticMessage {
        code: 2675,
        category: DiagnosticCategory::Error,
        message: "Cannot extend a class '{0}'. Class constructor is marked as private.",
    },
    DiagnosticMessage {
        code: 2676,
        category: DiagnosticCategory::Error,
        message: "Accessors must both be abstract or non-abstract.",
    },
    DiagnosticMessage {
        code: 2677,
        category: DiagnosticCategory::Error,
        message: "A type predicate's type must be assignable to its parameter's type.",
    },
    DiagnosticMessage {
        code: 2678,
        category: DiagnosticCategory::Error,
        message: "Type '{0}' is not comparable to type '{1}'.",
    },
    DiagnosticMessage {
        code: 2679,
        category: DiagnosticCategory::Error,
        message: "A function that is called with the 'new' keyword cannot have a 'this' type that is 'void'.",
    },
    DiagnosticMessage {
        code: 2680,
        category: DiagnosticCategory::Error,
        message: "A '{0}' parameter must be the first parameter.",
    },
    DiagnosticMessage {
        code: 2681,
        category: DiagnosticCategory::Error,
        message: "A constructor cannot have a 'this' parameter.",
    },
    DiagnosticMessage {
        code: 2683,
        category: DiagnosticCategory::Error,
        message: "'this' implicitly has type 'any' because it does not have a type annotation.",
    },
    DiagnosticMessage {
        code: 2684,
        category: DiagnosticCategory::Error,
        message: "The 'this' context of type '{0}' is not assignable to method's 'this' of type '{1}'.",
    },
    DiagnosticMessage {
        code: 2685,
        category: DiagnosticCategory::Error,
        message: "The 'this' types of each signature are incompatible.",
    },
    DiagnosticMessage {
        code: 2686,
        category: DiagnosticCategory::Error,
        message: "'{0}' refers to a UMD global, but the current file is a module. Consider adding an import instead.",
    },
    DiagnosticMessage {
        code: 2687,
        category: DiagnosticCategory::Error,
        message: "All declarations of '{0}' must have identical modifiers.",
    },
    DiagnosticMessage {
        code: 2688,
        category: DiagnosticCategory::Error,
        message: "Cannot find type definition file for '{0}'.",
    },
    DiagnosticMessage {
        code: 2689,
        category: DiagnosticCategory::Error,
        message: "Cannot extend an interface '{0}'. Did you mean 'implements'?",
    },
    DiagnosticMessage {
        code: 2690,
        category: DiagnosticCategory::Error,
        message: "'{0}' only refers to a type, but is being used as a value here. Did you mean to use '{1} in {0}'?",
    },
    DiagnosticMessage {
        code: 2692,
        category: DiagnosticCategory::Error,
        message: "'{0}' is a primitive, but '{1}' is a wrapper object. Prefer using '{0}' when possible.",
    },
];
