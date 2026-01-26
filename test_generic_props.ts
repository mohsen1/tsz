// Test generic type property access

// Test 1: Partial mapped type
type Partial<T> = { [P in keyof T]?: T[P] };

interface User {
    name: string;
    age: number;
}

const user: Partial<User> = { name: "John" };
const nameVal = user.name;  // Should work: string | undefined
const ageVal = user.age;    // Should work: number | undefined

// Test 2: Readonly mapped type
type Readonly<T> = { readonly [P in keyof T]: T[P] };

const readonlyUser: Readonly<User> = { name: "John", age: 30 };
const readonlyName = readonlyUser.name;  // Should work: string
const readonlyAge = readonlyUser.age;    // Should work: number

// Test 3: Generic class
class Container<T> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
    getValue(): T {
        return this.value;
    }
}

const container = new Container(42);
const val = container.value;  // Should work: number
const getVal = container.getValue();  // Should work: number

// Test 4: Generic function with constraint
interface Lengthwise {
    length: number;
}

function getLength<T extends Lengthwise>(arg: T): number {
    return arg.length;  // Should work - T extends Lengthwise which has length
}

const len = getLength("hello");  // Should work: number

// Test 5: Conditional type
type NonNullable<T> = T extends null | undefined ? never : T;

type Props = { x: string | null };
const obj: { x: NonNullable<Props["x"]> } = { x: "hello" };
const xVal = obj.x;  // Should work: string
