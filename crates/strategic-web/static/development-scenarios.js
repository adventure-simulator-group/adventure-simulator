(() => {
  const search = document.querySelector("[data-scenario-search]");
  if (!search) return;
  const cards = [...document.querySelectorAll("[data-scenario-card]")];
  const groups = [...document.querySelectorAll("[data-scenario-group]")];
  const filter = () => {
    const needle = search.value.trim().toLowerCase();
    for (const card of cards) {
      card.hidden = needle !== "" && !card.dataset.scenarioSearchText.includes(needle);
    }
    for (const group of groups) {
      group.hidden = !group.querySelector("[data-scenario-card]:not([hidden])");
    }
  };
  search.addEventListener("input", filter);
})();
