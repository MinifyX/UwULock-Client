/**
 * Chromium's offscreen document for the clipboard: the background worker has none. It writes
 * what the background sends, and nothing else; it listens to no one but the extension itself.
 */

type Copy = { target: 'offscreen'; type: 'copy'; text: string };

function isCopy(message: unknown): message is Copy {
  const value = message as Partial<Copy> | null;
  return value?.target === 'offscreen' && value.type === 'copy' && typeof value.text === 'string';
}

/** `execCommand('copy')` with the text put into the copy event: works without focus, also for ''. */
function copy(text: string): boolean {
  const onCopy = (event: ClipboardEvent) => {
    event.clipboardData?.setData('text/plain', text);
    event.preventDefault();
  };
  document.addEventListener('copy', onCopy);
  try {
    const area = document.getElementById('clip') as HTMLTextAreaElement;
    area.value = text || ' ';
    area.select();
    return document.execCommand('copy');
  } finally {
    document.removeEventListener('copy', onCopy);
    (document.getElementById('clip') as HTMLTextAreaElement).value = '';
  }
}

chrome.runtime.onMessage.addListener((message: unknown, sender, respond) => {
  if (sender.id !== chrome.runtime.id || !isCopy(message)) return false;
  respond(copy(message.text));
  return false;
});
