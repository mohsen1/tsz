abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() {}
}

function createAnimal(Ctor: typeof Animal): Animal {
    return new Dog();
}

const animal = createAnimal(Animal);
