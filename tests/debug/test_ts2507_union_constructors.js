var numOrDate;
numOrDate = new UnionCtor(10);
new UnionCtor2(10);
new UnionCtor2("hello");
var strOrNum;
strOrNum = new UnionCtor3("hello");
strOrNum = new UnionCtor3("hello", 10);
class A {
    constructor(x: number) {
    }
}
class B {
    constructor(x: number) {
    }
}
const instance1 = new ClassUnion(10);
class C {
    constructor(x: string) {
    }
}
const instance2 = new ClassUnion2(10);
const instance3 = new ClassUnion2("hello");
function factory(ctor) {
    return new ctor(10);
}
var result;
result = new AliasUnion(10);
new UnionCtor4(10);
