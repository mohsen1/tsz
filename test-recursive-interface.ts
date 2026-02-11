interface Foo {
    j: Foo;
}

var a: Foo = {
    j: <Foo>null
};
