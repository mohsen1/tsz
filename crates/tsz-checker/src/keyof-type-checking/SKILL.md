---
name: keyof-type-checking
description: This skill adds the TS2322 error to the checkJsObjectLiteralHasCheckedKeyof.ts test case when a string literal is not assignable to a keyof type.
---

# Keyof Type Checking

## Overview

This skill adds the TS2322 error to the checkJsObjectLiteralHasCheckedKeyof.ts test case when a string literal is not assignable to a keyof type.

## Instructions

1.  Modify the `is_assignable_to` function in `crates/tsz-checker/src/assignability_checker.rs`.
2.  Check if the target type is a `keyof` type.
3.  If the target is a `keyof` type, get the allowed keys using the `get_keyof_type` function.
4.  Check if the source type is a string literal.
5.  If the source is a string literal, check if its value is present in the allowed keys.
6.  If the value is not present, emit a TS2322 error.






