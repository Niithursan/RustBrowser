// Rust Browser's form helper on Linux. It runs in its own script world, so the
// page's scripts can neither see it nor send its messages. It reports whether
// the page has a login or contact form, sends a submitted login or contact
// details to the browser, and fills a form only when the browser calls
// rustBrowserFillLogin or rustBrowserFillContact after the user chose to.
(() => {
  'use strict';
  const CONTACT = new Set([
    'name', 'given-name', 'additional-name', 'family-name', 'email', 'tel',
    'organization', 'street-address', 'address-line1', 'address-line2',
    'address-level1', 'address-level2', 'postal-code', 'country', 'country-name',
  ]);
  const post = (message) => {
    try {
      window.webkit.messageHandlers.rustBrowserForms.postMessage(JSON.stringify(message));
    } catch (_) {
      // The browser is not listening (a private window): nothing is sent.
    }
  };
  const visible = (el) => el.isConnected && !el.disabled && el.type !== 'hidden'
    && el.getClientRects().length > 0;
  // A contact field's kind, from its autocomplete token, or email and phone
  // fields by type.
  const kind = (el) => {
    const parts = (el.getAttribute('autocomplete') || '').toLowerCase().trim().split(/\s+/);
    const token = parts[parts.length - 1];
    if (CONTACT.has(token)) return token;
    if (!parts[0] && el.type === 'email') return 'email';
    if (!parts[0] && el.type === 'tel') return 'tel';
    return '';
  };
  const passwords = (root) => [...root.querySelectorAll('input[type=password]')].filter(visible);
  const usernameFor = (pw) => {
    const inputs = [...(pw.form || document).querySelectorAll('input')].filter(visible);
    const hinted = inputs.find((el) => /\busername\b/.test((el.getAttribute('autocomplete') || '').toLowerCase()));
    if (hinted) return hinted;
    for (let i = inputs.indexOf(pw) - 1; i >= 0; i -= 1) {
      if (['text', 'email', 'tel'].includes(inputs[i].type)) return inputs[i];
    }
    return null;
  };
  const contactFields = (root) => [...root.querySelectorAll('input, select, textarea')]
    .filter((el) => visible(el) && kind(el));

  let shown = '';
  const report = () => {
    const login = passwords(document).length > 0;
    const contact = !login && contactFields(document).length > 0;
    const signature = `${login},${contact}`;
    if (signature === shown || !(login || contact)) return;
    shown = signature;
    post({ kind: 'fields', origin: location.origin, login, contact });
  };

  let sent = '';
  // `loginOnly`: when the page goes away, a typed login is offered, but
  // contact details are only ever taken from a submitted form.
  const capture = (root, loginOnly) => {
    const pw = passwords(root).find((el) => el.value);
    if (pw) {
      const user = usernameFor(pw);
      const username = user ? user.value.trim() : '';
      const key = `${username}\n${pw.value}`;
      if (key !== sent && pw.value.length <= 1024 && username.length <= 256) {
        sent = key;
        post({ kind: 'login', origin: location.origin, username, password: pw.value });
      }
      return;
    }
    if (loginOnly) return;
    const values = {};
    for (const el of contactFields(root)) {
      const value = (el.value || '').trim();
      if (value && value.length <= 256) values[kind(el)] = value;
    }
    if (Object.keys(values).length) post({ kind: 'contact', origin: location.origin, values });
  };
  document.addEventListener('submit', (e) => {
    capture(e.target instanceof HTMLFormElement ? e.target : document, false);
  }, true);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target instanceof HTMLInputElement && e.target.type === 'password') {
      capture(e.target.form || document, true);
    }
  }, true);
  window.addEventListener('pagehide', () => capture(document, true), true);
  document.addEventListener('focusin', report, true);
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', report);
  else report();

  // Set a value as typing would, so the page's own scripts see it.
  const setValue = (el, value) => {
    const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype
      : el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  window.rustBrowserFillLogin = (username, password) => {
    const focused = document.activeElement;
    const pw = (focused instanceof HTMLInputElement && focused.type === 'password' && visible(focused))
      ? focused : passwords(document)[0];
    if (!pw) return false;
    const user = usernameFor(pw);
    if (user && username) setValue(user, username);
    setValue(pw, password);
    return true;
  };
  window.rustBrowserFillContact = (json) => {
    const values = JSON.parse(json);
    let filled = 0;
    for (const el of contactFields(document)) {
      const value = values[kind(el)];
      if (typeof value === 'string' && !el.value) {
        setValue(el, value);
        filled += 1;
      }
    }
    return filled;
  };
})();
