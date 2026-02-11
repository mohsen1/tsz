interface Foo {
    e: void;
    f: number[];
    g: Object;
    h: (x: number) => number;
}

var a: Foo = {
    e: null,
    f: [1],
    g: {},
    h: (x: number) => 1
}
