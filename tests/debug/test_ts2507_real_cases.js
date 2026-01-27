const notAConstructor = 42;
new notAConstructor();
const x = "hello";
new x();
class A {
    constructor(x: number) {
    }
}
class B {
    constructor(x: string) {
    }
}
new ctorUnion1();
new ctorUnion2();
