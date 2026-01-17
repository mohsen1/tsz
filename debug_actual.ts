// Test the actual file content from asyncArrowFunction8_es2017.ts
// @target: es2017
// @noEmitHelpers: true

var foo = async (): Promise<void> => {
  var v = { [await]: foo }
}