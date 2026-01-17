// From asyncArrowFunction8_es2017.ts - should emit TS1005 not TS1109
var foo = async (): Promise<void> => {
  var v = { [await]: foo }
}