class MyClass {
    x: number;
}

function testSimple<T extends { a: string }>(val: T) {
    if (val instanceof MyClass) {
        const x: number = val.x;
        const a: string = val.a;
    }
}

function testSequential<T extends { a: string }>(val: T) {
    if (val instanceof MyClass) {
        const x1: number = val.x;
    }

    if (val instanceof MyClass) {
        const x2: number = val.x;
    }
}
