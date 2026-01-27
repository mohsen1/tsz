function testUnion(u) {
    if ("x"  u) {
        console.log(u.x);
    }
    if ("y"  u) {
        console.log(u.y);
    }
}
function testIntersection(i) {
    console.log(i.x);
    console.log(i.y);
}
function testOptionalChaining(o) {
    console.log(o.a.b);
    console.log(o.a.b.toUpperCase());
}
function testIndexAlias(map) {
    console.log(map.anyProperty);
    console.log(map[123]);
}
function testTuple(t) {
    console.log(t[0]);
    console.log(t[1]);
    console.log(t[2]);
}
function generic(obj) {
    console.log(obj.x);
}
const obj = {
    a: 1,
    b: 2
};
function testTypeof(o) {
    console.log(o.a);
    console.log(o.b);
    console.log(o.c);
}
class DynamicClass {
    [key: string]: string
    constructor() {
        this.a = "hello";
    }
}
function testDynamic(d) {
    console.log(d.a);
    console.log(d.b);
}
console.log("TS2339 investigation tests loaded");
