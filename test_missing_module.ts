// Test file for TS2307 - Cannot find module
// This should emit TS2307 errors for missing modules

import { foo } from './missing-module';
import { bar } from '../non-existent/path';
import baz from 'non-existent-package';

function test() {
    foo.x;
    bar.y;
    baz.z;
}
