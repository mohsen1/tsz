// @noUnusedLocals: true

// Should NOT report unused - used in element access
const key1 = "property";
const obj1 = { property: 1 };
console.log(obj1[key1]);

// Should NOT report unused - used in computed property name
const key2 = "method";
const obj2 = {
  [key2]() { return 1; }
};
obj2[key2]();

// Should NOT report unused - Symbol usage in computed property
const sym = Symbol("test");
const obj3 = {
  [sym]: "value"
};
console.log(obj3[sym]);

// Should NOT report unused - Symbol used as class property
const classSym = Symbol("classProp");
class MyClass {
  [classSym] = "class property";
}
const instance = new MyClass();
console.log(instance[classSym]);

// Should NOT report unused - computed property in type extension
const extendKey = "extended";
const baseObj = { extended: 42 };
const extendedObj = {
  ...baseObj,
  [extendKey]: 100
};
console.log(extendedObj[extendKey]);

// Should NOT report unused - ambient declarations
declare const ambientVar: string;
declare function ambientFunc(): void;
declare class AmbientClass {
  prop: string;
}

// Should report unused - truly unused variable
const actuallyUnused = 123;

// Should NOT report unused - used in array computed access
const arrayIndex = 0;
const arr = [1, 2, 3];
console.log(arr[arrayIndex]);

// Should NOT report unused - used in nested computed property
const nestedKey = "nested";
const nestedObj = {
  outer: {
    [nestedKey]: "value"
  }
};
console.log(nestedObj.outer[nestedKey]);
