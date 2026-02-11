class C { foo: string; }
function f1() { }
namespace M {
    export var y = 1;
}
enum E { A }

interface Foo {
    a: number;
    b: string;
    c: boolean;
    d: any;
    e: void;
    f: number[];
    g: Object;
    h: (x: number) => number;
}

var a: Foo = {
    a: 1,
    b: '',
    c: true,
    d: {},
    e: null,
    f: [1],
    g: {},
    h: (x: number) => 1
}
