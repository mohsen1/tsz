interface Point {
    x: number;
}
const point: Point = { x: 1 };
function read(value: Point): number {
    return value.x;
}
read(point);
