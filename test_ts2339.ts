
const obj: { a: string } | { b: number } = { a: "test" };
if ("a" in obj) {
    obj.a; // Should be string, but our narrower may not narrow properly
}

