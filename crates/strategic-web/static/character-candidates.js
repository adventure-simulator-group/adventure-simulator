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

  function initialize(document, windowObject) {
    const bootstrap = document.querySelector("[data-candidate-bootstrap]");
    if (bootstrap) {
      try {
        const version = Number(bootstrap.dataset.generatorVersion);
        const value = loadOrCreate(windowObject.sessionStorage, windowObject.crypto, version);
        const ageLinks = document.querySelectorAll("[data-candidate-age]");
        ageLinks.forEach((link) => {
          link.href = `/characters/candidates?version=${value.version}&seed=${value.seed}&age=${link.dataset.candidateAge}`;
        });
        if (ageLinks.length === 0) {
          windowObject.location.replace(`/characters/candidates?version=${value.version}&seed=${value.seed}`);
        }
      } catch (_) {
        bootstrap.outerHTML = '<p class="form-error" role="alert">This browser cannot securely prepare candidates.</p>';
      }
      return;
    }
    const form = document.querySelector("[data-candidate-confirm-form]");
    if (form) form.addEventListener("submit", (event) => { if (!lockSubmit(form)) event.preventDefault(); });
  }
  return { STORAGE_KEY, isSeed, bytesToHex, createSeed, loadOrCreate, lockSubmit, initialize };
});
