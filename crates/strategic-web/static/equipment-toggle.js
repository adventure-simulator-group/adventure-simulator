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
          occupant: placement.inputOccupants?.[input] || null,
          rank,
        }];
      }
      const bodyRank = inputRank(placement, input);
      if (placement.hasBody && bodyRank === null) return [];
      const selectedTargets = [];
      const selectedRanks = [];
      const selectedCapacity = new Map();
      const selectedOccupants = [];
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
            const freeCapacity = candidate.freeCapacity == null
              ? 1
              : Number(candidate.freeCapacity);
            const capacity = freeCapacity
              + (candidate.occupants || []).length;
            return (selectedCapacity.get(capacityKey) || 0) < capacity;
          });
        if (!target) return [];
        const capacityKey = `${target.parentInventoryItemId}:${target.attachmentPointId}`;
        const usedCapacity = selectedCapacity.get(capacityKey) || 0;
        selectedCapacity.set(capacityKey, usedCapacity + 1);
        selectedRanks.push(inputRank(target, input));
        const freeCapacity = target.freeCapacity == null ? 1 : Number(target.freeCapacity);
        const occupiedIndex = usedCapacity - freeCapacity;
        if (occupiedIndex >= 0 && target.occupants?.[occupiedIndex]) {
          selectedOccupants.push(target.occupants[occupiedIndex]);
        }
        selectedTargets.push({
          requirement_index: requirement.requirementIndex,
          parent_inventory_item_id: target.parentInventoryItemId,
          attachment_point_id: target.attachmentPointId,
        });
      }
      return [{
        placementIndex: placement.placementIndex,
        attachmentTargets: selectedTargets,
        occupant: selectedOccupants[0] || placement.inputOccupants?.[input] || null,
        rank: Math.max(bodyRank ?? -1, ...selectedRanks),
      }];
    });
    candidates.sort((left, right) =>
      right.rank - left.rank || left.placementIndex - right.placementIndex
    );
    if (!candidates[0]) return null;
    const selection = {
      placementIndex: candidates[0].placementIndex,
      attachmentTargets: candidates[0].attachmentTargets,
    };
    if (candidates[0].occupant) selection.occupant = candidates[0].occupant;
    return selection;
  };
  const normalizedEquipmentInput = (event) => {
    if (event.key === 'Tab') return 'tab';
    if (event.key.length === 1) return event.key.toLowerCase();
    return '';
  };
  const choicesForControl = (control) => {
    const placements = parsePlacementOptions(control.dataset.equipmentPlacementOptions);
    return parseInputMap(control.dataset.equipmentInputMap).map((input) => ({
      ...input,
      selection: selectionForInput(placements, input.input),
    }));
  };
  const createSlotKeyboard = (choices, onChoose = null) => {
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
      if (onChoose === null) button.tabIndex = -1;
      const key = document.createElement('kbd');
      key.textContent = choice.label;
      const icon = document.createElement('span');
      icon.className = 'equipment-slot-choice-icon';
      if (choice.selection?.occupant) {
        icon.style.setProperty(
          '--equipment-slot-icon',
          `url('${choice.selection.occupant.icon}')`,
        );
        icon.setAttribute('role', 'img');
        icon.setAttribute('aria-label', choice.selection.occupant.itemName);
        icon.title = choice.selection.occupant.itemName;
      } else {
        icon.setAttribute('aria-hidden', 'true');
      }
      const locations = document.createElement('span');
      locations.className = 'equipment-slot-choice-label';
      locations.textContent = (choice.locations || []).join(' / ');
      button.append(key, icon, locations);
      if (onChoose !== null) {
        button.addEventListener('click', () => onChoose(choice));
      }
      keyboard.append(button);
    });
    return keyboard;
  };
  const flashInvalidChoice = (keyboard, input, clearPreviousTimer = null) => {
    const unavailable = keyboard.querySelector(`[data-equipment-input="${input}"]`);
    unavailable?.classList.remove('is-invalid-input');
    void unavailable?.offsetWidth;
    unavailable?.classList.add('is-invalid-input');
    clearTimeout(clearPreviousTimer);
    return setTimeout(() => {
      unavailable?.classList.remove('is-invalid-input');
    }, 420);
  };
  const chooseSlot = (control) => new Promise((resolve) => {
    const choices = choicesForControl(control);

    const dialog = document.createElement('dialog');
    dialog.className = 'equipment-placement-modal equipment-slot-modal';
    dialog.setAttribute(
      'aria-label',
      control.getAttribute('aria-label') || 'Choose equipment slot',
    );
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'equipment-slot-close';
    close.setAttribute('aria-label', 'Close equipment slot picker');
    close.title = 'Close';
    close.textContent = '×';
    close.addEventListener('click', () => {
      dialog.returnValue = '';
      dialog.close();
    });
    const keyboard = createSlotKeyboard(choices, (choice) => {
      dialog.returnValue = choice.input;
      dialog.close();
    });
    dialog.append(close, keyboard);
    document.body.append(dialog);

    let invalidFeedbackTimer = null;
    const onKeydown = (event) => {
      if (event.key === 'Escape') return;
      const input = normalizedEquipmentInput(event);
      const choice = choices.find((candidate) => candidate.input === input);
      if (!choice) return;
      event.preventDefault();
      if (choice.selection === null) {
        invalidFeedbackTimer = flashInvalidChoice(
          keyboard,
          choice.input,
          invalidFeedbackTimer,
        );
        return;
      }
      dialog.returnValue = choice.input;
      dialog.close();
    };
    dialog.addEventListener('keydown', onKeydown);
    dialog.addEventListener('close', () => {
      dialog.removeEventListener('keydown', onKeydown);
      clearTimeout(invalidFeedbackTimer);
      const choice = choices.find((candidate) => candidate.input === dialog.returnValue);
      dialog.remove();
      control.focus();
      resolve(choice?.selection || null);
    }, { once: true });
    dialog.showModal();
    keyboard.querySelector('button:not([disabled])')?.focus();
  });
  let previewControl = null;
  let hoveredEquipmentControl = null;
  let previewChoices = [];
  let previewKeyboard = null;
  let previewInvalidFeedbackTimer = null;
  const hideSlotPreview = (control = null) => {
    if (control !== null && previewControl !== control) return;
    document.querySelector('.equipment-slot-preview')?.remove();
    clearTimeout(previewInvalidFeedbackTimer);
    previewInvalidFeedbackTimer = null;
    previewControl = null;
    previewChoices = [];
    previewKeyboard = null;
  };
  const showSlotPreview = (control) => {
    if (
      !control
      || control.disabled
      || !control.matches('[data-equipment-toggle]:not([data-equipment-medication])')
    ) {
      hideSlotPreview();
      return null;
    }
    if (previewControl === control && document.querySelector('.equipment-slot-preview')) {
      return document.querySelector('.equipment-slot-preview');
    }
    hideSlotPreview();
    previewControl = control;
    previewChoices = choicesForControl(control);
    const preview = document.createElement('div');
    preview.className = 'equipment-slot-preview';
    preview.setAttribute('aria-hidden', 'true');
    previewKeyboard = createSlotKeyboard(previewChoices);
    preview.append(previewKeyboard);
    document.body.append(preview);
    return preview;
  };
  const equipmentMutation = async (control, equipped, selection = null) =>
    window.strategicSubmitMutation('/api/equipment', {
      body: new URLSearchParams({
        inventory_item_id: control.dataset.inventoryItemId,
        equipped: String(equipped),
        ...(selection === null ? {} : { placement_index: String(selection.placementIndex) }),
        ...(selection ? {
          attachment_targets: JSON.stringify(selection.attachmentTargets),
          replace_occupied: 'true',
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
      chooseSlot,
      showSlotPreview,
      hideSlotPreview,
    };
  }
  if (typeof document === 'undefined') return;

  const equipmentControlSelector =
    '[data-equipment-toggle]:not([data-equipment-medication])';
  const refreshSlotPreview = (preferredControl = null) => {
    const focused = document.activeElement?.closest?.(equipmentControlSelector);
    showSlotPreview(hoveredEquipmentControl || preferredControl || focused);
  };

  document.addEventListener('mouseover', (event) => {
    const control = event.target.closest?.(equipmentControlSelector);
    if (!control || control.contains(event.relatedTarget)) return;
    hoveredEquipmentControl = control;
    refreshSlotPreview();
  });

  document.addEventListener('mouseout', (event) => {
    const control = event.target.closest?.(equipmentControlSelector);
    if (!control || control.contains(event.relatedTarget)) return;
    if (hoveredEquipmentControl === control) hoveredEquipmentControl = null;
    refreshSlotPreview();
  });

  document.addEventListener('focusin', (event) => {
    const control = event.target.closest?.(equipmentControlSelector);
    if (!control) return;
    refreshSlotPreview(control);
  });

  document.addEventListener('focusout', (event) => {
    if (!event.target.closest?.(equipmentControlSelector)) return;
    setTimeout(refreshSlotPreview);
  });

  document.addEventListener('keydown', async (event) => {
    const focusedControl = event.target.closest?.(equipmentControlSelector);
    if (
      focusedControl
      && (event.key === ' ' || event.code === 'Space')
    ) {
      event.preventDefault();
      if (focusedControl.disabled) return;
      hideSlotPreview();
      const selection = await chooseSlot(focusedControl);
      if (selection === null) return;
      const status = statusFor(focusedControl);
      clearStatus(status);
      focusedControl.disabled = true;
      try {
        await equipmentMutation(focusedControl, true, selection);
      } catch (error) {
        reportEquipmentError(focusedControl, status, error);
      }
      return;
    }
    const control = hoveredEquipmentControl;
    if (!control || control.disabled) return;
    if (document.activeElement === control) return;
    if (event.target.matches?.('input, textarea, select, [contenteditable="true"]')) return;
    const input = normalizedEquipmentInput(event);
    const choice = previewChoices.find((candidate) => candidate.input === input);
    if (!choice) return;
    event.preventDefault();
    if (choice.selection === null) {
      previewInvalidFeedbackTimer = flashInvalidChoice(
        previewKeyboard,
        choice.input,
        previewInvalidFeedbackTimer,
      );
      return;
    }
    const status = statusFor(control);
    clearStatus(status);
    hideSlotPreview();
    control.disabled = true;
    try {
      await equipmentMutation(control, true, choice.selection);
    } catch (error) {
      reportEquipmentError(control, status, error);
    }
  });

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
    hideSlotPreview();
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
