# Intentional Differences from TypeScript

This document documents any intentional differences between TSZ and TypeScript's reference implementation.

## Philosophy

Our goal is 100% conformance with TypeScript's behavior. However, in rare cases, there may be deliberate differences due to:

1. **Implementation limitations**: Temporary limitations that will be addressed in future releases
2. **Architecture differences**: Fundamental differences in how the type checker is implemented
3. **Bug fixes**: Cases where we believe TypeScript's behavior is buggy (with supporting evidence)

## Documented Differences

### None

As of 2026-01-26, there are **no intentional differences** documented. All failing conformance tests represent bugs or missing features that should be fixed.

## Future Additions

If an intentional difference is introduced, it must be documented here with:

- **Error Code**: The TypeScript error code affected
- **Description**: What differs
- **Reasoning**: Why the difference exists
- **Examples**: Code showing the difference
- **Temporary**: Whether this is a temporary limitation or permanent difference
- **Issue**: Link to GitHub issue discussing the difference

## Example Template

```markdown
### TSXXXX: Error Description

**TypeScript Behavior**: Description of what TypeScript does

**TSZ Behavior**: Description of what TSZ does differently

**Reasoning**: Why we differ
- This is a temporary limitation due to [technical reason]
- OR: We believe TypeScript's behavior is incorrect because [evidence]

**Example**:
\`\`\`typescript
// TypeScript error: XXXX
// TSZ error: YYYY
\`\`\`

**Temporary**: Yes/No

**Issue**: #[link]
```

---

**Last Updated**: 2026-01-26
**Total Intentional Differences**: 0
