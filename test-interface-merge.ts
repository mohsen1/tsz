// Test 1: Simple non-generic merged interface
interface A extends C {
    y: string;
}

interface A extends C2 {
    z: string;
}

class C {
    a: number;
}

class C2 {
    b: number;
}

// Should not require type arguments
class D implements A {
    a: number;
    b: number;
    y: string;
    z: string;
}

var a: A;
