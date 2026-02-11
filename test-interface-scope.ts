// Test 3: Check if top-level A is separate from M.A<T>

interface A {
    y: string;
}

namespace M {
    interface A<T> {
        y: T;
    }
}

// This should NOT require type arguments (top-level A is non-generic)
class D implements A {
    y: string;
}

// This SHOULD require type arguments (M.A is generic)
class E implements M.A<number> {
    y: number;
}
