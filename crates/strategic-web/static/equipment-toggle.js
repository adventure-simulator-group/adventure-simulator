(() => {
  const exactEquipmentError = async (response) => {
    const exact = (await response.text()).trim();
    return exact || `${response.status} ${response.statusText}`;
  };
  const parsePlacements = (value) => (value || '').split('|').filter(Boolean);
  const parseJsonArray = (value) => {
    try {
      const options = JSON.parse(value || '[]');
      return Array.isArray(options) ? options : [];
    } catch {
      return [];
    }
  };
  const parsePlacementOptions = parseJsonArray;
  const parseInputMap = parseJsonArray;
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
  const inputRank = (candidate, input) => {
    const rank = candidate?.inputRanks?.[input];
    return Number.isFinite(Number(rank)) ? Number(rank) : null;
  };
  const selectionForInput = (placements, input) => {
    const candidates = placements.flatMap((placement) => {
      const requirements = placement.requirements || [];
      if (requirements.length === 0) {
        const rank = inputRank(placement, input);
        return rank === null ? [] : [{
          placementIndex: placement.placementIndex,
          attachmentTargets: [],
          rank,
        }];
      }
      const bodyRank = inputRank(placement, input);
      if (placement.hasBody && bodyRank === null) return [];
      const selectedTargets = [];
      const selectedRanks = [];
      const selectedCapacity = new Map();
      for (const requirement of requirements) {
        const target = [...(requirement.targets || [])]
          .filter((candidate) => inputRank(candidate, input) !== null)
          .sort((left, right) =>
            inputRank(right, input) - inputRank(left, input) ||
            left.parentInventoryItemId - right.parentInventoryItemId ||
            left.attachmentPointId.localeCompare(right.attachmentPointId)
          )
          .find((candidate) => {
            const capacityKey = `${candidate.parentInventoryItemId}:${candidate.attachmentPointId}`;
            return (selectedCapacity.get(capacityKey) || 0) < Number(candidate.freeCapacity || 1);
          });
        if (!target) return [];
        const capacityKey = `${target.parentInventoryItemId}:${target.attachmentPointId}`;
        selectedCapacity.set(capacityKey, (selectedCapacity.get(capacityKey) || 0) + 1);
        selectedRanks.push(inputRank(target, input));
        selectedTargets.push({
          requirement_index: requirement.requirementIndex,
          parent_inventory_item_id: target.parentInventoryItemId,
          attachment_point_id: target.attachmentPointId,
        });
      }
      return [{
        placementIndex: placement.placementIndex,
        attachmentTargets: selectedTargets,
        rank: Math.max(bodyRank ?? -1, ...selectedRanks),
      }];
    });
    candidates.sort((left, right) =>
      right.rank - left.rank || left.placementIndex - right.placementIndex
    );
    if (!candidates[0]) return null;
    return {
      placementIndex: candidates[0].placementIndex,
      attachmentTargets: candidates[0].attachmentTargets,
    };
  };
  const normalizedEquipmentInput = (event) => {
    if (event.key === 'Tab') return 'tab';
    if (event.key.length === 1) return event.key.toLowerCase();
    return '';
  };
  const chooseSlot = (control) => new Promise((resolve) => {
    const placements = parsePlacementOptions(control.dataset.equipmentPlacementOptions);
    const inputs = parseInputMap(control.dataset.equipmentInputMap);
    const choices = inputs.map((input) => ({
      ...input,
      selection: selectionForInput(placements, input.input),
    }));

    const dialog = document.createElement('dialog');
    dialog.className = 'equipment-placement-modal equipment-slot-modal';
    dialog.setAttribute('aria-labelledby', `equipment-placement-title-${control.dataset.inventoryItemId}`);
    const title = document.createElement('h2');
    title.id = `equipment-placement-title-${control.dataset.inventoryItemId}`;
    title.textContent = control.getAttribute('aria-label') || 'Assign equipment slot';
    const help = document.createElement('p');
    help.className = 'small-copy';
    help.textContent = 'Press a highlighted key or click it. Unavailable slots are dimmed.';
    const keyboard = document.createElement('div');
    keyboard.className = 'equipment-slot-keyboard';
    keyboard.setAttribute('role', 'group');
    keyboard.setAttribute('aria-label', 'QWERTY equipment slots');
    choices.forEach((choice) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'equipment-slot-choice';
      button.dataset.equipmentInput = choice.input;
      button.style.setProperty('--equipment-key-row', Number(choice.row) + 1);
      button.style.setProperty('--equipment-key-column', Number(choice.column) + 1);
      button.disabled = choice.selection === null;
      button.title = `${choice.label}: ${(choice.locations || []).join(', ')}`;
      button.setAttribute(
        'aria-label',
        `${choice.label}, ${(choice.locations || []).join(', ')}${button.disabled ? ', unavailable' : ''}`,
      );
      const key = document.createElement('kbd');
      key.textContent = choice.label;
      const locations = document.createElement('span');
      locations.textContent = (choice.locations || []).join(' / ');
      button.append(key, locations);
      button.addEventListener('click', () => {
        dialog.returnValue = choice.input;
        dialog.close();
      });
      keyboard.append(button);
    });
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.className = 'equipment-slot-cancel';
    cancel.textContent = 'Cancel';
    cancel.addEventListener('click', () => {
      dialog.returnValue = '';
      dialog.close();
    });
    dialog.append(title, help, keyboard, cancel);
    document.body.append(dialog);

    const onKeydown = (event) => {
      if (event.key === 'Escape') return;
      const input = normalizedEquipmentInput(event);
      const choice = choices.find((candidate) =>
        candidate.input === input && candidate.selection !== null
      );
      if (!choice) return;
      event.preventDefault();
      dialog.returnValue = choice.input;
      dialog.close();
    };
    dialog.addEventListener('keydown', onKeydown);
    dialog.addEventListener('close', () => {
      dialog.removeEventListener('keydown', onKeydown);
      const choice = choices.find((candidate) => candidate.input === dialog.returnValue);
      dialog.remove();
      control.focus();
      resolve(choice?.selection || null);
    }, { once: true });
    dialog.showModal();
    keyboard.querySelector('button:not([disabled])')?.focus();
  });
  const equipmentMutation = async (control, equipped, selection = null) =>
    window.strategicSubmitMutation('/api/equipment', {
      body: new URLSearchParams({
        inventory_item_id: control.dataset.inventoryItemId,
        equipped: String(equipped),
        ...(selection === null ? {} : { placement_index: String(selection.placementIndex) }),
        ...(selection ? {
          attachment_targets: JSON.stringify(selection.attachmentTargets),
        } : {}),
      }),
      originPage: control.closest('#strategic-page'),
      errorMessageFromResponse: exactEquipmentError,
    });
  const statusFor = (control) =>
    control.parentElement?.querySelector('[data-equipment-status]');
  const clearStatus = (status) => {
    if (!status) return;
    status.hidden = true;
    status.textContent = '';
  };
  const reportEquipmentError = (control, status, error) => {
    control.disabled = false;
    if (status) {
      status.textContent = error.message || 'Equipment could not be changed.';
      status.hidden = false;
    }
    window.reportStrategicError?.(error, '/api/equipment');
  };

  if (typeof module !== 'undefined') {
    module.exports = {
      attachmentTargetsForPlacement,
      exactEquipmentError,
      normalizedEquipmentInput,
      parseInputMap,
      parsePlacements,
      parsePlacementOptions,
      selectionForInput,
    };
  }
  if (typeof document === 'undefined') return;

  document.addEventListener('change', async (event) => {
    const checkbox = event.target.closest?.('[data-equipment-medication]');
    if (!checkbox) return;
    const previous = !checkbox.checked;
    const status = statusFor(checkbox);
    clearStatus(status);
    checkbox.disabled = true;
    try {
      await equipmentMutation(checkbox, checkbox.checked);
    } catch (error) {
      checkbox.checked = previous;
      reportEquipmentError(checkbox, status, error);
    }
  });

  document.addEventListener('click', async (event) => {
    const control = event.target.closest?.(
      '[data-equipment-toggle]:not([data-equipment-medication])',
    );
    if (!control || control.disabled) return;
    const equipped = control.dataset.equipmentEquipped === 'true';
    const status = statusFor(control);
    clearStatus(status);
    const selection = equipped ? null : await chooseSlot(control);
    if (!equipped && selection === null) return;
    control.disabled = true;
    try {
      await equipmentMutation(control, !equipped, selection);
    } catch (error) {
      reportEquipmentError(control, status, error);
    }
  });
})();
