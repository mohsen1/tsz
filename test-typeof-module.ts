namespace M {
    export var y = 1;
}

interface Foo {
    m: typeof M;
}

var a: Foo = {
    m: M
};
