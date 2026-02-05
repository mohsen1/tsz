// Test switch statement narrowing

type Shape =
    | { kind: 'circle', radius: number }
    | { kind: 'square', side: number };

function area(shape: Shape): number {
    switch (shape.kind) {
        case 'circle':
            return Math.PI * shape.radius ** 2;
        case 'square':
            return shape.side ** 2;
    }
    // Error: Not all code paths return a value (if noImplicitReturns)
}

function area2(shape: Shape): number {
    switch (shape.kind) {
        case 'circle':
            return Math.PI * shape.radius ** 2;
        default:
            const exhaustive: never = shape;
            return exhaustive;
    }
}
