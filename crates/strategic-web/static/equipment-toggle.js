(() => {
  document.addEventListener('change', async (event) => {
    const checkbox = event.target.closest?.('[data-equipment-toggle]');
    if (!checkbox) return;
    const previous = !checkbox.checked;
    checkbox.disabled = true;
    try {
      await window.strategicSubmitMutation('/api/equipment', {
        body: new URLSearchParams({
          inventory_item_id: checkbox.dataset.inventoryItemId,
          equipped: String(checkbox.checked),
        }),
        originPage: checkbox.closest('#strategic-page'),
      });
    } catch (_error) {
      checkbox.checked = previous;
      checkbox.disabled = false;
    }
  });
})();
