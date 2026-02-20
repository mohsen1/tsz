---
name: Disable toLocaleString compatibility check
description: This skill disables the toLocaleString compatibility check in crates/tsz-checker/src/type_computation_call.rs.
---

To disable the toLocaleString compatibility check, replace the contents of the `is_tolocalestring_compat_call` function with `true` in `crates/tsz-checker/src/type_computation_call.rs`.

```rust
    fn is_tolocalestring_compat_call(&self, _callee_expr: NodeIndex, _arg_count: usize) -> bool {
        true
    }
```