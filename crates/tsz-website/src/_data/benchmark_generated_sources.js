function countFromName(name, pattern) {
  const match = String(name || "").match(pattern);
  return match ? Number(match[1]) : null;
}

function generatedUnionSource(count) {
  const lines = [
    "// Union type stress test - discriminated unions with many members",
    "",
    "type StressEvent =",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`  | { type: "event${i}"; payload${i}: string; timestamp: number }${i === count - 1 ? ";" : ""}`);
  }
  lines.push("", "function handleEvent(event: StressEvent): string {", "  switch (event.type) {");
  for (let i = 0; i < count; i += 1) {
    lines.push(`    case "event${i}": return event.payload${i};`);
  }
  lines.push("    default: throw new Error(\"unreachable\");", "  }", "}", "");
  for (let i = 0; i < count; i += 10) {
    lines.push(`function isEvent${i}(e: StressEvent): e is Extract<StressEvent, { type: "event${i}" }> {`);
    lines.push(`  return e.type === "event${i}";`);
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedClassesSource(count) {
  const lines = ["// Synthetic TypeScript benchmark file", ""];
  for (let i = 0; i < count; i += 1) {
    lines.push(`export interface Config${i} {`);
    lines.push("  readonly id: number;");
    lines.push("  name: string;");
    lines.push("  enabled: boolean;");
    lines.push("  options?: Record<string, unknown>;");
    lines.push("}", "");
    lines.push(`export class Service${i} implements Config${i} {`);
    lines.push(`  readonly id: number = ${i};`);
    lines.push("  name: string;");
    lines.push("  enabled: boolean = true;");
    lines.push("  private items: string[] = [];");
    lines.push("  constructor(name: string) { this.name = name; }");
    lines.push("  getId(): number { return this.id; }");
    lines.push("  getName(): string { return this.name; }");
    lines.push("  setName(value: string): void { this.name = value; }");
    lines.push("  isEnabled(): boolean { return this.enabled; }");
    lines.push("  addItem(item: string): void { this.items.push(item); }");
    lines.push("  getItems(): readonly string[] { return this.items; }");
    lines.push(`  static create(name: string): Service${i} { return new Service${i}(name); }`);
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedGenericFunctionsSource(count) {
  const lines = [
    "// Complex TypeScript with generics, unions, and conditional types",
    '/// <reference lib="es2015.promise" />',
    "",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "",
    "interface Result<T, E = Error> {",
    "  ok: boolean;",
    "  value?: T;",
    "  error?: E;",
    "}",
    "",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`async function process${i}<T extends Record<string, unknown>>(`);
    lines.push("  input: T,");
    lines.push("  options?: DeepPartial<{ timeout: number; retries: number }>");
    lines.push("): Promise<Result<T>> {");
    lines.push("  const timeout = options?.timeout ?? 1000;");
    lines.push("  const retries = options?.retries ?? 3;");
    lines.push("  for (let attempt = 0; attempt < retries; attempt++) {");
    lines.push("    try {");
    lines.push("      const result = await Promise.resolve(input);");
    lines.push("      if (timeout < 0) throw new Error(\"timeout\");");
    lines.push("      return { ok: true, value: result };");
    lines.push("    } catch (e) {");
    lines.push("      if (attempt === retries - 1) return { ok: false, error: e as Error };");
    lines.push("    }");
    lines.push("  }");
    lines.push("  return { ok: false, error: new Error(\"exhausted\") };");
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedOptionalChainSource(count, deep) {
  const scoreExpr = "(options?.timeout ?? 1000) + (options?.nested?.transport?.backoff?.base ?? 10) + (options?.nested?.transport?.backoff?.max ?? 100) + (options?.nested?.transport?.backoff?.jitter ?? 1) + (options?.nested?.flags?.safe ? 1 : 0) + (options?.nested?.flags?.fast ? 1 : 0) + (options?.retries ?? 3)";
  const lines = [
    deep
      ? "// DeepPartial + optional-chain hotspot benchmark."
      : "// Shallow optional-chain control benchmark.",
    "",
  ];
  if (deep) {
    lines.push("type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;");
    lines.push("type Normalize<T> = T extends object ? { [P in keyof T]: Normalize<T[P]> } : T;");
    lines.push("type DeepInput<T> = DeepPartial<Normalize<T>>;", "");
  }
  lines.push(`interface RetryOptions${deep ? "" : "Shallow"} {`);
  lines.push(`  timeout${deep ? "" : "?"}: number;`);
  lines.push(`  retries${deep ? "" : "?"}: number;`);
  lines.push(`  nested${deep ? "" : "?"}: {`);
  lines.push(`    transport${deep ? "" : "?"}: { backoff${deep ? "" : "?"}: { base${deep ? "" : "?"}: number; max${deep ? "" : "?"}: number; jitter${deep ? "" : "?"}: number; }; };`);
  lines.push(`    flags${deep ? "" : "?"}: { fast${deep ? "" : "?"}: boolean; safe${deep ? "" : "?"}: boolean; };`);
  lines.push("  };");
  lines.push("}", "");
  for (let i = 0; i < count; i += 1) {
    lines.push(`function ${deep ? "deepPartialHotspot" : "shallowOptionalControl"}${i}(`);
    lines.push(`  options?: ${deep ? "DeepInput<RetryOptions>" : "RetryOptionsShallow"}`);
    lines.push("): number {");
    lines.push("  let score = 0;");
    for (let j = 0; j < 34; j += 1) {
      lines.push(`  score += ${scoreExpr};`);
    }
    lines.push("  return score;");
    lines.push("}", "");
  }
  return lines.join("\n").trimEnd();
}

function generatedMappedTypeSource(count) {
  const lines = [
    "// Mapped type expansion stress test",
    "",
    "type MyOptional<T> = { [K in keyof T]?: T[K] };",
    "type MyRequired<T> = { [K in keyof T]-?: T[K] };",
    "type MyReadonly<T> = { readonly [K in keyof T]: T[K] };",
    "type MyMutable<T> = { -readonly [K in keyof T]: T[K] };",
    "type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };",
    "type Setters<T> = { [K in keyof T as `set${Capitalize<string & K>}`]: (val: T[K]) => void };",
    "",
    "interface BigObject {",
  ];
  for (let i = 0; i < count; i += 1) lines.push(`  prop${i}: string;`);
  lines.push("}", "");
  lines.push("type Partial1 = MyOptional<BigObject>;");
  lines.push("type Readonly1 = MyReadonly<BigObject>;");
  lines.push("type Both = MyReadonly<MyOptional<BigObject>>;");
  lines.push("type BigGetters = Getters<BigObject>;");
  lines.push("type BigSetters = Setters<BigObject>;");
  lines.push("type DeepOptional<T> = T extends object ? { [K in keyof T]?: DeepOptional<T[K]> } : T;");
  lines.push("type DeepBigObject = DeepOptional<BigObject>;", "");
  lines.push("declare const partial: Partial1;");
  lines.push("declare const getters: BigGetters;");
  lines.push("declare const deep: DeepBigObject;");
  lines.push("const _prop0 = partial.prop0;");
  return lines.join("\n").trimEnd();
}

function generatedConditionalDistributionSource(count) {
  const lines = [
    "// Conditional type distribution stress test",
    "",
    "type ExtractString<T> = T extends string ? T : never;",
    "type ToArray<T> = T extends any ? T[] : never;",
    "type Flatten<T> = T extends (infer U)[] ? Flatten<U> : T;",
    "",
    "type BigUnion =",
  ];
  for (let i = 0; i < count; i += 1) lines.push(`  | "value${i}"${i === count - 1 ? ";" : ""}`);
  lines.push("", "type Distributed1 = ToArray<BigUnion>;");
  lines.push("type Distributed2 = ExtractString<BigUnion | number>;");
  lines.push("type ChainedConditional<T> = T extends string ? `prefix_${T}` : T extends number ? T : never;");
  lines.push("type Applied = ChainedConditional<BigUnion>;");
  lines.push("type NestedConditional<T> = T extends `value${infer N}` ? N extends `${infer D}${infer Rest}` ? D : never : never;");
  lines.push("type Extracted = NestedConditional<BigUnion>;");
  lines.push("declare const distributed: Distributed1;");
  lines.push("declare const applied: Applied;");
  lines.push("declare const extracted: Extracted;");
  return lines.join("\n").trimEnd();
}

function generatedTemplateLiteralSource(count) {
  const maxVariants = Math.min(count, 50);
  const lines = ["// Template literal type expansion stress test", ""];
  for (const name of ["Colors", "Sizes", "Variants"]) {
    lines.push(`type ${name} =`);
    const prefix = name === "Colors" ? "color" : name === "Sizes" ? "size" : "variant";
    for (let i = 0; i < maxVariants; i += 1) lines.push(`  | "${prefix}${i}"${i === maxVariants - 1 ? ";" : ""}`);
    lines.push("");
  }
  lines.push("type ProductSmall = `${Colors}-${Sizes}`;");
  lines.push("type ProductMedium = `${Colors}-${Sizes}-${Variants}`;");
  lines.push("type Prefixed = `prefix_${Colors}`;");
  lines.push("type Suffixed = `${Colors}_suffix`;");
  lines.push("type Wrapped = `[${Colors}]`;");
  lines.push("type NestedTemplate = `start_${`mid_${Colors}`}_end`;");
  lines.push("declare const product: ProductSmall;");
  lines.push("declare const prefixed: Prefixed;");
  return lines.join("\n").trimEnd();
}

function generatedRecursiveGenericSource(depth) {
  const lines = [
    "// Recursive generic type instantiation stress test",
    "",
    "type LinkedList<T> = { value: T; next: LinkedList<T> | null };",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;",
    "",
  ];
  for (let i = 0; i < depth; i += 1) lines.push(`type Wrap${i}<T> = { layer${i}: T };`);
  let chain = "string";
  for (let i = Math.min(depth, 40) - 1; i >= 0; i -= 1) chain = `Wrap${i}<${chain}>`;
  lines.push("", "type DeepWrapped = " + chain + ";");
  lines.push("declare const deep: DeepWrapped;");
  lines.push("declare function extract<T>(x: Wrap0<T>): T;");
  lines.push("const _test = extract(deep);");
  lines.push("declare const list: LinkedList<number>;");
  lines.push("declare function mapList<T, U>(l: LinkedList<T>, f: (x: T) => U): LinkedList<U>;");
  lines.push("const mapped = mapList(list, x => x.toString());");
  return lines.join("\n").trimEnd();
}

function generatedDeepSubtypeSource(depth) {
  const lines = [
    "// Deep subtype checking stress test",
    "",
    "interface TreeNode<T> { value: T; children: TreeNode<T>[]; }",
    "interface MutualA<T> { data: T; ref: MutualB<T>; }",
    "interface MutualB<T> { info: T; back: MutualA<T>; }",
    "type Json = string | number | boolean | null | Json[] | { [key: string]: Json };",
    "",
    "class Base0 { x0: string = \"\"; }",
  ];
  for (let i = 1; i < Math.min(depth, 50); i += 1) {
    lines.push(`class Base${i} extends Base${i - 1} { x${i}: string = ""; }`);
  }
  let deepFn = "string";
  for (let i = 0; i < Math.min(depth, 30); i += 1) deepFn = `(x: ${deepFn}) => void`;
  lines.push("", "type CovariantContainer<T> = { get(): T };");
  lines.push("type ContravariantContainer<T> = { set(x: T): void };");
  lines.push("type InvariantContainer<T> = { get(): T; set(x: T): void };");
  lines.push(`type DeepFunction = ${deepFn};`);
  lines.push("declare const tree1: TreeNode<string>;");
  lines.push("const _check: TreeNode<string | number> = tree1;");
  return lines.join("\n").trimEnd();
}

function generatedIntersectionSource(count) {
  const lines = ["// Intersection type stress test", ""];
  for (let i = 0; i < count; i += 1) {
    lines.push(`interface Part${i} {`);
    lines.push(`  prop${i}: string;`);
    lines.push("  shared: number;");
    lines.push(`  method${i}(): number;`);
    lines.push("}", "");
  }
  let intersection = "Part0";
  for (let i = 1; i < Math.min(count, 50); i += 1) intersection += ` & Part${i}`;
  lines.push(`type BigIntersection = ${intersection};`);
  lines.push("type OverloadIntersection = ((x: string) => string) & ((x: number) => number) & ((x: boolean) => boolean);");
  lines.push("declare const big: BigIntersection;");
  lines.push("const _prop0 = big.prop0;");
  lines.push("const _shared = big.shared;");
  lines.push(`const _propLast = big.prop${Math.min(count, 50) - 1};`);
  return lines.join("\n").trimEnd();
}

function generatedCfaSource(count) {
  const lines = [
    "// Control flow analysis stress test",
    "",
    "type Entity =",
  ];
  for (let i = 0; i < count; i += 1) {
    lines.push(`  | { kind: "type${i}"; data${i}: string; common: number }${i === count - 1 ? ";" : ""}`);
  }
  lines.push("", "function processEntity(e: Entity): string {", "  switch (e.kind) {");
  for (let i = 0; i < count; i += 1) lines.push(`    case "type${i}": return e.data${i};`);
  lines.push("    default: throw new Error(\"unreachable\");", "  }", "}", "");
  lines.push("function processWithIf(e: Entity): string {");
  for (let i = 0; i < count; i += 1) {
    lines.push(`  if (e.kind === "type${i}") return e.data${i};`);
  }
  lines.push("  return processEntity(e);", "}");
  return lines.join("\n").trimEnd();
}

function generatedBctSource(count) {
  const lines = ["// Best Common Type O(N^2) stress test", "", "class Base { base: string = \"\"; }"];
  for (let i = 0; i < count; i += 1) lines.push(`class Derived${i} extends Base { prop${i}: number = ${i}; }`);
  lines.push("", `const items = [${Array.from({ length: count }, (_, i) => `new Derived${i}()`).join(", ")}];`);
  lines.push("function pickOne(index: number) {");
  for (let i = 0; i < count; i += 1) lines.push(`  if (index === ${i}) return new Derived${i}();`);
  lines.push("  return new Base();", "}", "function identity<T>(x: T): T { return x; }");
  lines.push(`const mixed = [${Array.from({ length: count }, (_, i) => `identity(new Derived${i}())`).join(", ")}];`);
  lines.push("declare const flag: number;");
  lines.push(`const chosen = ${Array.from({ length: count }, (_, i) => `flag === ${i} ? new Derived${i}() :`).join(" ")} new Base();`);
  return lines.join("\n").trimEnd();
}

function generatedConstraintConflictSource(count) {
  const lines = ["// Constraint Conflict Detection O(N^2) stress test", ""];
  for (let i = 0; i < count; i += 1) lines.push(`interface Constraint${i} { key${i}: string; shared: number; }`);
  lines.push("");
  for (let i = 0; i < count; i += 1) lines.push(`declare function constrain${i}<T extends Constraint${i}>(x: T): T;`);
  lines.push("");
  for (let i = 0; i < count; i += 1) {
    const keys = Array.from({ length: i + 1 }, (_, j) => `key${j}: "val"`).join(", ");
    lines.push(`const obj${i} = { shared: ${i}, ${keys} };`);
  }
  lines.push("");
  for (let i = 0; i < count; i += 1) lines.push(`const res${i} = constrain${i}(obj${i});`);
  lines.push(`function multiConstrained<T extends ${Array.from({ length: count }, (_, i) => `Constraint${i}`).join(" & ")}>(x: T): T { return x; }`);
  lines.push(`const allConstraints = { shared: 0, ${Array.from({ length: count }, (_, i) => `key${i}: "val"`).join(", ")} };`);
  lines.push("const _result = multiConstrained(allConstraints);");
  return lines.join("\n").trimEnd();
}

function generatedMappedComplexSource(count) {
  const lines = [
    "// Mapped Type Complex Template Expansion O(N^2) stress test",
    "",
    "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;",
    "type Stringify<T> = { [K in keyof T]: T[K] extends number ? string : T[K] extends boolean ? \"true\" | \"false\" : T[K] extends string ? T[K] : string };",
    "type Validate<T> = { [K in keyof T]: T[K] extends string | number ? { valid: true; value: T[K] } : { valid: false; value: never } };",
    "type Nullable<T> = { [K in keyof T]: T[K] | null | undefined };",
    "type Promisify<T> = { [K in keyof T]: Promise<T[K]> };",
    "type FormField<T> = T extends string ? { type: \"text\"; value: T } : T extends number ? { type: \"number\"; value: T } : T extends boolean ? { type: \"checkbox\"; value: T } : T extends (infer U)[] ? { type: \"list\"; items: FormField<U>[] } : T extends object ? { type: \"group\"; fields: FormFields<T> } : { type: \"unknown\"; value: T };",
    "type FormFields<T> = { [K in keyof T]: FormField<T[K]> };",
    "",
    "interface BigModel {",
  ];
  for (let i = 0; i < count; i += 1) {
    const type = ["string", "number", "boolean", "string[]", "{ nested: string; count: number }"][i % 5];
    lines.push(`  field${i}: ${type};`);
  }
  lines.push("}", "");
  lines.push("type BigForm = FormFields<BigModel>;");
  lines.push("type BigStringified = Stringify<BigModel>;");
  lines.push("type BigValidated = Validate<BigModel>;");
  lines.push("type Chained3 = FormFields<Nullable<BigModel>>;");
  lines.push("declare const form: BigForm;");
  lines.push(`const _fLast = form.field${count - 1};`);
  return lines.join("\n").trimEnd();
}

function generatedTypedArraysSource() {
  return `// Typed array benchmark fixture used by bench-vs-tsgo.sh.
// Keep this strict/explicit so all compilers can parse and type-check it.

function createTypedArrayInstancesFromLength(length: number) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(length);
    typedArrays[1] = new Uint8Array(length);
    typedArrays[2] = new Int16Array(length);
    typedArrays[3] = new Uint16Array(length);
    typedArrays[4] = new Int32Array(length);
    typedArrays[5] = new Uint32Array(length);
    typedArrays[6] = new Float32Array(length);
    typedArrays[7] = new Float64Array(length);
    typedArrays[8] = new Uint8ClampedArray(length);
    return typedArrays;
}

function createTypedArrayInstancesFromArrayLike(obj: ArrayLike<number>) {
    const typedArrays = [];
    typedArrays[0] = new Int8Array(obj);
    typedArrays[1] = new Uint8Array(obj);
    typedArrays[2] = new Int16Array(obj);
    typedArrays[3] = new Uint16Array(obj);
    typedArrays[4] = new Int32Array(obj);
    typedArrays[5] = new Uint32Array(obj);
    typedArrays[6] = new Float32Array(obj);
    typedArrays[7] = new Float64Array(obj);
    typedArrays[8] = new Uint8ClampedArray(obj);
    return typedArrays;
}

function createTypedArraysFromMapFn(
    obj: ArrayLike<number>,
    mapFn: (n: number, v: number) => number
) {
    const typedArrays = [];
    typedArrays[0] = Int8Array.from(obj, mapFn);
    typedArrays[1] = Uint8Array.from(obj, mapFn);
    typedArrays[2] = Int16Array.from(obj, mapFn);
    typedArrays[3] = Uint16Array.from(obj, mapFn);
    typedArrays[4] = Int32Array.from(obj, mapFn);
    typedArrays[5] = Uint32Array.from(obj, mapFn);
    typedArrays[6] = Float32Array.from(obj, mapFn);
    typedArrays[7] = Float64Array.from(obj, mapFn);
    typedArrays[8] = Uint8ClampedArray.from(obj, mapFn);
    return typedArrays;
}

const values: number[] = [1, 2, 3, 4];
const mapped = createTypedArraysFromMapFn(values, (n, i) => n + i);
const fromLength = createTypedArrayInstancesFromLength(128);
const fromArrayLike = createTypedArrayInstancesFromArrayLike(values);
const sampleCount = mapped.length + fromLength.length + fromArrayLike.length;`;
}

function generatedInferStressSource(count) {
  const maxFunctions = Math.min(count, 30);
  const lines = [
    "// Infer keyword stress test",
    "// Tests inference variable resolution in conditional types",
    "",
    "type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;",
    "type UnwrapArray<T> = T extends (infer U)[] ? U : T;",
    "type MyParameters<T> = T extends (...args: infer P) => any ? P : never;",
    "type MyReturnType<T> = T extends (...args: any[]) => infer R ? R : never;",
    "",
    "type FirstAndRest<T> = T extends [infer First, ...infer Rest] ? { first: First; rest: Rest } : never;",
    "",
    "type DeepUnwrap<T> =",
    "    T extends Promise<infer U> ? DeepUnwrap<U> :",
    "    T extends (infer V)[] ? DeepUnwrap<V>[] :",
    "    T;",
    "",
    "type ExtractPrefix<T> = T extends `${infer P}_${string}` ? P : never;",
    "type ExtractIfString<T> = T extends infer U extends string ? U : never;",
    "",
  ];

  for (let i = 0; i < maxFunctions; i += 1) {
    lines.push(`declare function func${i}(`);
    for (let j = 0; j <= i; j += 1) {
      lines.push(`    arg${j}: string${j === i ? "" : ","}`);
    }
    lines.push("): number;", "", `type Params${i} = MyParameters<typeof func${i}>;`, `type Return${i} = MyReturnType<typeof func${i}>;`, "");
  }

  lines.push(
    "type ComplexInfer<T> = T extends {",
    "    data: infer D;",
    "    nested: { value: infer V }[]",
    "} ? { data: D; values: V[] } : never;",
    "",
    "interface TestData {",
    "    data: string;",
    "    nested: { value: number }[];",
    "}",
    "",
    "type Inferred = ComplexInfer<TestData>;",
    "",
    `declare const params: Params${maxFunctions - 1};`,
    "declare const inferred: Inferred;",
  );

  return lines.join("\n").trimEnd();
}

export function generatedBenchmarkSource(name) {
  if (String(name || "") === "typedArrays.ts") return generatedTypedArraysSource();

  const unionCount = countFromName(name, /^(\d+)\s+union members$/i);
  if (unionCount) return generatedUnionSource(unionCount);

  const classCount = countFromName(name, /^(\d+)\s+classes$/i);
  if (classCount) return generatedClassesSource(classCount);

  const functionCount = countFromName(name, /^(\d+)\s+generic functions$/i);
  if (functionCount) return generatedGenericFunctionsSource(functionCount);

  const deepOptionalCount = countFromName(name, /^DeepPartial optional-chain N=(\d+)$/i);
  if (deepOptionalCount) return generatedOptionalChainSource(deepOptionalCount, true);

  const shallowOptionalCount = countFromName(name, /^Shallow optional-chain N=(\d+)$/i);
  if (shallowOptionalCount) return generatedOptionalChainSource(shallowOptionalCount, false);

  const mappedCount = countFromName(name, /^Mapped type keys=(\d+)$/i);
  if (mappedCount) return generatedMappedTypeSource(mappedCount);

  const conditionalCount = countFromName(name, /^Conditional dist N=(\d+)$/i);
  if (conditionalCount) return generatedConditionalDistributionSource(conditionalCount);

  const templateCount = countFromName(name, /^Template literal N=(\d+)$/i);
  if (templateCount) return generatedTemplateLiteralSource(templateCount);

  const recursiveDepth = countFromName(name, /^Recursive generic depth=(\d+)$/i);
  if (recursiveDepth) return generatedRecursiveGenericSource(recursiveDepth);

  const deepSubtypeDepth = countFromName(name, /^Deep subtype depth=(\d+)$/i);
  if (deepSubtypeDepth) return generatedDeepSubtypeSource(deepSubtypeDepth);

  const intersectionCount = countFromName(name, /^Intersection N=(\d+)$/i);
  if (intersectionCount) return generatedIntersectionSource(intersectionCount);

  const cfaCount = countFromName(name, /^CFA branches=(\d+)$/i);
  if (cfaCount) return generatedCfaSource(cfaCount);

  const bctCount = countFromName(name, /^BCT candidates=(\d+)$/i);
  if (bctCount) return generatedBctSource(bctCount);

  const constraintCount = countFromName(name, /^Constraint conflicts N=(\d+)$/i);
  if (constraintCount) return generatedConstraintConflictSource(constraintCount);

  const mappedComplexCount = countFromName(name, /^Mapped complex template keys=(\d+)$/i);
  if (mappedComplexCount) return generatedMappedComplexSource(mappedComplexCount);

  const inferStressCount = countFromName(name, /^Infer stress N=(\d+)$/i);
  if (inferStressCount) return generatedInferStressSource(inferStressCount);

  return null;
}
