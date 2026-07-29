(() => {
  const exactEquipmentError = async (response) => {
    const exact = (await response.text()).trim();
    return exact || `${response.status} ${response.statusText}`;
  };
  const parsePlacements = (value) => (value || '').split('|').filter(Boolean);
  const parsePlacementOptions = (value) => {
    try {
      const options = JSON.parse(value || '[]');
      return Array.isArray(options) ? options : [];
    } catch {
      return [];
    }
  };
  const attachmentTargetsForPlacement = (placement, selectedTargetIndexes = {}) =>
    (placement?.requirements || []).map((requirement) => {
      const selectedTarget = requirement.targets[
        Number(selectedTargetIndexes[requirement.requirementIndex] || 0)
      ];
      return {
        requirement_index: requirement.requirementIndex,
        parent_inventory_item_id: selectedTarget.parentInventoryItemId,
        attachment_point_id: selectedTarget.attachmentPointId,
      };
    });
  const choosePlacement = (checkbox) => new Promise((resolve) => {
    const placements = parsePlacementOptions(checkbox.dataset.equipmentPlacementOptions);
    if (placements.length === 0) return resolve(null);
    if (placements.length === 1 && placements[0].requirements.length === 0) {
      return resolve({ placementIndex: placements[0].placementIndex, attachmentTargets: [] });
    }
    const dialog = document.createElement('dialog');
    dialog.className = 'equipment-placement-modal';
    dialog.setAttribute('aria-labelledby', `equipment-placement-title-${checkbox.dataset.inventoryItemId}`);
    const title = document.createElement('h2');
    title.id = `equipment-placement-title-${checkbox.dataset.inventoryItemId}`;
    title.textContent = checkbox.getAttribute('aria-label') || 'Choose placement';
    const layer = document.createElement('p');
    layer.textContent = `Layer: ${checkbox.dataset.wearLayer}`;
    const form = document.createElement('form');
    form.method = 'dialog';
    placements.forEach((placement, index) => {
      const label = document.createElement('label');
      const radio = document.createElement('input');
      radio.type = 'radio';
      radio.name = 'placement';
      radio.value = String(placement.placementIndex);
      radio.required = true;
      if (index === 0) radio.checked = true;
      label.append(radio, document.createTextNode(` ${placement.label}`));
      form.append(label);
    });
    const targetContainer = document.createElement('fieldset');
    const targetLegend = document.createElement('legend');
    targetLegend.textContent = 'Attachment points';
    targetContainer.append(targetLegend);
    form.append(targetContainer);
    const equip = document.createElement('button');
    equip.value = 'equip';
    equip.textContent = 'Equip';
    const syncParentTarget = () => {
      const placementIndex = Number(form.elements.placement?.value || 0);
      const placement = placements.find((option) => option.placementIndex === placementIndex);
      targetContainer.replaceChildren(targetLegend);
      targetContainer.hidden = !placement?.requirements.length;
      equip.disabled = false;
      placement?.requirements.forEach((requirement) => {
        const label = document.createElement('label');
        label.textContent = `${requirement.channel}: `;
        const select = document.createElement('select');
        select.name = `attachment-${requirement.requirementIndex}`;
        select.required = true;
        requirement.targets.forEach((target, index) => {
          const option = document.createElement('option');
          option.value = String(index);
          option.textContent = target.label;
          select.append(option);
        });
        if (requirement.targets.length === 0) {
          equip.disabled = true;
          const option = document.createElement('option');
          option.textContent = 'No compatible free target';
          select.append(option);
        }
        label.append(select);
        targetContainer.append(label);
      });
    };
    form.addEventListener('change', syncParentTarget);
    const cancel = document.createElement('button');
    cancel.value = 'cancel';
    cancel.formNoValidate = true;
    cancel.textContent = 'Cancel';
    form.append(equip, cancel);
    syncParentTarget();
    dialog.append(title, layer, form);
    document.body.append(dialog);
    dialog.addEventListener('close', () => {
      const selected = form.elements.placement?.value;
      const accepted = dialog.returnValue === 'equip' && selected !== '';
      const placement = placements.find((option) => option.placementIndex === Number(selected));
      const selectedTargetIndexes = Object.fromEntries(
        (placement?.requirements || []).map((requirement) => [
          requirement.requirementIndex,
          form.elements[`attachment-${requirement.requirementIndex}`]?.value || 0,
        ]),
      );
      const attachmentTargets = accepted
        ? attachmentTargetsForPlacement(placement, selectedTargetIndexes)
        : [];
      dialog.remove();
      checkbox.focus();
      resolve(accepted ? {
        placementIndex: Number(selected),
        attachmentTargets,
      } : null);
    }, { once: true });
    dialog.showModal();
    form.elements.placement?.focus();
  });
  if (typeof module !== 'undefined') {
    module.exports = {
      attachmentTargetsForPlacement,
      exactEquipmentError,
      parsePlacements,
      parsePlacementOptions,
    };
  }
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
      const selection = checkbox.checked ? await choosePlacement(checkbox) : null;
      if (checkbox.checked && parsePlacements(checkbox.dataset.wearPlacements).length > 0 && selection === null) {
        checkbox.checked = previous;
        checkbox.disabled = false;
        return;
      }
      await window.strategicSubmitMutation('/api/equipment', {
        body: new URLSearchParams({
          inventory_item_id: checkbox.dataset.inventoryItemId,
          equipped: String(checkbox.checked),
          ...(selection === null ? {} : { placement_index: String(selection.placementIndex) }),
          ...(selection ? {
            attachment_targets: JSON.stringify(selection.attachmentTargets),
          } : {}),
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
