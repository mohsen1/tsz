# Emitter Module Deep Dive

The emitter transforms the AST into JavaScript code. It supports multiple module formats, ES5 downleveling, and source map generation through a two-phase architecture.

## File Structure

```
src/emitter/
├── mod.rs               (71 KB)  Main dispatcher and source file logic
├── declarations.rs      (17.9 KB) Classes, enums, modules, interfaces
├── expressions.rs       (7.8 KB)  Binary/unary ops, calls, literals
├── statements.rs        (14.8 KB) Control flow, variable declarations
├── functions.rs         (6.6 KB)  Arrow functions, function expressions
├── types.rs             (7.3 KB)  Type references (for .d.ts)
├── jsx.rs               (3.4 KB)  JSX elements and attributes
├── helpers.rs                     Source mapping utilities
├── es5_helpers.rs       (1,098)   ES5 downlevel transforms
├── es5_bindings.rs      (908)     Variable/destructuring ES5
├── module_emission.rs   (469)     Import/export handling
├── module_wrapper.rs    (4.5 KB)  AMD/UMD/System wrappers
└── special_expressions.rs         Decorators, spread, etc.
```

## Core Architecture

### Printer Struct (`mod.rs`)

```rust
pub struct Printer<'a> {
    arena: &'a NodeArena,              // AST access
    writer: SourceWriter,              // Output + source maps
    ctx: EmitContext,                  // Options and state
    transforms: TransformContext,      // Phase 2 directives
    emit_recursion_depth: u32,         // Limit: 1000
    // ... more fields
}
```

### Two-Phase Architecture

**Phase 1**: Parse and transform AST (handled by parser/transforms)
**Phase 2**: Emit directives guide code generation

```rust
pub enum EmitDirective {
    ES5Class,
    ES5ClassExpression,
    ES5Namespace,
    ES5Enum,
    ES5ArrowFunction { captures_this: bool },
    ES5AsyncFunction,
    ES5ForOf,
    ES5ObjectLiteral,
    ES5VariableDeclarationList,
    ES5FunctionParameters,
    ES5TemplateLiteral,
    CommonJSExport,
    CommonJSExportDefaultExpr,
    ModuleWrapper { format: ModuleFormat, dependencies: Vec<String> },
    Chain(Vec<EmitDirective>),
}
```

### Node Dispatch (`mod.rs`)

```rust
fn emit_node(&mut self, node: NodeIndex) {
    // 1. Check recursion depth
    if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
        self.writer.write("/* emit recursion limit exceeded */");
        return;
    }
    self.emit_recursion_depth += 1;

    // 2. Check for transforms
    if self.kind_may_have_transform(node.kind) {
        if let Some(directive) = self.transforms.get_directive(node) {
            self.apply_transform(node, directive);
            self.emit_recursion_depth -= 1;
            return;
        }
    }

    // 3. Emit by node kind
    self.emit_node_by_kind(node);
    self.emit_recursion_depth -= 1;
}

fn emit_node_by_kind(&mut self, node: NodeIndex) {
    match self.arena.get(node).kind {
        // Declarations
        FunctionDeclaration => self.emit_function_declaration(node),
        ClassDeclaration => self.emit_class_declaration(node),
        VariableStatement => self.emit_variable_statement(node),

        // Statements
        Block => self.emit_block(node),
        IfStatement => self.emit_if_statement(node),
        WhileStatement => self.emit_while_statement(node),
        ForStatement => self.emit_for_statement(node),
        ReturnStatement => self.emit_return_statement(node),

        // Expressions
        BinaryExpression => self.emit_binary_expression(node),
        CallExpression => self.emit_call_expression(node),
        PropertyAccessExpression => self.emit_property_access(node),

        // ... 100+ cases
    }
}
```

## Statement Emission (`statements.rs`)

### Variable Statements

```rust
fn emit_variable_statement(&mut self, node: NodeIndex) {
    let data = self.arena.variables[node.data_index()];

    // Emit keyword (var, let, const)
    match data.keyword {
        VarKeyword => self.writer.write("var "),
        LetKeyword => self.writer.write("let "),
        ConstKeyword => self.writer.write("const "),
    }

    // Emit declarations
    self.emit_variable_declaration_list(data.declarations);
    self.writer.write(";");
}
```

### Control Flow

```rust
fn emit_if_statement(&mut self, node: NodeIndex) {
    let data = self.arena.if_statements[node.data_index()];

    self.writer.write("if (");
    self.emit_node(data.expression);
    self.writer.write(") ");
    self.emit_node(data.then_statement);

    if let Some(else_stmt) = data.else_statement {
        self.writer.write(" else ");
        self.emit_node(else_stmt);
    }
}

fn emit_for_of_statement(&mut self, node: NodeIndex) {
    let data = self.arena.for_in_of[node.data_index()];

    self.writer.write("for (");
    self.emit_node(data.initializer);
    self.writer.write(" of ");
    self.emit_node(data.expression);
    self.writer.write(") ");
    self.emit_node(data.statement);
}
```

## Expression Emission (`expressions.rs`)

### Binary Expressions

```rust
fn emit_binary_expression(&mut self, node: NodeIndex) {
    let data = self.arena.binary_exprs[node.data_index()];

    self.emit_node(data.left);
    self.writer.write(" ");
    self.writer.write(get_operator_text(data.operator));
    self.writer.write(" ");
    self.emit_node(data.right);
}
```

### Call Expressions

```rust
fn emit_call_expression(&mut self, node: NodeIndex) {
    let data = self.arena.call_exprs[node.data_index()];

    self.emit_node(data.expression);

    // Type arguments (for .d.ts only)
    if let Some(type_args) = data.type_arguments {
        self.emit_type_arguments(type_args);
    }

    self.writer.write("(");
    self.emit_comma_separated(&data.arguments);
    self.writer.write(")");
}
```

### Property Access

```rust
fn emit_property_access(&mut self, node: NodeIndex) {
    let data = self.arena.access_exprs[node.data_index()];

    self.emit_node(data.expression);

    // Optional chaining
    if data.question_dot_token.is_some() {
        self.writer.write("?.");
    } else {
        self.writer.write(".");
    }

    self.emit_node(data.name);
}
```

## Declaration Emission (`declarations.rs`)

### Class Emission (ES6)

```rust
fn emit_class_es6(&mut self, node: NodeIndex) {
    let data = self.arena.classes[node.data_index()];

    self.writer.write("class ");
    if let Some(name) = data.name {
        self.emit_node(name);
    }

    // Type parameters (stripped in JS output)
    // Heritage clause
    if let Some(heritage) = data.heritage_clauses {
        self.emit_heritage_clauses(heritage);
    }

    self.writer.write(" {");
    self.writer.new_line();
    self.indent();

    for member in data.members.iter() {
        self.emit_class_member(member);
    }

    self.dedent();
    self.writer.write("}");
}
```

### Enum Emission

```rust
fn emit_enum_declaration(&mut self, node: NodeIndex) {
    let data = self.arena.enums[node.data_index()];

    // Skip const enums and declare enums (no runtime code)
    if data.is_const || data.is_ambient {
        return;
    }

    // (function (EnumName) { ... })(EnumName || (EnumName = {}));
    self.writer.write("(function (");
    self.emit_node(data.name);
    self.writer.write(") {");
    self.writer.new_line();
    self.indent();

    for (index, member) in data.members.iter().enumerate() {
        // EnumName[EnumName["MemberName"] = 0] = "MemberName";
        self.emit_enum_member(data.name, member, index);
    }

    self.dedent();
    self.writer.write("})(");
    self.emit_node(data.name);
    self.writer.write(" || (");
    self.emit_node(data.name);
    self.writer.write(" = {}));");
}
```

## Module System (`module_emission.rs`)

### CommonJS Transformation (`mod.rs`)

```rust
fn emit_source_file(&mut self, node: NodeIndex) {
    // 1. "use strict" for CommonJS
    if self.ctx.module_kind == ModuleKind::CommonJS {
        self.writer.write("\"use strict\";");
        self.writer.new_line();
    }

    // 2. __esModule marker
    if self.has_es_module_exports() {
        self.writer.write(
            "Object.defineProperty(exports, \"__esModule\", { value: true });"
        );
        self.writer.new_line();
    }

    // 3. Export initialization (exports.X = void 0;)
    for name in self.collect_export_names() {
        self.writer.write("exports.");
        self.writer.write(&name);
        self.writer.write(" = void 0;");
        self.writer.new_line();
    }

    // 4. Emit statements
    for statement in get_statements(node) {
        self.emit_statement(statement);
    }
}
```

### Import Handling

```rust
// import { foo } from "mod"  →  const { foo } = require("mod")
fn emit_import_declaration(&mut self, node: NodeIndex) {
    let data = self.arena.import_decls[node.data_index()];

    if let Some(clause) = data.import_clause {
        self.writer.write("const ");
        self.emit_import_clause(clause);
        self.writer.write(" = require(");
        self.emit_node(data.module_specifier);
        self.writer.write(");");
    } else {
        // Side-effect import: import "mod"
        self.writer.write("require(");
        self.emit_node(data.module_specifier);
        self.writer.write(");");
    }
}
```

### Export Handling

```rust
// export { foo }  →  exports.foo = foo;
fn emit_commonjs_export(&mut self, name: &str, value: NodeIndex) {
    self.writer.write("exports.");
    self.writer.write(name);
    self.writer.write(" = ");
    self.emit_node(value);
    self.writer.write(";");
}

// export default expr  →  exports.default = expr;
fn emit_commonjs_default_export(&mut self, expr: NodeIndex) {
    self.writer.write("exports.default = ");
    self.emit_node(expr);
    self.writer.write(";");
}
```

### Module Wrappers (`module_wrapper.rs`)

**AMD**:
```javascript
define(["require", "exports", "dep"], function (require, exports, dep_1) {
    "use strict";
    // ... module body
});
```

**UMD**:
```javascript
(function (factory) {
    if (typeof module === "object" && typeof module.exports === "object") {
        // CommonJS
        factory(require, exports, require("dep"));
    } else if (typeof define === "function" && define.amd) {
        // AMD
        define(["require", "exports", "dep"], factory);
    }
})(function (require, exports, dep_1) {
    // ... module body
});
```

## ES5 Transforms

### Arrow Functions (`es5_helpers.rs`)

```typescript
// Source:
const add = (a, b) => a + b;

// Target:
var add = (function (_this) {
    return function (a, b) { return a + b; };
})(this);
```

With `this` capture:
```typescript
// Source:
class Foo {
    bar = () => this.x;
}

// Target:
function Foo() {
    var _this = this;
    this.bar = function () { return _this.x; };
}
```

### Async Functions (`es5_helpers.rs`)

```typescript
// Source:
async function fetchData() {
    const result = await fetch(url);
    return result.json();
}

// Target:
function fetchData() {
    return __awaiter(this, void 0, void 0, function () {
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0: return [4 /*yield*/, fetch(url)];
                case 1:
                    result = _a.sent();
                    return [2 /*return*/, result.json()];
            }
        });
    });
}
```

### For-Of Loops (`es5_bindings.rs`)

```typescript
// Source:
for (const x of arr) {
    console.log(x);
}

// Target:
var e_1, _a;
try {
    for (var arr_1 = __values(arr), arr_1_1 = arr_1.next(); !arr_1_1.done; arr_1_1 = arr_1.next()) {
        var x = arr_1_1.value;
        console.log(x);
    }
} catch (e_1_1) { e_1 = { error: e_1_1 }; }
finally {
    try {
        if (arr_1_1 && !arr_1_1.done && (_a = arr_1.return)) _a.call(arr_1);
    } finally { if (e_1) throw e_1.error; }
}
```

### Object Literals with Computed Properties (`es5_helpers.rs`)

```typescript
// Source:
const obj = { [key]: value, get [computed]() { return x; } };

// Target:
var obj = (_a = {}, _a[key] = value, Object.defineProperty(_a, computed, {
    get: function () { return x; },
    enumerable: true,
    configurable: true
}), _a);
var _a;
```

### Class ES5 Transform (`es5_helpers.rs`)

```typescript
// Source:
class Animal {
    constructor(name) {
        this.name = name;
    }
    speak() {
        console.log(this.name);
    }
}
class Dog extends Animal {
    speak() {
        super.speak();
        console.log("Woof!");
    }
}

// Target:
var Animal = /** @class */ (function () {
    function Animal(name) {
        this.name = name;
    }
    Animal.prototype.speak = function () {
        console.log(this.name);
    };
    return Animal;
}());
var Dog = /** @class */ (function (_super) {
    __extends(Dog, _super);
    function Dog() {
        return _super !== null && _super.apply(this, arguments) || this;
    }
    Dog.prototype.speak = function () {
        _super.prototype.speak.call(this);
        console.log("Woof!");
    };
    return Dog;
}(Animal));
```

## Source Map Generation

### Integration (`helpers.rs`)

```rust
fn emit_node_with_mapping(&mut self, node: NodeIndex) {
    // Queue source position
    self.queue_source_mapping(node);

    // Emit content
    self.emit_node(node);

    // Apply pending mapping
    self.take_pending_source_pos();
}

fn queue_source_mapping(&mut self, node: NodeIndex) {
    let source_pos = self.arena.get(node).pos;
    self.pending_source_pos = Some(SourcePosition {
        line: self.compute_line(source_pos),
        column: self.compute_column(source_pos),
    });
}
```

### SourceWriter Integration (`mod.rs:195`)

```rust
impl Printer<'_> {
    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.writer.enable_source_map(output_name, source_name);
    }

    pub fn generate_source_map_json(&self) -> String {
        self.writer.generate_source_map_json()
    }
}
```

## JSX Emission (`jsx.rs`)

### JSX Elements

```typescript
// Source:
<div className="foo" onClick={handler}>
    {children}
    <span>text</span>
</div>

// Target (React.createElement):
React.createElement("div", { className: "foo", onClick: handler },
    children,
    React.createElement("span", null, "text")
);
```

### JSX Fragments

```typescript
// Source:
<>
    <A />
    <B />
</>

// Target:
React.createElement(React.Fragment, null,
    React.createElement(A, null),
    React.createElement(B, null)
);
```

## Helper Detection (`es5_helpers.rs`)

```rust
pub fn has_es5_transforms(&self) -> bool {
    self.transforms.has_any_directive()
}

pub fn needs_extends_helper(&self) -> bool {
    // Check for class extensions
}

pub fn needs_rest_helper(&self) -> bool {
    // Check for rest parameters in destructuring
}

pub fn needs_values_helper(&self) -> bool {
    // Check for for-of loops
}

pub fn needs_async_helpers(&self) -> bool {
    // Check for async functions
}
```

## Known Gaps

### ⚠️ GAP: Interface & Type Alias Emission (`declarations.rs`)

```rust
#[allow(dead_code)]
fn emit_interface_declaration(&mut self, ...) { ... }

#[allow(dead_code)]
fn emit_type_alias_declaration(&mut self, ...) { ... }
```

**Note**: Types are stripped in JavaScript output - these are infrastructure only

### ⚠️ GAP: Decorator Emission (`special_expressions.rs`)

```rust
fn emit_decorator(&mut self, node: NodeIndex) {
    if self.ctx.target_es5 {
        return;  // Skipped entirely in ES5 mode
    }
    // No ES5 equivalent lowering
}
```

**Impact**: Decorators not downleveled to ES5

### ⚠️ GAP: Export Assignment Suppression (`mod.rs`)

```rust
// has_export_assignment flag to suppress other exports
// Flag checked but not fully integrated throughout
```

### ⚠️ GAP: Comment Handling Edge Cases (`mod.rs`)

- Triple-slash directives filtered (`/// <reference`, `/// <amd`)
- Comment tracking infrastructure-only

### ⚠️ GAP: Recursion Overflow (`mod.rs`)

```rust
if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
    self.writer.write("/* emit recursion limit exceeded */");
    return;  // Silent failure with comment
}
```

**Impact**: Very deep ASTs emit comment marker instead of code

## Performance Considerations

### NodeArena Access

- 16-byte thin nodes enable cache-efficient traversal
- Data pools indexed by `data_index` avoid pointer chasing
- Borrowed arena reference avoids ownership overhead

### String Writing

```rust
impl SourceWriter {
    fn write(&mut self, s: &str) {
        self.output.push_str(s);  // Amortized O(1)
    }

    fn new_line(&mut self) {
        self.output.push('\n');
        self.current_line += 1;
        self.current_column = 0;
    }
}
```

### Transform Dispatch Optimization (`mod.rs`)

```rust
fn kind_may_have_transform(&self, kind: SyntaxKind) -> bool {
    // Pre-filter: only check transforms for kinds that might have them
    matches!(kind,
        ClassDeclaration | ClassExpression |
        ArrowFunction | FunctionDeclaration |
        ForOfStatement | VariableDeclarationList |
        // ... limited set
    )
}
```

---

**Previous**: [05-solver.md](./05-solver.md) - Solver Module
**Next**: [07-gaps-summary.md](./07-gaps-summary.md) - Consolidated Gaps
