abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() { console.log("Woof!"); }
}

function createAnimal(Ctor: typeof Animal): Animal {
    return new Dog();
}
