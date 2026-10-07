// The AutoFill extension (apps/desktop/src-tauri/apple/PasskeyProvider) is Swift no test runs on
// this machine; CI only compiles it. These checks read the source and fail when one of the
// properties that keep its sheet from staying empty goes missing — node --test.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const dir = new URL('../src-tauri/apple/PasskeyProvider/', import.meta.url);
/** The text without `//` line comments, so a property only mentioned in a comment doesn't count. */
const code = (name: string) =>
  readFileSync(new URL(name, dir), 'utf8')
    .split('\n')
    .map((line) => line.replace(/^\s*\/\/.*$/, ''))
    .join('\n');

const controller = code('CredentialProviderViewController.swift');

test('the sheet is built when the view loads, pinned with constraints, never from bounds', () => {
  assert.match(
    controller,
    /override func viewDidLoad\(\) \{\s*super\.viewDidLoad\(\)\s*install\(\)/,
  );
  assert.match(controller, /translatesAutoresizingMaskIntoConstraints = false/);
  assert.match(controller, /NSLayoutConstraint\.activate/);
  assert.doesNotMatch(controller, /host\.view\.frame = view\.bounds/);
  assert.doesNotMatch(controller, /autoresizingMask/);
  // "Abbrechen" is part of the view from the start.
  assert.match(controller, /Button\(tr\("Abbrechen", "Cancel"\), role: \.cancel\)/);
});

test('every way in the system may take is answered', () => {
  for (const entry of [
    /override func provideCredentialWithoutUserInteraction\(for credentialRequest: ASCredentialRequest\)/,
    /override func provideCredentialWithoutUserInteraction\(\s*for credentialIdentity: ASPasswordCredentialIdentity/,
    /override func prepareInterfaceToProvideCredential\(for credentialRequest: ASCredentialRequest\)/,
    /override func prepareInterfaceToProvideCredential\(\s*for credentialIdentity: ASPasswordCredentialIdentity/,
    /override func prepareCredentialList\(for serviceIdentifiers: \[ASCredentialServiceIdentifier\]\)/,
    /override func prepareCredentialList\(\s*for serviceIdentifiers: \[ASCredentialServiceIdentifier\],\s*requestParameters/,
    /@available\(iOS 18\.0, macOS 15\.0, \*\)\s*override func prepareOneTimeCodeCredentialList/,
    /@available\(iOS 18\.0, macOS 15\.0, \*\)\s*override func prepareInterfaceForUserChoosingTextToInsert/,
    /override func prepareInterface\(forPasskeyRegistration/,
    /override func prepareInterfaceForExtensionConfiguration/,
  ])
    assert.match(controller, entry);
});

test('Face ID waits for the sheet, retries a refusal and then offers a tap', () => {
  assert.match(controller, /override func viewDidAppear/);
  assert.match(controller, /whenOnScreen \{/);
  assert.match(controller, /case \.notInteractive\?:\s*again\(\)/);
  assert.match(controller, /private static let backoff: \[Double\]/);
  assert.match(controller, /unlockButton = Self\.unlockLabel\(\)/);
  assert.match(controller, /private func watch\(/);
});

test('the protocol never gets a secret', () => {
  const sources = [
    'CredentialProviderViewController.swift',
    'PasskeyVault.swift',
    'AutoFillLog.swift',
  ]
    .map(code)
    .join('\n');
  const steps = [...sources.matchAll(/logStep\(([\s\S]*?)\)\s*$/gm)].map((m) => m[1] ?? '');
  assert.ok(steps.length > 20);
  for (const step of steps) {
    assert.doesNotMatch(
      step,
      /\\\((password|key|signature|clientDataHash|userName|login\.userName|identity\.user\b)\)/,
      step,
    );
    assert.doesNotMatch(step, /serviceIdentifier\.identifier|service\.identifier/, step);
  }
  assert.match(code('AutoFillLog.swift'), /PasskeyVault\.writing/);
});
