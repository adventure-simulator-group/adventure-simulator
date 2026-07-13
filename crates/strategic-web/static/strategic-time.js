(() => {
  const format = (minutes) => {
    const day = Math.floor(minutes / 1440) % 365 + 1;
    const hour = Math.floor(minutes / 60) % 24;
    const minute = minutes % 60;
    return `Day ${day} · ${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`;
  };

  fetch('/time')
    .then((response) => response.json())
    .then(({ character_minutes: characterMinutes, official_minutes: officialMinutes }) => {
      document.querySelectorAll('[data-player-time]').forEach((element) => {
        element.textContent = format(characterMinutes);
        element.title = `Official time: ${format(officialMinutes)}`;
      });
      const availableDays = Math.floor(Math.max(0, officialMinutes - characterMinutes) / 1440);
      document.querySelectorAll('[data-rest-days]').forEach((input) => {
        input.max = availableDays;
        if (Number(input.value) > availableDays) input.value = availableDays;
        const submit = input.form?.querySelector('[data-rest-submit]');
        if (submit) submit.disabled = Number(input.value) <= 0 || availableDays <= 0;
      });
    })
    .catch(() => {});
})();
