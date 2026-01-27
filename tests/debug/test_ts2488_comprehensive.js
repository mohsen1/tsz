const num = 123;
for (var x = void 0 of num) {
}
const obj = {
    a: 1,
    b: 2
};
for (var x = void 0 of obj) {
}
for (var v = void 0 of iterableWithOptionalIterator) {
}
class MyStringIterator {
    next() {
        return {
            value: "",
            done: false
        };
    }
}
for (var v = void 0 of new MyStringIterator()) {
}
const spreadNum = [...123];
const spreadObj = [...{ a: 1 }];
const [a, b] = {
    0: "",
    1: true
};
const [c, d] = 456;
function testTypeParam(x) {
    for (var item = void 0 of x) {
    }
}
for (var item = void 0 of indexed) {
}
for (var item = void 0 of conditional) {
}
for (var item = void 0 of mapped) {
}
for (var item = void 0 of union) {
}
for (var item = void 0 of inter) {
}
for (var item = void 0 of fn) {
}
for (var item = void 0 of neverVal) {
}
const [e, f] = neverVal;
for (var item = void 0 of null) {
}
const [g, h] = null;
class MyClass {
    value = 42;
}
for (var item = void 0 of new MyClass()) {
}
for (var item = void 0 of genericFn) {
}
