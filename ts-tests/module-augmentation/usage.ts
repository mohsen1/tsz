// File 4: Uses the augmented Window interface
// This should work if cross-file interface merging is working correctly

// Use title from file1.ts
declare const window: Window;
const t: string = window.title;

// Use alert from file2.ts
window.alert("test");

// Use location from file3.ts
const loc: string = window.location;

// All three properties should be available due to interface merging
