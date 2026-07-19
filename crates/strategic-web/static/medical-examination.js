(() => {
  const examination = document.querySelector("[data-medical-examination]");
  if (!examination) return;

  let resolved = false;
  examination.querySelectorAll("form").forEach((form) => {
    form.addEventListener("submit", () => {
      resolved = true;
    });
  });

  window.addEventListener("pagehide", () => {
    if (!resolved && examination.dataset.dismissUrl) {
      navigator.sendBeacon(examination.dataset.dismissUrl, new Blob([], {
        type: "application/x-www-form-urlencoded",
      }));
    }
  }, { once: true });
})();
