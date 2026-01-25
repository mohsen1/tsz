// Test cases for TS2507 constructor type fixes

// Test 1: Class expression in variable used in extends clause
const BaseClass = class {
    constructor() {
        this.x = 1;
    }
    method() {
        return this.x;
    }
};

class Derived extends BaseClass {
    // Should NOT error TS2507 - BaseClass is a valid constructor
    y = 2;
    getSum() {
        return this.x + this.y;
    }
}

// Test 2: Function with prototype property
function Foo() {
    this.value = 42;
}
Foo.prototype.getValue = function() {
    return this.value;
};

const foo = new Foo(); // Should NOT error TS2507
console.log(foo.getValue());

// Test 3: Generic class expression
const GenericBase = class<T> {
    constructor(public value: T) {}
    getValue(): T {
        return this.value;
    }
};

class StringWrapper extends GenericBase<string> {
    // Should NOT error TS2507
    toUpper() {
        return this.value.toUpperCase();
    }
}

// Test 4: Mixin pattern with class expressions
type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = Date.now();
    };
}

function Activatable<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        isActive = false;
        activate() {
            this.isActive = true;
        }
        deactivate() {
            this.isActive = false;
        }
    };
}

class User {
    constructor(public name: string) {}
}

const TimestampedUser = Timestamped(User);
const ActivatableTimestampedUser = Activatable(TimestampedUser);

const user = new ActivatableTimestampedUser("Alice"); // Should NOT error TS2507
user.activate();
console.log(user.timestamp);

// Test 5: Class expression with extends
const Animal = class {
    constructor(public name: string) {}
    move(distance: number) {
        console.log(`${this.name} moved ${distance}m`);
    }
};

const Dog = class extends Animal {
    bark() {
        console.log("Woof!");
    }
};

const dog = new Dog("Buddy"); // Should NOT error TS2507
dog.bark();
dog.move(10);

// Test 6: Constructor type with generic constraint
interface Constructor<T> {
    new(...args: any[]): T;
}

function createInstance<T>(ctor: Constructor<T>, ...args: any[]): T {
    return new ctor(...args); // Should NOT error TS2507
}

class MyClass {
    constructor(public value: number) {}
}

const instance = createInstance(MyClass, 42); // Should NOT error TS2507

// Test 7: Intersection of constructor types
type MixinConstructor = Constructor & { prototype: { mixinMethod(): void } };

const mixinCtor: MixinConstructor = class {
    mixinMethod() {
        console.log("Mixed in!");
    }
};

class UsingMixin extends (mixinCtor as any) {
    // Should NOT error TS2507
    ownMethod() {
        console.log("Own method");
    }
}

// Test 8: Class expression returned from function
function createBaseClass() {
    return class {
        constructor(public id: number) {}
        getId() {
            return this.id;
        }
    };
}

const DynamicBase = createBaseClass();

class DerivedDynamic extends DynamicBase {
    // Should NOT error TS2507
    name = "derived";
}

const derived = new DerivedDynamic(1);
console.log(derived.getId(), derived.name);

// Test 9: Generic class with type parameters in extends
class Container<T> {
    constructor(public item: T) {}
}

const StringContainer = class extends Container<string> {
    // Should NOT error TS2507
    toString() {
        return this.item;
    }
};

const strContainer = new StringContainer("hello");
console.log(strContainer.toString());

// Test 10: Anonymous class in extends
class Point {
    constructor(public x: number, public y: number) {}
}

class Point3D extends (class extends Point {
    constructor(x: number, y: number, public z: number) {
        super(x, y);
    }
}) {
    // Should NOT error TS2507
    distance() {
        return Math.sqrt(this.x ** 2 + this.y ** 2 + this.z ** 2);
    }
}

const p3d = new Point3D(1, 2, 3);
console.log(p3d.distance());
