(() => {
  const exactEquipmentError = async (response) => {
    const exact = (await response.text()).trim();
    return exact || `${response.status} ${response.statusText}`;
  };
  if (typeof module !== 'undefined') module.exports = { exactEquipmentError };
  if (typeof document === 'undefined') return;

  document.addEventListener('change', async (event) => {
    const checkbox = event.target.closest?.('[data-equipment-toggle]');
    if (!checkbox) return;
    const previous = !checkbox.checked;
    const status = checkbox.parentElement?.querySelector('[data-equipment-status]');
    if (status) {
      status.hidden = true;
      status.textContent = '';
    }
    checkbox.disabled = true;
    try {
      await window.strategicSubmitMutation('/api/equipment', {
        body: new URLSearchParams({
          inventory_item_id: checkbox.dataset.inventoryItemId,
          equipped: String(checkbox.checked),
        }),
        originPage: checkbox.closest('#strategic-page'),
        errorMessageFromResponse: exactEquipmentError,
      });
    } catch (error) {
      checkbox.checked = previous;
      checkbox.disabled = false;
      if (status) {
        status.textContent = error.message || 'Equipment could not be changed.';
        status.hidden = false;
      }
      window.reportStrategicError?.(error, '/api/equipment');
    }
  });
})();
