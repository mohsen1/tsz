// Test 2: Non-generic at top level + generic in namespace with same name

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

class D implements A {  // Should not require type arguments
    a: number;
    b: number;
    y: string;
    z: string;
}

namespace M {
    class C<T> {
        a: T;
    }

    class C2<T> {
        b: T;
    }

    interface A<T> extends C<T> {
        y: T;
    }

    interface A<T> extends C2<string> {
        z: T;
    }

    class D implements A<boolean> {  // Should require type arguments
        a: boolean;
        b: string;
        y: boolean;
        z: boolean;
    }
}
