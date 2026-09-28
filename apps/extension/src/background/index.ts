/**
 * The background: a service worker in Chromium, an event page in Firefox. It holds the keys
 * and the open vault, talks to the server, and answers the popup, the prompt window and the
 * content scripts.
 *
 * Who asks is checked on every message: the extension's own pages may ask for anything, a
 * content script only for what its page may have (`content:*`, see autofill.ts), and nothing
 * outside the extension reaches here at all (no `onMessageExternal`). Every listener is added
 * right away, at the top level, as Manifest V3 wants it: a worker that was ended is started
 * again by the event it missed.
 */

import { ext } from '../shared/browser';
import type { ContentRequest, PageRequest, Reply } from '../shared/protocol';
import * as autofill from './autofill';
import * as clipboard from './clipboard';
import { onChanged } from './events';
import * as generator from './generator';
import { failure, request } from './http';
import * as live from './live';
import * as menus from './menus';
import * as passkeys from './passkeys';
import * as session from './session';
import { settings, updateSettings } from './settings';
import { activeAccount, closeSessionToContentScripts } from './store';
import * as vault from './vault';

type Sender = chrome.runtime.MessageSender;

const MINUTE = 'minute';

function ownPrefix(): string {
  return ext.runtime.getURL('/');
}

/** The popup, the prompt window, the offscreen document: pages of this extension. */
function isOwnPage(sender: Sender): boolean {
  return (
    sender.id === ext.runtime.id &&
    typeof sender.url === 'string' &&
    sender.url.startsWith(ownPrefix())
  );
}

/** A content script of this extension, in a web page. */
function isContentScript(sender: Sender): boolean {
  return (
    sender.id === ext.runtime.id &&
    sender.tab?.id !== undefined &&
    typeof sender.url === 'string' &&
    /^https?:\/\//.test(sender.url)
  );
}

async function currentAccount() {
  const found = await activeAccount();
  if (!found) throw { kind: 'session-expired', message: 'Log in first.' };
  return found;
}

async function handlePage(message: PageRequest): Promise<unknown> {
  switch (message.type) {
    case 'status':
      return session.status();
    case 'login':
      return session.login(message.server, message.email, message.password);
    case 'login-two-factor':
      return session.loginTwoFactor(message.provider, message.code, message.remember);
    case 'login-webauthn':
      return session.loginWebAuthn(message.remember);
    case 'login-new-device':
      return session.loginNewDevice(message.code);
    case 'login-send-email':
      return session.loginSendEmail();
    case 'login-cancel':
      return session.loginCancel();
    case 'unlock':
      return session.unlock(message.password);
    case 'unlock-pin':
      return session.unlockWithPin(message.pin);
    case 'set-pin':
      return session.setPin(message.pin, message.afterRestart);
    case 'lock':
      await lock();
      return session.status();
    case 'logout':
      return session.logout(message.id);
    case 'switch-account':
      return session.switchAccount(message.id);
    case 'touch':
      return session.touch();
    case 'sync': {
      await vault.sync(await session.requireUnlocked());
      return session.status();
    }
    case 'overview':
      await session.requireUnlocked();
      return vault.overview();
    case 'items':
      await session.requireUnlocked();
      return vault.items();
    case 'item':
      return vault.item(message.id);
    case 'reveal':
      return vault.reveal(message.id, message.field);
    case 'copy':
      await clipboard.copy(await vault.reveal(message.id, message.field));
      return null;
    case 'copy-text':
      await clipboard.copy(message.text);
      return null;
    case 'totp':
      return vault.totp(message.id);
    case 'verify-reprompt':
      return vault.verifyReprompt(message.id, message.password);
    case 'save-item':
      await session.requireUnlocked();
      return vault.saveItem(message.id, message.draft);
    case 'set-favorite':
      await session.requireUnlocked();
      return vault.setFavorite(message.id, message.favorite);
    case 'delete-item':
      await session.requireUnlocked();
      return vault.deleteItem(message.id, message.permanent);
    case 'restore-item':
      await session.requireUnlocked();
      return vault.restoreItem(message.id);
    case 'save-folder':
      await session.requireUnlocked();
      return vault.saveFolder(message.id, message.name);
    case 'delete-folder':
      await session.requireUnlocked();
      return vault.deleteFolder(message.id);
    case 'open-uri': {
      const uri = await vault.reveal(message.id, `uri:${message.index}`);
      if (/^https?:\/\//i.test(uri)) await ext.tabs.create({ url: uri });
      return null;
    }
    case 'generate':
      return generator.generateWithBits(message.settings);
    case 'generator-history':
      return generator.history();
    case 'clear-generator-history':
      return generator.clearHistory();
    case 'settings':
      return settings();
    case 'set-settings': {
      const next = await updateSettings(message.patch);
      void menus.refreshMenus();
      return next;
    }
    case 'tab-items':
      return autofill.tabItems();
    case 'fill-tab':
      await session.requireUnlocked();
      return autofill.fillTab(message.id, message.confirmedInsecure);
    case 'pending-saves':
      return autofill.pendingSaves();
    case 'answer-pending-save':
      return autofill.answerPendingSave(message.id, message.answer);
    case 'passkey-prompt':
      return passkeys.prompt(message.id);
    case 'passkey-decide':
      return passkeys.decide(message.decision);
  }
  throw { kind: 'invalid', message: 'Unknown request.' };
}

async function handleContent(message: ContentRequest, sender: Sender): Promise<unknown> {
  switch (message.type) {
    case 'content:page-info':
      return autofill.pageInfo(sender);
    case 'content:fill':
      return autofill.fill(
        sender,
        message.itemId,
        message.token,
        message.confirmedInsecure === true,
      );
    case 'content:submitted':
      return autofill.submitted(sender, message.username, message.password, message.newPassword);
    case 'content:pending-prompt':
      return autofill.pendingPrompt(sender);
    case 'content:prompt-answer':
      return autofill.promptAnswer(sender, message.id, message.answer);
    case 'content:copy-totp':
      return autofill.copyTotp(sender, message.itemId);
    case 'content:open-popup':
      return session.openPopup();
    case 'content:webauthn-result':
      return session.webAuthnResult(sender.url, sender.tab?.id, message.data);
    case 'content:passkey-create':
      return passkeys.create(sender, message.requestId, message.options);
    case 'content:passkey-get':
      return passkeys.get(sender, message.requestId, message.options);
    case 'content:passkey-abort':
      return passkeys.abort(message.requestId);
  }
  throw { kind: 'invalid', message: 'Unknown request.' };
}

/** Strings from a content script are the page's: checked for shape before anything reads them. */
function isContentRequest(message: unknown): message is ContentRequest {
  const type = (message as { type?: unknown } | null)?.type;
  return typeof type === 'string' && type.startsWith('content:');
}

function isPageRequest(message: unknown): message is PageRequest {
  const type = (message as { type?: unknown } | null)?.type;
  return typeof type === 'string' && !type.startsWith('content:') && !type.startsWith('bg:');
}

ext.runtime.onMessage.addListener(
  (message: unknown, sender: Sender, respond: (reply: Reply<unknown>) => void) => {
    // The clipboard's offscreen document has its own messages; they are not for here.
    if ((message as { target?: unknown } | null)?.target === 'offscreen') return false;
    let work: (() => Promise<unknown>) | null = null;
    if (isOwnPage(sender) && isPageRequest(message)) {
      work = async () => {
        if (message.type !== 'status' && message.type !== 'touch') await session.touch();
        return handlePage(message);
      };
    } else if (isContentScript(sender) && isContentRequest(message)) {
      work = () => handleContent(message, sender);
    }
    if (!work) return false;
    const run = work;
    void (async () => {
      try {
        await session.restored;
        respond({ ok: true, value: (await run()) ?? null });
      } catch (error) {
        respond({ ok: false, error: failure(error) });
      }
    })();
    return true;
  },
);

// ── Locking ───────────────────────────────────────────────

async function lock() {
  // What was copied from the vault goes with it.
  if (await ext.alarms.get('clear-clipboard')) await clipboard.clear();
  await session.lock();
}

// The popup keeps a port open while it is shown: "lock when the popup closes" hears it close.
ext.runtime.onConnect.addListener((port) => {
  if (port.name !== 'popup' || !port.sender || !isOwnPage(port.sender)) return;
  port.onDisconnect.addListener(() => {
    void (async () => {
      await session.restored;
      const { lockTimeout } = await settings();
      if (lockTimeout === 0) await lock();
      else await session.checkTimeout(lockTimeout);
    })();
  });
});

ext.alarms.onAlarm.addListener((alarm) => {
  void (async () => {
    await session.restored;
    if (clipboard.isClearAlarm(alarm.name)) {
      await clipboard.clear();
      return;
    }
    if (alarm.name !== MINUTE) return;
    await session.checkTimeout((await settings()).lockTimeout);
    const unlocked = session.unlockedAccountId();
    if (!unlocked) return;
    const found = await activeAccount();
    if (!found) return;
    // The live connection brings changes at once; without it, ask every minute.
    live.reconnect();
    if (!live.connected()) await vault.syncIfChanged(found).catch(() => undefined);
  })();
});

// ── The hub ───────────────────────────────────────────────

live.onUpdates({
  sync: () => {
    void (async () => {
      const found = await activeAccount();
      if (found && session.unlockedAccountId() === found.id)
        await vault.sync(found).catch(() => undefined);
    })();
  },
  // The account's sessions were ended somewhere: a request tells whether ours was too, and
  // if so, the refresh that fails closes the vault.
  logout: () => {
    void (async () => {
      const found = await currentAccount().catch(() => null);
      if (found) await request(found, '/api/accounts/revision-date').catch(() => undefined);
    })();
  },
});

// ── Menus, shortcut, badge ────────────────────────────────

ext.contextMenus.onClicked.addListener((info, tab) => {
  void session.restored.then(() => menus.onMenuClick(info, tab)).catch(() => undefined);
});

ext.commands.onCommand.addListener((command, tab) => {
  void session.restored.then(() => menus.onCommand(command, tab)).catch(() => undefined);
});

ext.tabs.onActivated.addListener(({ tabId }) => menus.onTabActivated(tabId));
autofill.onTopFrameChange(() => menus.refreshMenus());
onChanged(() => menus.refreshMenus());

// ── Start ─────────────────────────────────────────────────

/** Every time the background starts: a worker that was ended and woken again, too. */
async function wake() {
  await closeSessionToContentScripts();
  if (!(await ext.alarms.get(MINUTE))) await ext.alarms.create(MINUTE, { periodInMinutes: 1 });
}

/** Installed, updated, or the browser started: the menus are built new (they outlive a worker). */
async function start() {
  await wake();
  await session.restored;
  await menus.refreshMenus();
}

ext.runtime.onInstalled.addListener(() => void start());
ext.runtime.onStartup.addListener(() => void start());
void wake();
