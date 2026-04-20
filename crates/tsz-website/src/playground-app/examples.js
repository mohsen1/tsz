export const playgroundExamples = [
  {
    key: "hello",
    title: "Hello World",
    category: "basics",
    description: "A minimal typed function and value flow.",
    source: `const greeting: string = "Hello, tsz!";
console.log(greeting);

function add(a: number, b: number): number {
  return a + b;
}

const result = add(1, 2);
`,
  },
  {
    key: "literals",
    title: "Literal Types",
    category: "type-system",
    description: "String and numeric literals drive exact type behavior.",
    source: `type HttpMethod = "GET" | "POST";

function send(method: HttpMethod) {
  return method;
}

const ok = send("GET");
const created = 201 as const;

type Status = typeof created;
`,
  },
  {
    key: "unions",
    title: "Union Types",
    category: "type-system",
    description: "Combine related possibilities into one type.",
    source: `type Result =
  | { ok: true; value: string }
  | { ok: false; error: Error };

function format(result: Result) {
  if (result.ok) {
    return result.value.toUpperCase();
  }
  return result.error.message;
}
`,
  },
  {
    key: "intersections",
    title: "Intersection Types",
    category: "type-system",
    description: "Merge shape requirements together.",
    source: `type Named = { name: string };
type Timestamped = { createdAt: Date };
type RecordWithMeta = Named & Timestamped;

const record: RecordWithMeta = {
  name: "build",
  createdAt: new Date(),
};
`,
  },
  {
    key: "tuples",
    title: "Tuples",
    category: "type-system",
    description: "Use fixed-length ordered arrays with specific element types.",
    source: `type Point = [x: number, y: number];

const origin: Point = [0, 0];

function move(point: Point, dx: number, dy: number): Point {
  return [point[0] + dx, point[1] + dy];
}
`,
  },
  {
    key: "readonly",
    title: "Readonly Collections",
    category: "type-system",
    description: "Readonly arrays and tuples protect immutable data.",
    source: `const palette: readonly string[] = ["red", "green", "blue"];
const axis: readonly [number, number, number] = [1, 0, 0];

function first(values: readonly string[]) {
  return values[0];
}

const head = first(palette);
`,
  },
  {
    key: "object-shapes",
    title: "Structural Objects",
    category: "type-system",
    description: "Object compatibility is driven by structure, not nominal identity.",
    source: `interface User {
  id: string;
  profile: {
    name: string;
    admin: boolean;
  };
}

const user: User = {
  id: "u1",
  profile: { name: "Ada", admin: true },
};
`,
  },
  {
    key: "keyof-indexed-access",
    title: "Keyof and Indexed Access",
    category: "type-system",
    description: "Derive property names and property types from object shapes.",
    source: `interface Settings {
  theme: "light" | "dark";
  retries: number;
  cache: boolean;
}

type SettingKey = keyof Settings;
type Theme = Settings["theme"];

function read<K extends SettingKey>(settings: Settings, key: K): Settings[K] {
  return settings[key];
}
`,
  },
  {
    key: "generics",
    title: "Generics",
    category: "inference",
    description: "Reusable functions preserve the caller's types.",
    source: `function identity<T>(value: T): T {
  return value;
}

interface Container<T> {
  value: T;
  map<U>(fn: (val: T) => U): Container<U>;
}

function wrap<T>(value: T): Container<T> {
  return {
    value,
    map(fn) {
      return wrap(fn(value));
    },
  };
}

const boxed = wrap(42).map(n => n.toString());
`,
  },
  {
    key: "generic-constraints",
    title: "Generic Constraints",
    category: "inference",
    description: "Constrain generics so callers supply compatible shapes.",
    source: `interface HasId {
  id: string;
}

function pluckId<T extends HasId>(value: T): string {
  return value.id;
}

const team = pluckId({ id: "team-1", name: "compiler" });
`,
  },
  {
    key: "narrowing",
    title: "Type Narrowing",
    category: "control-flow",
    description: "Control flow refines unions into safer specific types.",
    source: `type Shape =
  | { kind: "circle"; radius: number }
  | { kind: "rectangle"; width: number; height: number };

function area(shape: Shape): number {
  switch (shape.kind) {
    case "circle":
      return Math.PI * shape.radius ** 2;
    case "rectangle":
      return shape.width * shape.height;
    default:
      const exhaustive: never = shape;
      return exhaustive;
  }
}
`,
  },
  {
    key: "overloads",
    title: "Overloads",
    category: "functions",
    description: "Expose multiple call signatures with one implementation.",
    source: `function makeLabel(value: string): { kind: "name"; value: string };
function makeLabel(value: number): { kind: "id"; value: number };
function makeLabel(value: string | number) {
  return typeof value === "string"
    ? { kind: "name", value }
    : { kind: "id", value };
}

const named = makeLabel("compiler");
const identified = makeLabel(42);
`,
  },
  {
    key: "classes",
    title: "Classes",
    category: "objects",
    description: "Class fields, methods, and private state remain strongly typed.",
    source: `abstract class Logger {
  abstract write(message: string): void;
}

class MemoryLogger extends Logger {
  #lines: string[] = [];

  write(message: string) {
    this.#lines.push(message);
  }

  all(): readonly string[] {
    return this.#lines;
  }
}
`,
  },
  {
    key: "modules",
    title: "Modules",
    category: "declarations",
    description: "Exported surfaces flow into declaration emit cleanly.",
    source: `export type Id = string | number;

export interface User {
  id: Id;
  name: string;
  tags?: readonly string[];
}

export function createUser(name: string): User {
  return { id: name.toLowerCase(), name };
}
`,
  },
  {
    key: "dts",
    title: "Declaration Emit",
    category: "declarations",
    description: "A slightly richer export surface for DTS output.",
    source: `export type Id = string | number;

export interface User {
  id: Id;
  name: string;
  tags?: readonly string[];
}

export class UserStore<T extends User> {
  #items: T[] = [];

  add(user: T): void {
    this.#items.push(user);
  }

  getById(id: Id): T | undefined {
    return this.#items.find(item => item.id === id);
  }

  all(): readonly T[] {
    return this.#items;
  }
}
`,
  },
  {
    key: "errors",
    title: "Type Errors",
    category: "diagnostics",
    description: "Intentional checker failures for diagnostics output.",
    source: `// Intentional type errors - tsz should catch all of these

let x: string = 42;

function greet(name: string): string {
  return "Hello, " + name;
}

greet(123);

interface User {
  name: string;
  age: number;
}

const user: User = {
  name: "Alice",
  age: "thirty",
};
`,
  },
];

const exampleMap = new Map(playgroundExamples.map(example => [example.key, example]));

export function getDefaultExampleKey() {
  return playgroundExamples[0].key;
}

export function getExampleByKey(key) {
  return exampleMap.get(key) ?? null;
}

export function isValidExampleKey(key) {
  return exampleMap.has(key);
}