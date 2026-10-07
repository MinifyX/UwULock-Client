// The few `expect` matchers the import tests use (they come from the web
// vault, which runs Vitest), on top of node:assert — so the tests run with
// plain `node --test` like the others here.

import assert from 'node:assert/strict';

export { describe, it } from 'node:test';

type Expected = string | RegExp | Error | (new (...args: never[]) => Error) | undefined;

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/** Vitest's `toEqual`: deep, and a key whose value is `undefined` counts as missing. */
function equals(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (a instanceof Uint8Array && b instanceof Uint8Array) {
    return a.length === b.length && a.every((byte, i) => byte === b[i]);
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((value, i) => equals(value, b[i]));
  }
  if (!isObject(a) || !isObject(b)) return false;
  const keys = (o: Record<string, unknown>) => Object.keys(o).filter((k) => o[k] !== undefined);
  const ka = keys(a);
  const kb = keys(b);
  return ka.length === kb.length && ka.every((k) => equals(a[k], b[k]));
}

/** Vitest's `toMatchObject`: every key of `expected`, deep; arrays item by item. */
function matches(actual: unknown, expected: unknown): boolean {
  if (Array.isArray(expected)) {
    return (
      Array.isArray(actual) &&
      actual.length === expected.length &&
      expected.every((value, i) => matches(actual[i], value))
    );
  }
  if (isObject(expected) && !(expected instanceof Uint8Array)) {
    return isObject(actual) && Object.keys(expected).every((k) => matches(actual[k], expected[k]));
  }
  return equals(actual, expected);
}

function checkThrown(error: unknown, expected: Expected) {
  const message = error instanceof Error ? error.message : String(error);
  if (expected === undefined) return;
  if (typeof expected === 'string') {
    assert.ok(message.includes(expected), `"${message}" does not contain "${expected}"`);
  } else if (expected instanceof RegExp) {
    assert.match(message, expected);
  } else if (expected instanceof Error) {
    assert.equal(message, expected.message);
  } else {
    assert.ok(error instanceof expected, `${String(error)} is not a ${expected.name}`);
  }
}

const show = (value: unknown) => {
  try {
    return JSON.stringify(value, null, 1)?.slice(0, 2000);
  } catch {
    return String(value);
  }
};

export function expect(actual: unknown) {
  const matchers = {
    toBe: (expected: unknown) =>
      assert.ok(Object.is(actual, expected), `${show(actual)} !== ${show(expected)}`),
    toEqual: (expected: unknown) =>
      assert.ok(equals(actual, expected), `${show(actual)}\n  is not equal to\n${show(expected)}`),
    toMatchObject: (expected: unknown) =>
      assert.ok(matches(actual, expected), `${show(actual)}\n  does not match\n${show(expected)}`),
    toContain: (expected: unknown) =>
      assert.ok(
        typeof actual === 'string'
          ? actual.includes(String(expected))
          : (actual as unknown[]).includes(expected),
        `${show(actual)} does not contain ${show(expected)}`,
      ),
    toBeNull: () => assert.equal(actual, null),
    toBeUndefined: () => assert.equal(actual, undefined),
    toHaveLength: (length: number) => assert.equal((actual as { length: number }).length, length),
    toBeInstanceOf: (type: new (...args: never[]) => unknown) => assert.ok(actual instanceof type),
    toThrow: (expected?: Expected) => {
      let thrown: { error: unknown } | null = null;
      try {
        (actual as () => unknown)();
      } catch (error) {
        thrown = { error };
      }
      assert.ok(thrown, 'did not throw');
      checkThrown(thrown.error, expected);
    },
  };
  return {
    ...matchers,
    not: {
      toContain: (expected: unknown) =>
        assert.ok(
          !(typeof actual === 'string'
            ? actual.includes(String(expected))
            : (actual as unknown[]).includes(expected)),
          `${show(actual)} contains ${show(expected)}`,
        ),
      toThrow: () => assert.doesNotThrow(actual as () => unknown),
      toBe: (expected: unknown) => assert.ok(!Object.is(actual, expected)),
      toEqual: (expected: unknown) => assert.ok(!equals(actual, expected)),
    },
    rejects: {
      toThrow: async (expected?: Expected) => {
        let thrown: { error: unknown } | null = null;
        try {
          await (actual as Promise<unknown>);
        } catch (error) {
          thrown = { error };
        }
        assert.ok(thrown, 'did not reject');
        checkThrown(thrown.error, expected);
      },
    },
  };
}
