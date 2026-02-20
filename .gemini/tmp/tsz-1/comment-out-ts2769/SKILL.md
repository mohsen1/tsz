---
name: comment-out-ts2769
description: This skill comments out the code block that removes TS2769 for arrayToLocaleStringES2015.ts in crates/conformance/src/tsz_wrapper.rs. Use when Gemini CLI needs to prevent the removal of TS2769 for this specific test case.
---

# Comment out the TS2769 removal

To comment out the code block that removes TS2769 for arrayToLocaleStringES2015.ts in crates/conformance/src/tsz_wrapper.rs, use the following replace commands:

```tool_code
replace(
    file_path = "crates/conformance/src/tsz_wrapper.rs",
    instruction = "Comment out the code block that removes TS2769 for arrayToLocaleStringES2015.ts to see if it fixes the TS2345 error.",
    new_string = "                   // Narrow temporary normalization: this test currently produces a single\n                   // false-positive TS2769 in tsz while tsc emits none.\n                   // if path\n                   //     .to_string_lossy()\n                   //     .replace('\\\', "/")\n                   //     .ends_with(\"arrayToLocaleStringES2015.ts\")\n                   // {\n                   //     all_codes.remove(&2769);\n                   //     all_fingerprints.retain(|fp| fp.code != 2769);\n                   // }",
    old_string = "                   // Narrow temporary normalization: this test currently produces a single\n                    // false-positive TS2769 in tsz while tsc emits none.\n                    if path\n                        .to_string_lossy()\n                        .replace('\\\', "/")\n                        .ends_with(\"arrayToLocaleStringES2015.ts\")\n                    {
                        all_codes.remove(&2769);
                        all_fingerprints.retain(|fp| fp.code != 2769);
                    }"
)
```
