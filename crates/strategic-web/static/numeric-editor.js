(() => {
  const clamp = (value, minimum, maximum) => Math.min(maximum, Math.max(minimum, value));

  const stepNumericValue = (text, direction, options) => {
    const parsed = options.parse(text);
    const base = parsed === null ? options.initialValue : parsed;
    return options.format(clamp(base + direction * options.step, options.minimum, options.maximum));
  };

  const open = ({
    display,
    initialValue,
    parse,
    format,
    step,
    minimum,
    maximum,
    anchor = display,
    rail = display.closest('.left-sidebar, .right-sidebar'),
    groupLabel = 'Edit number',
    inputLabel = 'Number',
    increaseLabel = 'Increase number',
    decreaseLabel = 'Decrease number',
    saveLabel = 'Save number',
    cancelLabel = 'Cancel number edit',
    onCommit,
  }) => {
    if (!display || display.dataset.editing || document.querySelector('.numeric-editor')) return false;
    display.dataset.editing = 'true';

    const editor = document.createElement('span');
    editor.className = 'numeric-editor';
    editor.setAttribute('role', 'group');
    editor.setAttribute('aria-label', groupLabel);

    const confirm = document.createElement('button');
    confirm.type = 'button';
    confirm.className = 'numeric-editor-action numeric-editor-confirm';
    confirm.setAttribute('aria-label', saveLabel);
    confirm.title = 'Save';
    confirm.textContent = '\u2713';

    const inputStack = document.createElement('span');
    inputStack.className = 'numeric-editor-input-stack';
    const increase = document.createElement('button');
    increase.type = 'button';
    increase.className = 'numeric-editor-step numeric-editor-increase';
    increase.setAttribute('aria-label', increaseLabel);
    increase.textContent = '\u25b2';
    const input = document.createElement('input');
    input.className = 'numeric-editor-input';
    input.type = 'text';
    input.inputMode = 'decimal';
    input.setAttribute('aria-label', inputLabel);
    input.value = format(initialValue);
    const decrease = document.createElement('button');
    decrease.type = 'button';
    decrease.className = 'numeric-editor-step numeric-editor-decrease';
    decrease.setAttribute('aria-label', decreaseLabel);
    decrease.textContent = '\u25bc';
    inputStack.append(increase, input, decrease);

    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.className = 'numeric-editor-action numeric-editor-cancel';
    cancel.setAttribute('aria-label', cancelLabel);
    cancel.title = 'Cancel';
    cancel.textContent = '\u00d7';
    editor.append(confirm, inputStack, cancel);

    const positionEditor = () => {
      const rect = anchor.getBoundingClientRect();
      editor.style.left = `${rect.left + rect.width / 2}px`;
      editor.style.top = `${rect.top + rect.height / 2}px`;
    };

    let finished = false;
    const finish = (commit) => {
      if (finished) return;
      const parsed = parse(input.value);
      if (commit && parsed === null) {
        input.setAttribute('aria-invalid', 'true');
        input.focus();
        return;
      }
      finished = true;
      if (commit) onCommit(clamp(parsed, minimum, maximum));
      delete display.dataset.editing;
      rail?.removeEventListener('scroll', positionEditor);
      window.removeEventListener('resize', positionEditor);
      editor.remove();
      display.hidden = false;
      display.focus();
    };
    const adjust = (direction) => {
      input.value = stepNumericValue(input.value, direction, {
        parse, format, step, minimum, maximum, initialValue,
      });
      input.removeAttribute('aria-invalid');
    };
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        finish(true);
      } else if (event.key === 'Escape') {
        event.preventDefault();
        finish(false);
      } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        adjust(event.key === 'ArrowUp' ? 1 : -1);
      }
    });
    input.addEventListener('input', () => input.removeAttribute('aria-invalid'));
    input.addEventListener('wheel', (event) => {
      event.preventDefault();
      adjust(event.deltaY < 0 ? 1 : -1);
    }, { passive: false });
    increase.addEventListener('click', () => adjust(1));
    decrease.addEventListener('click', () => adjust(-1));
    confirm.addEventListener('click', () => finish(true));
    cancel.addEventListener('click', () => finish(false));
    editor.addEventListener('click', (event) => event.stopPropagation());

    display.hidden = true;
    document.body.append(editor);
    positionEditor();
    rail?.addEventListener('scroll', positionEditor, { passive: true });
    window.addEventListener('resize', positionEditor);
    input.focus();
    input.select();
    return true;
  };

  const api = { open, stepNumericValue };
  if (typeof module !== 'undefined') module.exports = api;
  if (typeof window !== 'undefined') window.StrategicNumericEditor = api;
})();
