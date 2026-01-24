// Test various TS2307 scenarios

// Test 1: Relative import - missing file
import { Component } from './missing/Component';
import * as utils from './utils/index';

// Test 2: Bare specifier - missing package
import React from 'non-existent-react';
import { useState } from 'missing-package';

// Test 3: Scoped package - missing
import { foo } from '@scope/missing-package';

// Test 4: Parent directory import - missing
import { config } from '../config';

// Test 5: Default import - missing
import MissingModule from './MissingModule';

// Test 6: Namespace import - missing
import * as MissingNs from './missing-ns';

// Test 7: Type-only import - missing
import type { MissingType } from './missing-types';

// Test 8: Mixed type and value import - missing
import { MissingType, MissingValue } from './mixed-missing';

// Test 9: Deep nested import - missing
import { deep } from '../../deeply/nested/path';

// Test 10: Re-export from missing module
export { Something } from './missing-source';

// Test 11: Export * from missing
export * from './missing-wildcard';

// Using the imports to trigger type checking
function testUsage() {
    const c = new Component();
    const state = useState(0);
    const f = foo();
    const cfg = config();
    const m = new MissingModule();
    const u = MissingNs.helper();
    const t: MissingType = null;
    const v = new MissingValue();
    const d = deep.value;
}
