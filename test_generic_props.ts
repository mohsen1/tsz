// Simple test for generic parameter property access
interface Props {
    onFoo?: (value: string) => boolean;
}

function test<P extends Props>(props: Readonly<P>) {
    props.onFoo; // Should resolve onFoo, not error
}
