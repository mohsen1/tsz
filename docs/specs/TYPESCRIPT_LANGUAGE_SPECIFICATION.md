# TypeScript Language Specification

**Version:** Based on TypeScript 5.x
**Last Updated:** January 2026
**Status:** Comprehensive Language Reference

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Lexical Structure](#2-lexical-structure)
3. [Types](#3-types)
4. [Variables and Declarations](#4-variables-and-declarations)
5. [Expressions](#5-expressions)
6. [Statements](#6-statements)
7. [Functions](#7-functions)
8. [Classes](#8-classes)
9. [Interfaces](#9-interfaces)
10. [Enums](#10-enums)
11. [Generics](#11-generics)
12. [Type Operators and Advanced Types](#12-type-operators-and-advanced-types)
13. [Modules](#13-modules)
14. [Namespaces](#14-namespaces)
15. [Decorators](#15-decorators)
16. [JSX/TSX](#16-jsxtsx)
17. [Declaration Files](#17-declaration-files)
18. [Type Compatibility](#18-type-compatibility)
19. [Type Inference](#19-type-inference)
20. [Type Narrowing](#20-type-narrowing)
21. [Utility Types](#21-utility-types)
22. [Compiler Options](#22-compiler-options)

---

## 1. Introduction

### 1.1 Overview

TypeScript is a statically typed superset of JavaScript that compiles to plain JavaScript. It adds optional static typing and class-based object-oriented programming to the language. TypeScript is designed for the development of large applications and transpiles to JavaScript.

### 1.2 Design Goals

1. **Statically identify constructs that are likely to be errors**
2. **Provide a structuring mechanism for larger pieces of code**
3. **Impose no runtime overhead on emitted programs**
4. **Emit clean, idiomatic, recognizable JavaScript code**
5. **Produce a language that is composable and easy to reason about**
6. **Align with current and future ECMAScript proposals**
7. **Preserve runtime behavior of all JavaScript code**
8. **Avoid adding expression-level syntax**
9. **Use a consistent, fully erasable, structural type system**
10. **Be a cross-platform development tool**

### 1.3 Language Characteristics

- **Structural Typing**: TypeScript uses structural subtyping (duck typing), not nominal typing
- **Gradual Typing**: Mix typed and untyped code via `any` and `unknown`
- **Type Erasure**: All type annotations are removed at compile time
- **Bidirectional Inference**: Types flow both up (synthesis) and down (checking)
- **Flow-Sensitive**: Types are narrowed based on control flow analysis

### 1.4 Relationship to JavaScript

TypeScript is a strict syntactical superset of JavaScript:
- All JavaScript code is valid TypeScript
- TypeScript adds type annotations and other features
- TypeScript compiles down to JavaScript (ES3, ES5, ES6+, ESNext)
- No runtime library is required

---

## 2. Lexical Structure

### 2.1 Source Text

TypeScript source text is a sequence of Unicode characters. The source text is first converted to a sequence of input elements, which are either tokens, comments, or white space.

### 2.2 Comments

```typescript
// Single-line comment

/* Multi-line
   comment */

/**
 * JSDoc comment - used for documentation and type annotations
 * @param name - The name parameter
 * @returns The greeting string
 */
```

### 2.3 Tokens

#### 2.3.1 Reserved Words (Keywords)

**ECMAScript Reserved Words:**
```
break      case       catch      class      const      continue
debugger   default    delete     do         else       enum
export     extends    false      finally    for        function
if         import     in         instanceof new        null
return     super      switch     this       throw      true
try        typeof     var        void       while      with
```

**Strict Mode Reserved Words:**
```
implements  interface   let         package     private
protected   public      static      yield
```

**TypeScript Contextual Keywords:**
```
abstract    accessor    any         as          asserts     assert
async       await       bigint      boolean     constructor declare
defer       get         infer       intrinsic   is          keyof
module      namespace   never       number      object      out
override    readonly    require     satisfies   set         string
symbol      type        undefined   unique      unknown     using
from        global      of
```

#### 2.3.2 Punctuators

```
{  }  (  )  [  ]  .  ...  ;  ,  <  >  <=  >=  ==  !=  ===  !==
=>  +  -  *  **  /  %  ++  --  <<  >>  >>>  &  |  ^  !  ~
&&  ||  ??  ?  ?.  :  =  +=  -=  *=  **=  /=  %=  <<=  >>=
>>>=  &=  |=  ^=  &&=  ||=  ??=  @  #
```

#### 2.3.3 Literals

**Numeric Literals:**
```typescript
42          // Decimal
0x2A        // Hexadecimal
0o52        // Octal
0b101010    // Binary
3.14        // Floating point
1e10        // Exponential
1_000_000   // Numeric separator
42n         // BigInt
```

**String Literals:**
```typescript
'single quotes'
"double quotes"
`template literal with ${expression}`
`multi-line
 template literal`
```

**Regular Expression Literals:**
```typescript
/pattern/flags
/^hello$/gi
```

### 2.4 Automatic Semicolon Insertion

TypeScript follows JavaScript's automatic semicolon insertion (ASI) rules. Semicolons are automatically inserted at the end of lines in certain circumstances.

---

## 3. Types

### 3.1 Type Universe

TypeScript's type system can be categorized hierarchically:

```
Types
├── Primitive Types
│   ├── string, number, boolean, bigint, symbol
│   ├── null, undefined, void
│   ├── never (bottom type - uninhabited)
│   └── unknown (top type - all values)
│
├── Literal Types (singleton types)
│   ├── String literals: "hello", "world"
│   ├── Number literals: 42, 3.14
│   ├── Boolean literals: true, false
│   ├── BigInt literals: 100n
│   └── Template literal types: `hello ${string}`
│
├── Compound Types
│   ├── Union (A | B) - sum type, either/or
│   ├── Intersection (A & B) - product, both
│   └── Tuple ([A, B, ...C]) - ordered product
│
├── Object Types
│   ├── Interface/Type literal: { x: T, y: U }
│   ├── Array: T[] or Array<T>
│   ├── Function: (x: T) => U
│   ├── Class (structural, with nominal hints via private)
│   └── Constructor: new (x: T) => U
│
├── Parametric Types (Generics)
│   ├── Type parameters: T, K extends keyof T
│   ├── Generic types: Array<T>, Map<K, V>
│   ├── Mapped types: { [K in keyof T]: ... }
│   └── Conditional types: T extends U ? X : Y
│
└── Type Operators
    ├── keyof T - index type query
    ├── T[K] - indexed access
    ├── typeof x - type query
    ├── infer R - type inference in conditionals
    └── readonly T - readonly modifier
```

### 3.2 Primitive Types

#### 3.2.1 `string`

Represents textual data:
```typescript
let greeting: string = "Hello, World!";
let template: string = `Hello, ${name}!`;
```

#### 3.2.2 `number`

Represents both integer and floating-point numbers:
```typescript
let integer: number = 42;
let float: number = 3.14;
let hex: number = 0xff;
let binary: number = 0b1010;
let octal: number = 0o744;
```

#### 3.2.3 `bigint`

Represents arbitrarily large integers:
```typescript
let big: bigint = 9007199254740991n;
let alsobig: bigint = BigInt(9007199254740991);
```

#### 3.2.4 `boolean`

Represents logical values:
```typescript
let isDone: boolean = false;
let isActive: boolean = true;
```

#### 3.2.5 `symbol`

Represents unique identifiers:
```typescript
let sym1: symbol = Symbol("key");
let sym2: symbol = Symbol("key");
// sym1 !== sym2

// Unique symbols
const uniqueSym: unique symbol = Symbol("unique");
```

#### 3.2.6 `null` and `undefined`

Represent absence of value:
```typescript
let u: undefined = undefined;
let n: null = null;
```

With `strictNullChecks` enabled, these are only assignable to themselves and `any`/`unknown`.

#### 3.2.7 `void`

Represents absence of a return value:
```typescript
function log(message: string): void {
    console.log(message);
}
```

#### 3.2.8 `never`

Represents values that never occur:
```typescript
// Function that never returns
function fail(message: string): never {
    throw new Error(message);
}

// Infinite loop
function infiniteLoop(): never {
    while (true) {}
}

// Exhaustive checking
type Shape = Circle | Square;
function assertNever(x: never): never {
    throw new Error("Unexpected: " + x);
}
```

#### 3.2.9 `unknown`

The type-safe counterpart to `any`:
```typescript
let value: unknown;
value = 42;
value = "hello";
value = true;

// Must narrow before use
if (typeof value === "string") {
    console.log(value.toUpperCase()); // OK
}
```

#### 3.2.10 `any`

Opts out of type checking:
```typescript
let anything: any = 42;
anything = "now a string";
anything.foo.bar.baz; // No error
```

### 3.3 Literal Types

Literal types represent exact values:

```typescript
// String literals
type Direction = "north" | "south" | "east" | "west";

// Numeric literals
type DiceRoll = 1 | 2 | 3 | 4 | 5 | 6;

// Boolean literals
type Success = true;
type Failure = false;

// Template literal types
type Greeting = `Hello, ${string}!`;
type EmailLocaleIDs = `${string}_id`;
```

### 3.4 Object Types

#### 3.4.1 Object Type Literals

```typescript
type Person = {
    name: string;
    age: number;
    email?: string;           // Optional property
    readonly id: number;      // Read-only property
};
```

#### 3.4.2 Index Signatures

```typescript
interface StringMap {
    [key: string]: string;
}

interface NumberMap {
    [index: number]: string;
}

// Mixed index signatures
interface MixedMap {
    [key: string]: string | number;
    [index: number]: string;  // Must be subtype of string index
}
```

#### 3.4.3 Call Signatures

```typescript
interface Callable {
    (x: number, y: number): number;
}

// With properties
interface CallableWithProps {
    (x: number): number;
    description: string;
}
```

#### 3.4.4 Construct Signatures

```typescript
interface Constructable {
    new (name: string): Person;
}

// Hybrid types
interface HybridType {
    (x: number): string;
    new (x: string): number[];
    property: boolean;
}
```

### 3.5 Array Types

```typescript
// Array type syntax
let list1: number[] = [1, 2, 3];
let list2: Array<number> = [1, 2, 3];

// Readonly arrays
let roArray: readonly number[] = [1, 2, 3];
let roArray2: ReadonlyArray<number> = [1, 2, 3];
```

### 3.6 Tuple Types

Fixed-length arrays with specific element types:

```typescript
// Basic tuple
let tuple: [string, number] = ["hello", 42];

// Optional elements
let optTuple: [string, number?] = ["hello"];

// Rest elements
let restTuple: [string, ...number[]] = ["hello", 1, 2, 3];

// Named tuple elements
type NamedTuple = [name: string, age: number];

// Readonly tuples
type ROTuple = readonly [string, number];
```

### 3.7 Union Types

Represent values that can be one of several types:

```typescript
type StringOrNumber = string | number;
type Status = "pending" | "approved" | "rejected";

function format(value: string | number): string {
    if (typeof value === "string") {
        return value.toUpperCase();
    }
    return value.toFixed(2);
}
```

### 3.8 Intersection Types

Combine multiple types into one:

```typescript
type Named = { name: string };
type Aged = { age: number };
type Person = Named & Aged;

// Equivalent to:
// type Person = { name: string; age: number; }
```

### 3.9 Function Types

```typescript
// Function type expression
type Add = (a: number, b: number) => number;

// Call signature in object type
type AddFn = {
    (a: number, b: number): number;
};

// Constructor type
type PersonConstructor = new (name: string) => Person;

// Function with properties
type DescribedFunction = {
    description: string;
    (x: number): number;
};
```

### 3.10 Type Aliases

Create named types:

```typescript
type ID = string | number;
type Point = { x: number; y: number };
type Callback<T> = (data: T) => void;

// Recursive types
type Tree<T> = {
    value: T;
    left?: Tree<T>;
    right?: Tree<T>;
};
```

---

## 4. Variables and Declarations

### 4.1 Variable Declarations

```typescript
// var - function-scoped, hoisted
var x = 10;

// let - block-scoped
let y = 20;

// const - block-scoped, immutable binding
const z = 30;

// Type annotations
let name: string = "Alice";
const age: number = 30;
```

### 4.2 `using` Declarations (ECMAScript Explicit Resource Management)

```typescript
// Synchronous disposal
using file = openFile("path");
// file[Symbol.dispose]() called automatically

// Asynchronous disposal
await using connection = await openConnection();
// connection[Symbol.asyncDispose]() called automatically
```

### 4.3 Destructuring

#### 4.3.1 Array Destructuring

```typescript
let [first, second] = [1, 2];
let [head, ...tail] = [1, 2, 3, 4];
let [a, , b] = [1, 2, 3]; // Skip elements

// With types
let [x, y]: [number, string] = [1, "hello"];
```

#### 4.3.2 Object Destructuring

```typescript
let { name, age } = person;
let { name: personName, age: personAge } = person;
let { name, ...rest } = person;

// With types
let { name, age }: { name: string; age: number } = person;

// Default values
let { name = "Anonymous" } = person;
```

### 4.4 Spread Operator

```typescript
// Array spread
let arr1 = [1, 2, 3];
let arr2 = [...arr1, 4, 5];

// Object spread
let obj1 = { a: 1, b: 2 };
let obj2 = { ...obj1, c: 3 };
```

---

## 5. Expressions

### 5.1 Type Assertions

```typescript
// Angle-bracket syntax (not allowed in JSX)
let str = <string>someValue;

// as syntax (preferred)
let str = someValue as string;

// const assertion
let arr = [1, 2, 3] as const;  // readonly [1, 2, 3]

// Non-null assertion
let value = maybeNull!;  // Asserts non-null
```

### 5.2 Satisfies Expression

Validates a type without changing the inferred type:

```typescript
type Colors = "red" | "green" | "blue";
type RGB = [number, number, number];

const palette = {
    red: [255, 0, 0],
    green: "#00ff00",
    blue: [0, 0, 255]
} satisfies Record<Colors, string | RGB>;

// palette.green is still string, not string | RGB
palette.green.toUpperCase(); // OK
```

### 5.3 Optional Chaining

```typescript
let value = obj?.property;
let element = arr?.[0];
let result = func?.(arg);

// Combines with nullish coalescing
let value = obj?.property ?? "default";
```

### 5.4 Nullish Coalescing

```typescript
let value = maybeNull ?? "default";
// Unlike ||, only falls back for null/undefined, not falsy values

let x = 0;
console.log(x ?? 42);  // 0
console.log(x || 42);  // 42
```

### 5.5 Logical Assignment Operators

```typescript
x ||= y;   // x = x || y
x &&= y;   // x = x && y
x ??= y;   // x = x ?? y
```

### 5.6 Template Literal Expressions

```typescript
let name = "Alice";
let greeting = `Hello, ${name}!`;
let multiline = `
    This is a
    multiline string
`;

// Tagged templates
function tag(strings: TemplateStringsArray, ...values: any[]) {
    return strings.reduce((acc, str, i) =>
        acc + str + (values[i] ?? ''), '');
}
let result = tag`Hello ${name}!`;
```

---

## 6. Statements

### 6.1 Control Flow Statements

```typescript
// if-else
if (condition) {
    // ...
} else if (otherCondition) {
    // ...
} else {
    // ...
}

// switch
switch (value) {
    case 1:
        break;
    case 2:
    case 3:
        break;
    default:
        break;
}
```

### 6.2 Loop Statements

```typescript
// for loop
for (let i = 0; i < 10; i++) { }

// for-in (iterates over keys)
for (const key in object) { }

// for-of (iterates over values)
for (const value of iterable) { }

// for-await-of (async iteration)
for await (const value of asyncIterable) { }

// while
while (condition) { }

// do-while
do { } while (condition);
```

### 6.3 Exception Handling

```typescript
try {
    throw new Error("Something went wrong");
} catch (error) {
    if (error instanceof Error) {
        console.log(error.message);
    }
} finally {
    // Always executed
}

// Type annotation on catch clause
try {
    // ...
} catch (error: unknown) {
    // Must narrow the type
}
```

---

## 7. Functions

### 7.1 Function Declarations

```typescript
// Named function
function add(a: number, b: number): number {
    return a + b;
}

// Function expression
const add = function(a: number, b: number): number {
    return a + b;
};

// Arrow function
const add = (a: number, b: number): number => a + b;

// Arrow function with block body
const add = (a: number, b: number): number => {
    return a + b;
};
```

### 7.2 Parameter Types

```typescript
// Optional parameters
function greet(name: string, greeting?: string): string {
    return `${greeting ?? "Hello"}, ${name}!`;
}

// Default parameters
function greet(name: string, greeting: string = "Hello"): string {
    return `${greeting}, ${name}!`;
}

// Rest parameters
function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

// Destructured parameters
function process({ name, age }: { name: string; age: number }): void {
    // ...
}
```

### 7.3 Return Types

```typescript
// Explicit return type
function add(a: number, b: number): number {
    return a + b;
}

// Inferred return type
function add(a: number, b: number) {
    return a + b; // Inferred as number
}

// void return type
function log(message: string): void {
    console.log(message);
}

// never return type
function fail(): never {
    throw new Error();
}
```

### 7.4 Function Overloads

```typescript
// Overload signatures
function process(x: string): string;
function process(x: number): number;
function process(x: string[]): string[];

// Implementation signature
function process(x: string | number | string[]): string | number | string[] {
    if (typeof x === "string") return x.toUpperCase();
    if (typeof x === "number") return x * 2;
    return x.map(s => s.toUpperCase());
}
```

### 7.5 `this` Parameter

```typescript
interface User {
    name: string;
    greet(this: User): string;
}

const user: User = {
    name: "Alice",
    greet() {
        return `Hello, ${this.name}!`;
    }
};

// Explicitly typed this
function fn(this: SomeType, x: number) {
    // this is SomeType
}
```

### 7.6 Async Functions

```typescript
async function fetchData(): Promise<Data> {
    const response = await fetch(url);
    return response.json();
}

// Arrow async function
const fetchData = async (): Promise<Data> => {
    const response = await fetch(url);
    return response.json();
};
```

### 7.7 Generator Functions

```typescript
function* numberGenerator(): Generator<number, void, unknown> {
    yield 1;
    yield 2;
    yield 3;
}

// Async generator
async function* asyncGenerator(): AsyncGenerator<number> {
    yield await fetchNumber();
}
```

---

## 8. Classes

### 8.1 Class Declarations

```typescript
class Person {
    // Fields
    name: string;
    private age: number;
    protected id: string;
    readonly birthDate: Date;

    // Static members
    static count: number = 0;

    // Constructor
    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
        this.id = crypto.randomUUID();
        this.birthDate = new Date();
        Person.count++;
    }

    // Methods
    greet(): string {
        return `Hello, ${this.name}!`;
    }

    // Getters and setters
    get displayName(): string {
        return this.name.toUpperCase();
    }

    set displayName(value: string) {
        this.name = value.toLowerCase();
    }

    // Static methods
    static getCount(): number {
        return Person.count;
    }
}
```

### 8.2 Access Modifiers

```typescript
class Example {
    public publicField: string;      // Accessible everywhere (default)
    private privateField: string;    // Only accessible within class
    protected protectedField: string; // Accessible in class and subclasses
    readonly readonlyField: string;  // Cannot be reassigned after init

    // ECMAScript private fields (truly private)
    #reallyPrivate: string;
}
```

### 8.3 Parameter Properties

```typescript
class Person {
    // Shorthand for declaring and initializing properties
    constructor(
        public name: string,
        private age: number,
        readonly id: string
    ) {}
}
```

### 8.4 Inheritance

```typescript
class Animal {
    constructor(public name: string) {}

    speak(): void {
        console.log(`${this.name} makes a sound.`);
    }
}

class Dog extends Animal {
    constructor(name: string, public breed: string) {
        super(name);
    }

    override speak(): void {
        console.log(`${this.name} barks.`);
    }
}
```

### 8.5 Abstract Classes

```typescript
abstract class Shape {
    abstract area(): number;
    abstract perimeter(): number;

    describe(): string {
        return `Area: ${this.area()}, Perimeter: ${this.perimeter()}`;
    }
}

class Circle extends Shape {
    constructor(public radius: number) {
        super();
    }

    area(): number {
        return Math.PI * this.radius ** 2;
    }

    perimeter(): number {
        return 2 * Math.PI * this.radius;
    }
}
```

### 8.6 Implementing Interfaces

```typescript
interface Printable {
    print(): void;
}

interface Serializable {
    serialize(): string;
}

class Document implements Printable, Serializable {
    print(): void {
        console.log("Printing...");
    }

    serialize(): string {
        return JSON.stringify(this);
    }
}
```

### 8.7 Static Blocks

```typescript
class Config {
    static settings: Map<string, string>;

    static {
        this.settings = new Map();
        this.settings.set("version", "1.0.0");
        this.settings.set("env", process.env.NODE_ENV ?? "development");
    }
}
```

### 8.8 Auto-Accessors

```typescript
class Person {
    accessor name: string = "Anonymous";
}

// Equivalent to:
class Person {
    #name: string = "Anonymous";

    get name() { return this.#name; }
    set name(value: string) { this.#name = value; }
}
```

---

## 9. Interfaces

### 9.1 Interface Declarations

```typescript
interface Person {
    name: string;
    age: number;
    email?: string;              // Optional
    readonly id: string;         // Read-only
    greet(): string;             // Method
    greet(greeting: string): string;  // Overloaded method
}
```

### 9.2 Extending Interfaces

```typescript
interface Named {
    name: string;
}

interface Person extends Named {
    age: number;
}

// Multiple inheritance
interface Employee extends Person, Serializable {
    employeeId: string;
}
```

### 9.3 Interface Merging

Interfaces with the same name merge:

```typescript
interface Box {
    height: number;
    width: number;
}

interface Box {
    depth: number;
}

// Result: { height: number; width: number; depth: number; }
```

### 9.4 Interfaces vs Type Aliases

| Feature | Interface | Type Alias |
|---------|-----------|------------|
| Extend/Inherit | `extends` | `&` (intersection) |
| Merging | Yes | No |
| Implements | Yes | Yes |
| Computed properties | No | Yes |
| Union types | No | Yes |
| Tuple types | No | Yes |
| Mapped types | No | Yes |

---

## 10. Enums

### 10.1 Numeric Enums

```typescript
enum Direction {
    North,  // 0
    East,   // 1
    South,  // 2
    West    // 3
}

enum Status {
    Pending = 1,
    Approved = 2,
    Rejected = 3
}

let dir: Direction = Direction.North;
let name: string = Direction[0];  // "North" (reverse mapping)
```

### 10.2 String Enums

```typescript
enum Direction {
    North = "NORTH",
    East = "EAST",
    South = "SOUTH",
    West = "WEST"
}

// No reverse mapping for string enums
let dir: Direction = Direction.North;
```

### 10.3 Heterogeneous Enums

```typescript
enum Mixed {
    No = 0,
    Yes = "YES"
}
// Not recommended
```

### 10.4 Const Enums

```typescript
const enum Direction {
    North,
    East,
    South,
    West
}

let dir = Direction.North;  // Compiled to: let dir = 0;
// Completely inlined at compile time
```

### 10.5 Computed and Constant Members

```typescript
enum FileAccess {
    // Constant members
    None,
    Read = 1 << 1,
    Write = 1 << 2,
    ReadWrite = Read | Write,

    // Computed member
    G = "123".length
}
```

### 10.6 Enums as Types

```typescript
enum ShapeKind {
    Circle,
    Square
}

interface Circle {
    kind: ShapeKind.Circle;
    radius: number;
}

interface Square {
    kind: ShapeKind.Square;
    sideLength: number;
}
```

---

## 11. Generics

### 11.1 Generic Functions

```typescript
function identity<T>(arg: T): T {
    return arg;
}

// Call with explicit type
let output1 = identity<string>("hello");

// Type argument inference
let output2 = identity("hello");  // T inferred as string
```

### 11.2 Generic Interfaces

```typescript
interface Container<T> {
    value: T;
    getValue(): T;
}

interface Map<K, V> {
    get(key: K): V | undefined;
    set(key: K, value: V): void;
}
```

### 11.3 Generic Classes

```typescript
class Box<T> {
    private contents: T;

    constructor(value: T) {
        this.contents = value;
    }

    getValue(): T {
        return this.contents;
    }
}

let numberBox = new Box<number>(42);
let stringBox = new Box("hello");  // Inferred
```

### 11.4 Generic Constraints

```typescript
interface HasLength {
    length: number;
}

function logLength<T extends HasLength>(arg: T): T {
    console.log(arg.length);
    return arg;
}

logLength("hello");     // OK
logLength([1, 2, 3]);   // OK
logLength(123);         // Error: number has no length
```

### 11.5 Using Type Parameters in Constraints

```typescript
function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

let person = { name: "Alice", age: 30 };
let name = getProperty(person, "name");  // string
let age = getProperty(person, "age");    // number
```

### 11.6 Generic Default Types

```typescript
interface Container<T = string> {
    value: T;
}

let c1: Container = { value: "hello" };        // T = string
let c2: Container<number> = { value: 42 };     // T = number
```

### 11.7 Variance Annotations

```typescript
type Getter<out T> = () => T;       // Covariant in T
type Setter<in T> = (value: T) => void;  // Contravariant in T
type Property<in out T> = {         // Invariant in T
    get(): T;
    set(value: T): void;
};
```

---

## 12. Type Operators and Advanced Types

### 12.1 `keyof` Type Operator

```typescript
type Point = { x: number; y: number };
type PointKeys = keyof Point;  // "x" | "y"

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}
```

### 12.2 `typeof` Type Operator

```typescript
let person = { name: "Alice", age: 30 };
type Person = typeof person;  // { name: string; age: number }

function f() { return { x: 10, y: 20 }; }
type FReturn = ReturnType<typeof f>;  // { x: number; y: number }
```

### 12.3 Indexed Access Types

```typescript
type Person = { name: string; age: number; address: { city: string } };

type Name = Person["name"];              // string
type NameOrAge = Person["name" | "age"]; // string | number
type City = Person["address"]["city"];   // string

// With arrays
type StringArray = string[];
type StringElement = StringArray[number];  // string

// With tuples
type Tuple = [string, number, boolean];
type First = Tuple[0];  // string
```

### 12.4 Conditional Types

```typescript
// Basic conditional type
type IsString<T> = T extends string ? true : false;

type A = IsString<string>;   // true
type B = IsString<number>;   // false

// Distributive conditional types
type ToArray<T> = T extends any ? T[] : never;
type NumOrStrArray = ToArray<number | string>;  // number[] | string[]

// Non-distributive (wrapped in tuple)
type ToArrayND<T> = [T] extends [any] ? T[] : never;
type Combined = ToArrayND<number | string>;  // (number | string)[]

// Inferring within conditional types
type Flatten<T> = T extends Array<infer U> ? U : T;
type Str = Flatten<string[]>;    // string
type Num = Flatten<number>;      // number

// Multiple infer clauses
type Unpacked<T> =
    T extends (infer U)[] ? U :
    T extends (...args: any[]) => infer U ? U :
    T extends Promise<infer U> ? U :
    T;
```

### 12.5 Mapped Types

```typescript
// Basic mapped type
type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

type Partial<T> = {
    [P in keyof T]?: T[P];
};

// Mapping modifiers
type Mutable<T> = {
    -readonly [P in keyof T]: T[P];  // Remove readonly
};

type Required<T> = {
    [P in keyof T]-?: T[P];  // Remove optional
};

// Key remapping via `as`
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

// Filtering keys
type OnlyStrings<T> = {
    [K in keyof T as T[K] extends string ? K : never]: T[K];
};
```

### 12.6 Template Literal Types

```typescript
type Greeting = `Hello, ${string}!`;
type EmailDomain = `${string}@${string}.${string}`;

// Combinations
type Lang = "en" | "fr" | "de";
type ContentKey = "title" | "body";
type LocalizedKey = `${Lang}_${ContentKey}`;
// "en_title" | "en_body" | "fr_title" | "fr_body" | "de_title" | "de_body"

// With other type operators
type PropEventSource<T> = {
    on<K extends string & keyof T>(
        eventName: `${K}Changed`,
        callback: (newValue: T[K]) => void
    ): void;
};
```

### 12.7 Intrinsic String Manipulation Types

```typescript
type Upper = Uppercase<"hello">;      // "HELLO"
type Lower = Lowercase<"HELLO">;      // "hello"
type Cap = Capitalize<"hello">;       // "Hello"
type Uncap = Uncapitalize<"Hello">;   // "hello"
```

---

## 13. Modules

### 13.1 ES Modules

```typescript
// Exporting
export const pi = 3.14159;
export function square(x: number): number { return x * x; }
export class Calculator { }
export type Point = { x: number; y: number };
export interface Named { name: string; }

// Default export
export default class MyClass { }

// Re-exports
export { something } from "./other-module";
export { something as alias } from "./other-module";
export * from "./other-module";
export * as utils from "./utils";
```

```typescript
// Importing
import { pi, square } from "./math";
import { square as sq } from "./math";
import * as math from "./math";
import Calculator from "./calculator";  // Default import
import type { Point } from "./types";   // Type-only import

// Side-effect import
import "./polyfills";

// Dynamic import
const module = await import("./module");
```

### 13.2 Import Types

```typescript
// Import types inline
let point: import("./types").Point;

// Import type with typeof
let value: typeof import("./config").default;
```

### 13.3 Module Resolution

TypeScript supports multiple module resolution strategies:

- **Classic**: Legacy resolution mode
- **Node**: Mimics Node.js resolution algorithm
- **Node16/NodeNext**: ESM and CJS interop in Node.js
- **Bundler**: For bundler environments

```json
{
    "compilerOptions": {
        "moduleResolution": "node16"
    }
}
```

### 13.4 CommonJS Interop

```typescript
// CommonJS-style exports
export = myFunction;

// CommonJS-style imports
import myFunction = require("./module");

// esModuleInterop
import fs from "fs";  // Instead of import * as fs from "fs"
```

---

## 14. Namespaces

### 14.1 Namespace Declaration

```typescript
namespace Validation {
    export interface StringValidator {
        isAcceptable(s: string): boolean;
    }

    export class ZipCodeValidator implements StringValidator {
        isAcceptable(s: string): boolean {
            return s.length === 5 && /^\d+$/.test(s);
        }
    }

    // Not exported - internal
    const numberRegexp = /^\d+$/;
}

// Usage
let validator = new Validation.ZipCodeValidator();
```

### 14.2 Splitting Across Files

```typescript
// validation.ts
namespace Validation {
    export interface StringValidator { /* ... */ }
}

// zipCodeValidator.ts
/// <reference path="validation.ts" />
namespace Validation {
    export class ZipCodeValidator implements StringValidator { /* ... */ }
}
```

### 14.3 Namespace vs Modules

- **Modules** (ES Modules): Recommended for most cases, file-based
- **Namespaces**: Internal modules, global scope organization
- Namespaces can span multiple files
- Modules provide better encapsulation

---

## 15. Decorators

### 15.1 Stage 3 Decorators (TypeScript 5.0+)

```typescript
// Class decorator
function logged<T extends new (...args: any[]) => any>(
    target: T,
    context: ClassDecoratorContext
) {
    return class extends target {
        constructor(...args: any[]) {
            super(...args);
            console.log(`Creating instance of ${context.name}`);
        }
    };
}

@logged
class MyClass { }
```

### 15.2 Method Decorators

```typescript
function bound<T extends Function>(
    target: T,
    context: ClassMethodDecoratorContext
) {
    const methodName = String(context.name);

    context.addInitializer(function (this: any) {
        this[methodName] = this[methodName].bind(this);
    });

    return target;
}

class MyClass {
    @bound
    handleClick() {
        console.log(this);
    }
}
```

### 15.3 Field Decorators

```typescript
function observable<T>(
    target: undefined,
    context: ClassFieldDecoratorContext
) {
    return function (this: any, initialValue: T): T {
        // Initialize with tracking
        return initialValue;
    };
}

class Store {
    @observable
    count = 0;
}
```

### 15.4 Accessor Decorators

```typescript
function logged(
    target: ClassAccessorDecoratorTarget<MyClass, number>,
    context: ClassAccessorDecoratorContext<MyClass, number>
) {
    return {
        get(this: MyClass) {
            console.log("Getting value");
            return target.get.call(this);
        },
        set(this: MyClass, value: number) {
            console.log("Setting value");
            target.set.call(this, value);
        }
    };
}

class MyClass {
    @logged
    accessor x = 0;
}
```

### 15.5 Decorator Metadata

```typescript
import "reflect-metadata";

function format(formatString: string) {
    return function (target: any, context: ClassFieldDecoratorContext) {
        Reflect.defineMetadata("format", formatString, target, context.name);
    };
}

class Person {
    @format("Hello, %s")
    name: string;
}
```

### 15.6 Legacy Decorators (experimentalDecorators)

```typescript
// Legacy class decorator
function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

// Legacy method decorator
function enumerable(value: boolean) {
    return function (
        target: any,
        propertyKey: string,
        descriptor: PropertyDescriptor
    ) {
        descriptor.enumerable = value;
    };
}

// Legacy parameter decorator (not in Stage 3)
function required(target: Object, propertyKey: string, parameterIndex: number) {
    // ...
}
```

---

## 16. JSX/TSX

### 16.1 JSX Configuration

```json
{
    "compilerOptions": {
        "jsx": "react",           // Emit React.createElement
        // "jsx": "react-jsx",    // Emit _jsx (React 17+)
        // "jsx": "react-native", // Preserve JSX
        // "jsx": "preserve",     // Preserve JSX for other tools
        "jsxFactory": "React.createElement",
        "jsxFragmentFactory": "React.Fragment",
        "jsxImportSource": "react"
    }
}
```

### 16.2 JSX Types

```typescript
// Intrinsic elements (lowercase)
declare namespace JSX {
    interface IntrinsicElements {
        div: React.HTMLAttributes<HTMLDivElement>;
        span: React.HTMLAttributes<HTMLSpanElement>;
        // ...
    }
}

// Value-based elements (uppercase)
interface MyComponentProps {
    name: string;
    age?: number;
}

function MyComponent(props: MyComponentProps): JSX.Element {
    return <div>Hello, {props.name}!</div>;
}
```

### 16.3 JSX Expressions

```typescript
// Element creation
let element = <div className="container">Hello</div>;

// With expressions
let greeting = <h1>Hello, {name}!</h1>;

// Spread attributes
let props = { className: "btn", onClick: handleClick };
let button = <button {...props}>Click me</button>;

// Fragments
let fragment = (
    <>
        <div>First</div>
        <div>Second</div>
    </>
);

// Conditional rendering
let content = condition ? <Truthy /> : <Falsy />;
let maybeContent = condition && <Content />;
```

### 16.4 Generic Components

```typescript
interface ListProps<T> {
    items: T[];
    renderItem: (item: T) => JSX.Element;
}

function List<T>(props: ListProps<T>): JSX.Element {
    return (
        <ul>
            {props.items.map(props.renderItem)}
        </ul>
    );
}

// Usage
<List items={[1, 2, 3]} renderItem={(n) => <li>{n}</li>} />
```

---

## 17. Declaration Files

### 17.1 Declaration File Structure

Declaration files (`.d.ts`) contain only type information:

```typescript
// types.d.ts
declare module "my-module" {
    export function doSomething(): void;
    export const version: string;
    export interface Options {
        verbose?: boolean;
    }
}
```

### 17.2 Ambient Declarations

```typescript
// Declare global variables
declare const VERSION: string;
declare function greet(name: string): void;

// Declare global interfaces
declare interface Window {
    myCustomProperty: string;
}

// Declare global module
declare module "*.css" {
    const content: { [className: string]: string };
    export default content;
}

declare module "*.png" {
    const value: string;
    export default value;
}
```

### 17.3 Module Augmentation

```typescript
// Extend existing module
import { MyClass } from "my-library";

declare module "my-library" {
    interface MyClass {
        newMethod(): void;
    }
}
```

### 17.4 Global Augmentation

```typescript
export {};  // Make this a module

declare global {
    interface Array<T> {
        customMethod(): T[];
    }

    namespace NodeJS {
        interface ProcessEnv {
            MY_VAR: string;
        }
    }
}
```

### 17.5 Triple-Slash Directives

```typescript
/// <reference path="./other.d.ts" />
/// <reference types="node" />
/// <reference lib="es2020" />
/// <reference no-default-lib="true" />
```

---

## 18. Type Compatibility

### 18.1 Structural Typing

TypeScript uses structural type compatibility:

```typescript
interface Named {
    name: string;
}

class Person {
    name: string;
    age: number;
}

let p: Named;
p = new Person();  // OK - Person has 'name' property
```

### 18.2 Subtype vs Assignment

```typescript
// Subtype: source is a proper subtype
// Assignment: looser, allows some unsound cases

interface Animal { name: string; }
interface Dog extends Animal { breed: string; }

let animal: Animal;
let dog: Dog = { name: "Rex", breed: "German Shepherd" };

animal = dog;  // OK - Dog is subtype of Animal
```

### 18.3 Function Compatibility

```typescript
// Parameters: contravariant (with strictFunctionTypes)
// Return type: covariant

type Handler = (a: string) => void;
let handler: Handler;

handler = (a: string) => {};        // OK
handler = (a: string | number) => {};  // OK - accepts more
handler = () => {};                  // OK - ignores extra params
```

### 18.4 Class Compatibility

```typescript
// Only instance members are compared (not static or constructor)

class Animal {
    feet: number;
    constructor(name: string, numFeet: number) { }
}

class Size {
    feet: number;
    constructor(meters: number) { }
}

let a: Animal;
let s: Size;

a = s;  // OK - same structure
s = a;  // OK
```

### 18.5 Private and Protected Members

```typescript
class Animal {
    private name: string;
    constructor(name: string) { this.name = name; }
}

class Dog extends Animal {
    constructor(name: string) { super(name); }
}

class Employee {
    private name: string;
    constructor(name: string) { this.name = name; }
}

let animal: Animal = new Dog("Rex");  // OK - same private origin
let employee: Animal = new Employee("Alice");  // Error!
```

---

## 19. Type Inference

### 19.1 Basic Inference

```typescript
// Variable inference
let x = 3;           // number
let y = "hello";     // string
let z = [1, 2, 3];   // number[]

// Return type inference
function add(a: number, b: number) {
    return a + b;    // Return type inferred as number
}

// Contextual typing
window.onmousedown = function(event) {
    // event is inferred as MouseEvent
    console.log(event.button);
};
```

### 19.2 Best Common Type

```typescript
let arr = [0, 1, null];  // (number | null)[]

class Animal { }
class Dog extends Animal { }
class Cat extends Animal { }

let pets = [new Dog(), new Cat()];  // (Dog | Cat)[]
let animals: Animal[] = [new Dog(), new Cat()];  // Animal[]
```

### 19.3 Generic Inference

```typescript
function identity<T>(arg: T): T {
    return arg;
}

let output = identity("hello");  // T inferred as string

function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}

let result = map([1, 2, 3], x => x.toString());  // string[]
```

### 19.4 Bidirectional Inference

```typescript
// Type flows from context to expression
const handler: (e: MouseEvent) => void = (e) => {
    console.log(e.button);  // e is MouseEvent
};

// Type flows from expression to context
let nums = [1, 2, 3].map(x => x * 2);  // number[]
```

### 19.5 Widening and Narrowing

```typescript
// Widening: literals become their base type
let x = "hello";           // string (widened)
const y = "hello";         // "hello" (literal type)
let z = "hello" as const;  // "hello" (const assertion)

// Narrowing: types become more specific
function process(x: string | number) {
    if (typeof x === "string") {
        // x is string here
    }
}
```

---

## 20. Type Narrowing

### 20.1 Control Flow Analysis

```typescript
function example(x: string | number | null) {
    if (x === null) {
        // x is null
    } else if (typeof x === "string") {
        // x is string
    } else {
        // x is number
    }
}
```

### 20.2 Type Guards

#### `typeof` Guards

```typescript
function padLeft(value: string, padding: string | number) {
    if (typeof padding === "number") {
        return " ".repeat(padding) + value;
    }
    return padding + value;
}
```

#### `instanceof` Guards

```typescript
function move(date: Date | string) {
    if (date instanceof Date) {
        return date.toISOString();
    }
    return new Date(date).toISOString();
}
```

#### `in` Operator Guards

```typescript
interface Fish { swim(): void; }
interface Bird { fly(): void; }

function move(animal: Fish | Bird) {
    if ("swim" in animal) {
        animal.swim();
    } else {
        animal.fly();
    }
}
```

### 20.3 User-Defined Type Guards

```typescript
function isFish(pet: Fish | Bird): pet is Fish {
    return (pet as Fish).swim !== undefined;
}

function move(pet: Fish | Bird) {
    if (isFish(pet)) {
        pet.swim();  // pet is Fish
    } else {
        pet.fly();   // pet is Bird
    }
}
```

### 20.4 Assertion Functions

```typescript
function assert(condition: unknown, message?: string): asserts condition {
    if (!condition) {
        throw new Error(message ?? "Assertion failed");
    }
}

function assertIsString(val: unknown): asserts val is string {
    if (typeof val !== "string") {
        throw new Error("Not a string!");
    }
}

function process(value: unknown) {
    assertIsString(value);
    // value is string from here on
    console.log(value.toUpperCase());
}
```

### 20.5 Discriminated Unions

```typescript
interface Circle {
    kind: "circle";
    radius: number;
}

interface Square {
    kind: "square";
    sideLength: number;
}

interface Triangle {
    kind: "triangle";
    base: number;
    height: number;
}

type Shape = Circle | Square | Triangle;

function area(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "square":
            return shape.sideLength ** 2;
        case "triangle":
            return (shape.base * shape.height) / 2;
    }
}
```

### 20.6 Exhaustiveness Checking

```typescript
function assertNever(x: never): never {
    throw new Error("Unexpected value: " + x);
}

function area(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "square":
            return shape.sideLength ** 2;
        case "triangle":
            return (shape.base * shape.height) / 2;
        default:
            return assertNever(shape);  // Error if any case missed
    }
}
```

---

## 21. Utility Types

### 21.1 Partial\<T\>

Makes all properties optional:

```typescript
type Partial<T> = {
    [P in keyof T]?: T[P];
};

interface Todo {
    title: string;
    description: string;
}

type PartialTodo = Partial<Todo>;
// { title?: string; description?: string; }
```

### 21.2 Required\<T\>

Makes all properties required:

```typescript
type Required<T> = {
    [P in keyof T]-?: T[P];
};
```

### 21.3 Readonly\<T\>

Makes all properties readonly:

```typescript
type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};
```

### 21.4 Record\<K, T\>

Creates an object type with keys K and values T:

```typescript
type Record<K extends keyof any, T> = {
    [P in K]: T;
};

type PageInfo = { title: string };
type Pages = Record<"home" | "about" | "contact", PageInfo>;
```

### 21.5 Pick\<T, K\>

Picks specific properties from a type:

```typescript
type Pick<T, K extends keyof T> = {
    [P in K]: T[P];
};

type TodoPreview = Pick<Todo, "title" | "completed">;
```

### 21.6 Omit\<T, K\>

Omits specific properties from a type:

```typescript
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

type TodoWithoutDescription = Omit<Todo, "description">;
```

### 21.7 Exclude\<T, U\>

Excludes types from a union:

```typescript
type Exclude<T, U> = T extends U ? never : T;

type T = Exclude<"a" | "b" | "c", "a">;  // "b" | "c"
```

### 21.8 Extract\<T, U\>

Extracts types from a union:

```typescript
type Extract<T, U> = T extends U ? T : never;

type T = Extract<"a" | "b" | "c", "a" | "f">;  // "a"
```

### 21.9 NonNullable\<T\>

Removes null and undefined:

```typescript
type NonNullable<T> = T & {};
// or: T extends null | undefined ? never : T

type T = NonNullable<string | null | undefined>;  // string
```

### 21.10 ReturnType\<T\>

Gets the return type of a function:

```typescript
type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

function f() { return { x: 10, y: 3 }; }
type FReturn = ReturnType<typeof f>;  // { x: number; y: number }
```

### 21.11 Parameters\<T\>

Gets parameter types as a tuple:

```typescript
type Parameters<T extends (...args: any) => any> =
    T extends (...args: infer P) => any ? P : never;

function f(a: string, b: number) { }
type FParams = Parameters<typeof f>;  // [string, number]
```

### 21.12 ConstructorParameters\<T\>

Gets constructor parameter types:

```typescript
type ConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;

class Person {
    constructor(name: string, age: number) { }
}
type PersonParams = ConstructorParameters<typeof Person>;  // [string, number]
```

### 21.13 InstanceType\<T\>

Gets the instance type of a constructor:

```typescript
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

class Person { name: string; }
type PersonInstance = InstanceType<typeof Person>;  // Person
```

### 21.14 ThisParameterType\<T\>

Gets the type of `this` parameter:

```typescript
type ThisParameterType<T> =
    T extends (this: infer U, ...args: any[]) => any ? U : unknown;

function toHex(this: Number) { return this.toString(16); }
type ThisType = ThisParameterType<typeof toHex>;  // Number
```

### 21.15 OmitThisParameter\<T\>

Removes `this` parameter:

```typescript
type OmitThisParameter<T> =
    unknown extends ThisParameterType<T>
        ? T
        : T extends (...args: infer A) => infer R
            ? (...args: A) => R
            : T;
```

### 21.16 Awaited\<T\>

Unwraps Promise types:

```typescript
type Awaited<T> =
    T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F): any } ?
        F extends ((value: infer V) => any) ? Awaited<V> : never :
    T;

type T = Awaited<Promise<Promise<string>>>;  // string
```

### 21.17 NoInfer\<T\>

Blocks inference at a position:

```typescript
declare function fn<T>(arg: NoInfer<T>, arg2: T): T;

fn("hello", "world");  // T is inferred from arg2 only
```

---

## 22. Compiler Options

### 22.1 Type Checking Options

```json
{
    "compilerOptions": {
        // Strict mode (enables all strict options)
        "strict": true,

        // Individual strict options
        "noImplicitAny": true,
        "strictNullChecks": true,
        "strictFunctionTypes": true,
        "strictBindCallApply": true,
        "strictPropertyInitialization": true,
        "noImplicitThis": true,
        "useUnknownInCatchVariables": true,
        "alwaysStrict": true,

        // Additional checks
        "noUnusedLocals": true,
        "noUnusedParameters": true,
        "exactOptionalPropertyTypes": true,
        "noImplicitReturns": true,
        "noFallthroughCasesInSwitch": true,
        "noUncheckedIndexedAccess": true,
        "noImplicitOverride": true,
        "noPropertyAccessFromIndexSignature": true
    }
}
```

### 22.2 Module Options

```json
{
    "compilerOptions": {
        "module": "esnext",
        "moduleResolution": "bundler",
        "baseUrl": "./src",
        "paths": {
            "@/*": ["*"],
            "@utils/*": ["utils/*"]
        },
        "rootDirs": ["src", "generated"],
        "typeRoots": ["./typings", "./node_modules/@types"],
        "types": ["node", "jest"],
        "resolveJsonModule": true,
        "esModuleInterop": true,
        "allowSyntheticDefaultImports": true
    }
}
```

### 22.3 Emit Options

```json
{
    "compilerOptions": {
        "target": "ES2022",
        "lib": ["ES2022", "DOM"],
        "outDir": "./dist",
        "rootDir": "./src",
        "declaration": true,
        "declarationDir": "./types",
        "declarationMap": true,
        "sourceMap": true,
        "inlineSources": true,
        "removeComments": false,
        "noEmit": false,
        "noEmitOnError": true,
        "importHelpers": true,
        "downlevelIteration": true,
        "emitDecoratorMetadata": true,
        "preserveConstEnums": false,
        "verbatimModuleSyntax": true
    }
}
```

### 22.4 JavaScript Support

```json
{
    "compilerOptions": {
        "allowJs": true,
        "checkJs": true,
        "maxNodeModuleJsDepth": 0
    }
}
```

### 22.5 Project Configuration

```json
{
    "compilerOptions": {
        "composite": true,
        "incremental": true,
        "tsBuildInfoFile": "./.tsbuildinfo"
    },
    "include": ["src/**/*"],
    "exclude": ["node_modules", "dist"],
    "references": [
        { "path": "../common" }
    ]
}
```

---

## Appendix A: TypeFlags Reference

The internal representation of types uses flags defined in `src/compiler/types.ts`:

| Flag | Description |
|------|-------------|
| `Any` | The `any` type |
| `Unknown` | The `unknown` type |
| `String` | Primitive `string` type |
| `Number` | Primitive `number` type |
| `Boolean` | Primitive `boolean` type |
| `BigInt` | Primitive `bigint` type |
| `ESSymbol` | Primitive `symbol` type |
| `Void` | The `void` type |
| `Undefined` | The `undefined` type |
| `Null` | The `null` type |
| `Never` | The `never` type |
| `StringLiteral` | String literal type |
| `NumberLiteral` | Number literal type |
| `BooleanLiteral` | Boolean literal type |
| `BigIntLiteral` | BigInt literal type |
| `EnumLiteral` | Enum literal type |
| `UniqueESSymbol` | Unique symbol type |
| `Object` | Object type |
| `Union` | Union type |
| `Intersection` | Intersection type |
| `TypeParameter` | Generic type parameter |
| `Index` | `keyof T` type |
| `IndexedAccess` | `T[K]` type |
| `Conditional` | Conditional type |
| `TemplateLiteral` | Template literal type |
| `StringMapping` | String manipulation type |

---

## Appendix B: SyntaxKind Reference

TypeScript's AST node types are defined by the `SyntaxKind` enum:

### Token Categories

- **Trivia**: Comments, whitespace, newlines
- **Literals**: Numeric, string, regex, template
- **Punctuators**: Braces, brackets, operators
- **Keywords**: Reserved words and contextual keywords

### Node Categories

- **Type Nodes**: TypeReference, UnionType, ConditionalType, etc.
- **Declaration Nodes**: FunctionDeclaration, ClassDeclaration, etc.
- **Expression Nodes**: CallExpression, BinaryExpression, etc.
- **Statement Nodes**: IfStatement, ForStatement, etc.
- **JSX Nodes**: JsxElement, JsxAttribute, etc.
- **JSDoc Nodes**: JSDoc, JSDocTag, etc.

---

## Appendix C: Compilation Pipeline

```
Source Code (TypeScript)
         │
         ▼
┌─────────────────┐
│     Scanner     │  → Token Stream
│  (scanner.ts)   │     - Tokenization
└─────────────────┘     - Keyword recognition
         │
         ▼
┌─────────────────┐
│     Parser      │  → Abstract Syntax Tree (AST)
│   (parser.ts)   │     - Syntax analysis
└─────────────────┘     - Error recovery
         │
         ▼
┌─────────────────┐
│     Binder      │  → Symbol Table
│   (binder.ts)   │     - Name resolution
└─────────────────┘     - Scope analysis
         │
         ▼
┌─────────────────┐
│  Type Checker   │  → Types + Diagnostics
│  (checker.ts)   │     - Type inference
│   (~54K lines)  │     - Type checking
└─────────────────┘     - Error reporting
         │
         ▼
┌─────────────────┐
│    Emitter      │  → JavaScript / Declaration Files
│  (emitter.ts)   │     - Code generation
└─────────────────┘     - Type erasure
         │
         ▼
    Output Files
```

---

## Appendix D: Further Reading

### Official Resources
- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [TypeScript Documentation](https://www.typescriptlang.org/docs/)
- [TypeScript GitHub Repository](https://github.com/microsoft/TypeScript)
- [TypeScript Playground](https://www.typescriptlang.org/play)

### Books and Guides
- [TypeScript Deep Dive](https://basarat.gitbook.io/typescript/)
- [Programming TypeScript](https://www.oreilly.com/library/view/programming-typescript/9781492037644/)
- [Effective TypeScript](https://effectivetypescript.com/)

### Community Resources
- [DefinitelyTyped](https://github.com/DefinitelyTyped/DefinitelyTyped) - Type definitions
- [TypeScript ESLint](https://typescript-eslint.io/) - Linting rules
- [Total TypeScript](https://www.totaltypescript.com/) - Advanced tutorials

---

*This specification is based on the TypeScript compiler source code and official documentation. For the most up-to-date information, consult the official TypeScript documentation.*
