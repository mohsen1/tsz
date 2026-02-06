declare function bar(): Promise<void>;
async function foo() {
    await bar();
}
