import { strict as assert } from 'assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { readFile } from 'fs/promises';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Executes a single test file with support for async type checking.
 * @param {string} filePath - Path to the file to test
 * @param {object} [options] - Optional configuration
 * @param {boolean} [options.watch=false] - Watch mode
 */
export async function testSingleFile(filePath, options = {}) {
  try {
    const fullPath = join(__dirname, filePath);
    const fileContent = await readFile(fullPath, 'utf-8');
    
    console.log(`Testing file: ${filePath}`);
    
    // Execute the test file (assuming it exports test functions)
    const testModule = await import(fullPath);
    
    let passed = 0;
    let failed = 0;
    const results = [];
    
    for (const [name, test] of Object.entries(testModule)) {
      if (name.startsWith('test') || name.startsWith('spec')) {
        try {
          const result = await test();
          // Handle async type checking results
          if (result instanceof Promise) {
            const resolved = await result;
            if (resolved === undefined || resolved === true) {
              passed++;
              results.push({ name, status: 'passed' });
            } else {
              failed++;
              results.push({ name, status: 'failed', error: 'Test returned falsy value' });
            }
          } else if (result === undefined || result === true) {
            passed++;
            results.push({ name, status: 'passed' });
          } else {
            failed++;
            results.push({ name, status: 'failed', error: 'Test returned falsy value' });
          }
        } catch (error) {
          failed++;
          results.push({ name, status: 'failed', error: error.message });
        }
      }
    }
    
    console.log(`\nResults: ${passed} passed, ${failed} failed`);
    
    // Output detailed results
    for (const result of results) {
      if (result.status === 'failed') {
        console.log(`✗ ${result.name}`);
        console.log(`  ${result.error}`);
      } else {
        console.log(`✓ ${result.name}`);
      }
    }
    
    // Exit with appropriate code
    process.exit(failed > 0 ? 1 : 0);
    
  } catch (error) {
    console.error(`Error testing file ${filePath}:`, error.message);
    process.exit(1);
  }
}

// Example usage:
// await testSingleFile('./example.test.mjs');
```

### Key Features:

1. **Async Support**: Handles both synchronous and asynchronous test functions
2. **Type-Checking Results**: Properly processes test results that might be promises
3. **Clear Output**: Shows pass/fail counts and detailed results
4. **Error Handling**: Catches and reports both file execution errors and test failures
5. **Exit Codes**: Returns appropriate exit codes for CI/CD integration

### Usage Example:

```javascript
// example.test.mjs
export async function testAsyncTypeCheck() {
  const result = await someAsyncOperation();
  return typeof result === 'string'; // Returns true/false for type checking
}

export function testSyncTypeCheck() {
  const value = 42;
  return typeof value === 'number'; // Returns true/false
}
```

You can run tests like this:
```bash
node test-single-file.mjs example.test.mjs
```

Would you like me to modify any specific aspects of this implementation to better match your existing codebase or requirements?
