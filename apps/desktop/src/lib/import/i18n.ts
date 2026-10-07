/**
 * The import module's words. It is plain TypeScript that Node runs as it is
 * (`apps/desktop/test/import-*.test.ts`), so it can't load the app's
 * catalogue (`lib/i18n.ts` reads the settings). Strings are German and wrapped
 * in `t()` like everywhere else, so `scripts/check-i18n.mjs` finds them; the
 * app hands its own `t` over with `setTranslator` before the first import
 * (`lib/import/run.ts`), and until then — in the tests — the German stays.
 */

type Vars = Record<string, string | number>;

const fill = (text: string, vars?: Vars) =>
  vars
    ? text.replace(/\{(\w+)\}/g, (whole, name: string) =>
        name in vars ? String(vars[name]) : whole,
      )
    : text;

let translator: (text: string, vars?: Vars) => string = fill;

export function setTranslator(translate: (text: string, vars?: Vars) => string) {
  translator = translate;
}

export function t(text: string, vars?: Vars): string {
  return translator(text, vars);
}

export function N_(text: string): string {
  return text;
}
