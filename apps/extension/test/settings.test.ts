// @vitest-environment node
/** The extension's settings as the background keeps them: only known values get in. */

import { beforeEach, describe, expect, it, vi } from 'vitest';

const local: Record<string, unknown> = {};
vi.mock('../src/background/store', () => ({
  local: async (key: string) => local[key],
  setLocal: async (key: string, value: unknown) => void (local[key] = value),
}));

const { cleanPassword, DEFAULT_SETTINGS, settings, updateSettings } =
  await import('../src/background/settings');

beforeEach(() => {
  for (const key of Object.keys(local)) delete local[key];
});

describe('the font', () => {
  it('is UwU Sans until another one is picked, and only a known one', async () => {
    expect((await settings()).font).toBe('uwu');
    expect((await updateSettings({ font: 'rubik' })).font).toBe('rubik');
    expect(
      (await updateSettings({ font: 'comic' as unknown as typeof DEFAULT_SETTINGS.font })).font,
    ).toBe('rubik');
  });
});

describe('the generator’s minimums', () => {
  const base = DEFAULT_SETTINGS.generator.password;

  it('are kept as whole numbers', () => {
    const next = cleanPassword(base, { minNumber: 3.6, minSpecial: 2, length: 4 });
    expect(next).toMatchObject({ minNumber: 3, minSpecial: 2, length: 5 });
  });

  it('are dropped when together they can’t fit into 128 characters', () => {
    const next = cleanPassword(base, { minNumber: 100, minSpecial: 100 });
    expect(next.minNumber).toBeUndefined();
    expect(next.minSpecial).toBeUndefined();
    expect(next.length).toBe(base.length);
  });

  it('are saved with the other options', async () => {
    const saved = await updateSettings({
      generator: {
        ...DEFAULT_SETTINGS.generator,
        password: { ...base, minUppercase: 4, symbols: false },
      },
    });
    expect(saved.generator.password).toMatchObject({ minUppercase: 4, symbols: false });
  });
});
