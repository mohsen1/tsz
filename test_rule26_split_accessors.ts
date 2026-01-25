// Test for Rule #26: Split Accessors (Getter/Setter Variance)
// This tests that properties have covariant reads and contravariant writes

// Test 1: Basic split accessor - getter returns narrower type than setter accepts
class SplitAccessor1 {
  private _x: string | number;
  
  // Getter returns string (narrower)
  get x(): string {
    return typeof this._x === 'string' ? this._x : '';
  }
  
  // Setter accepts string | number (wider)
  set x(value: string | number) {
    this._x = value;
  }
}

// Test 2: Assignability - covariant reads
class ReadOnly {
  get x(): string {
    return 'test';
  }
}

class ReadWrite {
  private _x: string;
  
  get x(): string {
    return this._x;
  }
  
  set x(value: string) {
    this._x = value;
  }
}

// ReadWrite can be assigned to ReadOnly (read-only target is satisfied)
const test1: ReadOnly = new ReadWrite(); // OK

// ReadOnly cannot be assigned to ReadWrite (can't write to readonly source)
// const test2: ReadWrite = new ReadOnly(); // Error

// Test 3: Split accessor with union types
class FlexibleSetter {
  private _x: string;
  
  // Getter returns string
  get x(): string {
    return this._x;
  }
  
  // Setter accepts string | null
  set x(value: string | null) {
    this._x = value ?? '';
  }
}

class StrictGetter {
  get x(): string {
    return 'test';
  }
}

// FlexibleSetter can be assigned to StrictGetter
const test3: StrictGetter = new FlexibleSetter(); // OK

// Test 4: Readonly properties only check read type
class ReadonlyProp {
  readonly x: string = 'test';
}

class MutableProp {
  x: string = 'test';
}

// Mutable can be assigned to readonly
const test4: ReadonlyProp = new MutableProp(); // OK

// Readonly cannot be assigned to mutable
// const test5: MutableProp = new ReadonlyProp(); // Error

// Test 5: Property with different read and write types
class Property1 {
  private _x: string | number;
  
  get x(): string {
    return 'test';
  }
  
  set x(value: string | number) {
    this._x = value;
  }
}

class Property2 {
  private _x: string;
  
  get x(): string {
    return this._x;
  }
  
  set x(value: string) {
    this._x = value;
  }
}

// Property1 can be assigned to Property2
// - Read: string <: string (OK)
// - Write: string <: string | number (OK, contravariant)
const test6: Property2 = new Property1(); // OK

// Property2 cannot be assigned to Property1
// - Read: string <: string (OK)
// - Write: string | number <: string (NOT OK)
// const test7: Property1 = new Property2(); // Error

// Test 6: Optional properties with split accessors
class Optional1 {
  private _x?: string;
  
  get x(): string | undefined {
    return this._x;
  }
  
  set x(value: string | undefined) {
    this._x = value;
  }
}

class Optional2 {
  private _x: string;
  
  get x(): string {
    return this._x;
  }
  
  set x(value: string) {
    this._x = value;
  }
}

// Optional1 (with undefined) cannot be assigned to Optional2 (required)
// const test8: Optional2 = new Optional1(); // Error

// Optional2 can be assigned to Optional1
const test9: Optional1 = new Optional2(); // OK

console.log('Rule #26 split accessor tests passed!');
