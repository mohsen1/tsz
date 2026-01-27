const notIterable = { x: 1 };
const arr = [...notIterable];
for (var x = void 0 of notIterable) {
}
const [a, b] = notIterable;
function testFn(a, b) {
}
testFn(...notIterable);
const arr2 = [...notIterable, ...notIterable];
const [[c, d]] = [notIterable];
const [first, ...rest] = notIterable;
const arr3 = [...null];
for (var y = void 0 of null) {
}
const [e, f] = null;
const validArray = [1, 2, 3];
const arr4 = [...validArray];
for (var z = void 0 of validArray) {
}
const [g, h, i] = validArray;
const str = "hello";
const arr5 = [...str];
for (var ch = void 0 of str) {
}
const [j, k] = str;
console.log("TS2488 verification complete");
