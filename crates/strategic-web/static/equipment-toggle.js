(() => {
  document.addEventListener('change', async (event) => {
    const checkbox = event.target.closest?.('[data-equipment-toggle]');
    if (!checkbox) return;
    const previous = !checkbox.checked;
    checkbox.disabled = true;
    try {
      await window.strategicFetch('/api/equipment', {
        method: 'POST',
        body: new URLSearchParams({
          inventory_item_id: checkbox.dataset.inventoryItemId,
          equipped: String(checkbox.checked),
        }),
      });
      window.location.reload();
    } catch (_error) {
      checkbox.checked = previous;
      checkbox.disabled = false;
    }
  });
})();
