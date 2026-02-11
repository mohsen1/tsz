interface A {
    y: string;
}

namespace M {
    interface A<T> {
        z: T;
    }
}

class D implements A {
    y: string;
}
