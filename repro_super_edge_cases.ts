class Base {
    baseMethod() { return "base"; }
    static staticMethod() { return "static base"; }
}

class Derived extends Base {
    // Valid: Nested arrow functions in property initializer
    nestedArrow = () => () => super.baseMethod();

    // Valid: Arrow function in method
    method() {
        const arrow = () => super.baseMethod();
        return arrow();
    }

    // Valid: Arrow function in constructor
    constructor() {
        super();
        const arrow = () => super.baseMethod();
        arrow();
    }
}

// Invalid: Arrow function outside class (should error)
const outsideArrow = () => {
    // This should error because there's no class context
    // But we can't actually test this without a class wrapper
};

// Invalid: Regular function in method (should error)
class InvalidCase extends Base {
    invalidMethod() {
        // Regular function does NOT capture super context
        function regularFunction() {
            // This SHOULD error because regular functions don't capture super
            // But TypeScript's implementation is complex here
            // Let's see what tsz does
        }
    }
}
