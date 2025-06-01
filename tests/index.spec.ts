import {expect, test} from 'vitest'
import {getVirtualization} from '../index.js'

test('it works', () => {
    const result = getVirtualization();
    console.log(result);
    expect(result).toBeDefined();
    expect(result.cpuSupported).toBeTypeOf('boolean');
    expect(result.osReportedEnabled).toBeTypeOf('boolean');
    expect(result.os).toBeOneOf(['windows', 'linux', 'macos']);
    expect(result.arch).toBeOneOf(['x86_64', 'aarch64']);
})
