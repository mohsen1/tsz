interface RegExpMatchArray {
    indices?: RegExpIndicesArray;
}
interface RegExpExecArray {
    indices?: RegExpIndicesArray;
}
interface RegExpIndicesArray extends Array<[number, number] | undefined> {
    groups?: {
        [key: string]: [number, number];
    };
}
interface RegExp {
    readonly hasIndices: boolean;
}
