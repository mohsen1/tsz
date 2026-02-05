class MyClass {
    x: number;
}

function testCompound1<T extends { a: string }>(val1: T, val2: T) {
    if (val1 instanceof MyClass && val2 instanceof MyClass) {
        const x1: number = val1.x;
        const x2: number = val2.x;
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
