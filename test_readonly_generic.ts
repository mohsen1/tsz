// Test different levels of generic parameter resolution
interface Props {
    foo: string;
}

// Direct generic parameter - should resolve to constraint
function test1<P extends Props>(props: P) {
    props.foo; // Should work
}

// Readonly wrapper - should also resolve
function test2<P extends Props>(props: Readonly<P>) {
    props.foo; // Currently fails, should work
}

// Multiple nesting
type MyReadonly<T> = Readonly<T>;
function test3<P extends Props>(props: MyReadonly<P>) {
    props.foo; // Should work
}
