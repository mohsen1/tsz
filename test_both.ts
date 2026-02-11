interface Props {
    foo: string;
}

// This works
function test1<P extends Props>(props: P) {
    props.foo;
}

// This fails
function test2<P extends Props>(props: Readonly<P>) {
    props.foo;
}
