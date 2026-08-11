import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import packageJson from '../package.json';
import { VALID_DIALECTS } from '../src/types';

describe('dialects', () => {
  it('keeps the VS Code setting choices synchronized with runtime dialects', () => {
    const setting = packageJson.contributes.configuration.properties['flowscope.dialect'];

    assert.deepEqual(setting.enum, VALID_DIALECTS);
  });

  it('includes Oracle', () => {
    assert.ok(VALID_DIALECTS.includes('oracle'));
  });
});
