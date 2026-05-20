interface RegExpMatchArray {
    indices?: RegExpIndicesArray;
}
interface RegExpExecArray {
    indices?: RegExpIndicesArray;
}
interface RegExpIndicesArray extends Array<[number, number]> {
    groups?: {
        [key: string]: [number, number];
    };
}
interface RegExp {
    readonly hasIndices: boolean;
}
