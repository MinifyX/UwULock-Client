// The browser's DOMParser for the import tests: KeePass files are XML, which
// the app reads with the WebView's own parser. jsdom stands in for it here,
// as in the web vault's tests.

import { JSDOM } from 'jsdom';

(globalThis as { DOMParser?: unknown }).DOMParser ??= new JSDOM('').window.DOMParser;
