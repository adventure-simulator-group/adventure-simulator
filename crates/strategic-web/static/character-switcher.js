(() => {
  const loadOptions = async (root) => {
    if (root.dataset.characterSwitcherLoaded) return;
    root.dataset.characterSwitcherLoaded = "true";
    try {
      const response = await window.strategicBackgroundFetch(
        "character-switcher",
        root.dataset.characterSwitcherUrl,
        { headers: { Accept: "text/html" } },
      );
      if (!response.ok) throw new Error(`Character menu returned ${response.status}`);
      root.innerHTML = await response.text();
    } catch (error) {
      delete root.dataset.characterSwitcherLoaded;
      root.innerHTML = '<p class="character-switcher-empty">Adventurers unavailable.</p>';
      window.reportStrategicError?.(error, "character switcher");
    }
  };

  const mount = (scope = document) => {
    scope.querySelectorAll("[data-character-switcher-options]").forEach((root) => {
      loadOptions(root);
    });
  };

  mount();
  document.addEventListener("strategic-page-mounted", (event) => mount(event.target));
})();
