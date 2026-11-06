import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parse, stringify } from 'lossless-json';
test('monetary integers preserve every digit', () => {
  const text = '{"msatoshi":9007199254740993}';
  assert.equal(stringify(parse(text)), text);
});
