(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  else api.initialize(root.document, root);
})(typeof window !== "undefined" ? window : globalThis, function () {
  "use strict";
  const STORAGE_KEY = "adventuresim.starting-candidates";
  const isSeed = (seed) => typeof seed === "string" && /^[0-9a-f]{32}$/.test(seed);
  const bytesToHex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

  function createSeed(cryptoObject) {
    if (!cryptoObject || typeof cryptoObject.getRandomValues !== "function") throw new Error("secure random generation unavailable");
    const bytes = new Uint8Array(16);
    cryptoObject.getRandomValues(bytes);
    return bytesToHex(bytes);
  }

  function loadOrCreate(storage, cryptoObject, version) {
    try {
      const parsed = JSON.parse(storage.getItem(STORAGE_KEY));
      if (parsed && parsed.version === version && isSeed(parsed.seed)) return parsed;
    } catch (_) { /* corrupt or unavailable storage is replaced */ }
    const value = { version, seed: createSeed(cryptoObject) };
    try { storage.setItem(STORAGE_KEY, JSON.stringify(value)); } catch (_) { /* URL carries the fallback */ }
    return value;
  }

  function lockSubmit(form) {
    if (form.dataset.submitted === "true") return false;
    form.dataset.submitted = "true";
    form.querySelectorAll("button[type=submit]").forEach((button) => { button.disabled = true; });
    return true;
  }

  function focusables(dialog) { return Array.from(dialog.querySelectorAll('a[href], button:not([disabled]), input:not([type="hidden"])')); }
  function initialize(document, windowObject) {
    const bootstrap = document.querySelector("[data-candidate-bootstrap]");
    if (bootstrap) {
      try {
        const version = Number(bootstrap.dataset.generatorVersion);
        const value = loadOrCreate(windowObject.sessionStorage, windowObject.crypto, version);
        windowObject.location.replace(`/characters/candidates?version=${value.version}&seed=${value.seed}`);
      } catch (_) {
        bootstrap.outerHTML = '<p class="form-error" role="alert">This browser cannot securely prepare candidates.</p>';
      }
      return;
    }
    document.querySelectorAll(".candidate-portrait").forEach((portrait) => portrait.addEventListener("click", () => {
      try { windowObject.sessionStorage.setItem("adventuresim.candidate-opener", portrait.getAttribute("href")); } catch (_) {}
    }));
    const dialog = document.querySelector("[data-candidate-dialog]");
    if (!dialog) return;
    const close = dialog.querySelector("[data-candidate-dialog-close]");
    const form = dialog.querySelector("[data-candidate-confirm-form]");
    form.addEventListener("submit", (event) => { if (!lockSubmit(form)) event.preventDefault(); });
    dialog.addEventListener("keydown", (event) => {
      if (event.key === "Escape") { event.preventDefault(); close.click(); return; }
      if (event.key !== "Tab") return;
      const controls = focusables(dialog); if (!controls.length) return;
      const first = controls[0], last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    });
    dialog.focus();
  }
  return { STORAGE_KEY, isSeed, bytesToHex, createSeed, loadOrCreate, lockSubmit, initialize };
});
