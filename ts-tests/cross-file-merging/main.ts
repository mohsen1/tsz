// Main file: Uses the augmented Window interface
// This should work if cross-file interface merging is working correctly

declare const window: Window;

// These should all be available because of interface merging across files
window.title;      // from file1.ts
window.alert();    // from file2.ts
window.location;   // from file3.ts
