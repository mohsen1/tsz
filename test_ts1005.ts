// Test 1: default without export
default abstract class C {}

// Test 2: import abstract
import abstract class D {}

// Test 3: void as class name
class void {}

// Test 4: await in parameter default
var foo = async (a = await => await): Promise<void> => {}

// Test 5: debugger as namespace name
declare namespace debugger {}
