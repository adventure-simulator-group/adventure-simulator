(() => {
  async function loadRecruitment() {
    const overlay = document.querySelector(".party-portrait-overlay");
    if (!overlay) return;

    const response = await fetch("/party-recruitment/panel", { headers: { Accept: "text/html" } });
    if (!response.ok) return;
    const host = document.createElement("div");
    host.innerHTML = await response.text();
    const panel = host.firstElementChild;
    if (!panel) return;
    document.body.append(panel);

    const leaderId = panel.dataset.leaderId;
    const leaderPortrait = overlay.querySelector(`[data-character-id="${leaderId}"]`);
    if (leaderPortrait) {
      const crown = document.createElement("span");
      crown.className = "party-leader-crown";
      crown.textContent = "♛";
      crown.title = "Party leader";
      leaderPortrait.append(crown);
    }

    panel.querySelectorAll("[data-party-role-group]").forEach((group) => {
      group.hidden = false;
      group.querySelectorAll("[data-filled-character-id]").forEach((marker) => {
        const portrait = overlay.querySelector(`[data-character-id="${marker.dataset.filledCharacterId}"]`);
        if (portrait) marker.replaceWith(portrait);
        else marker.remove();
      });
      overlay.append(group);
    });

    if (panel.dataset.canManage === "true" && leaderPortrait) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "party-recruitment-plus";
      button.textContent = "+";
      button.title = "Add a recruitment role";
      button.setAttribute("aria-label", "Add a recruitment role");
      leaderPortrait.append(button);
      const dialog = panel.querySelector("[data-recruitment-dialog]");
      button.addEventListener("click", (event) => {
        event.preventDefault();
        dialog?.showModal();
      });
    }

    const syncSlider = (slider) => {
      const labels = (slider.dataset.sliderLabels || "").split("|");
      const output = panel.querySelector(`[data-slider-output="${slider.name}"]`);
      if (output) output.textContent = labels[Number(slider.value)] || slider.value;
    };
    panel.querySelectorAll("[data-discrete-slider]").forEach((slider) => {
      slider.addEventListener("input", () => syncSlider(slider));
      syncSlider(slider);
    });

    panel.querySelectorAll("[data-load-saved-role]").forEach((button) => {
      button.addEventListener("click", () => {
        const form = panel.querySelector("[data-role-builder]");
        if (!form) return;
        form.reset();
        form.elements.name.value = button.dataset.roleName || "";
        const requirements = JSON.parse(button.dataset.roleRequirements || "{}");
        Object.entries(requirements).forEach(([name, value]) => {
          const field = form.elements[name];
          if (!field) return;
          if (typeof value === "boolean") field.checked = value;
          else field.value = String(value);
        });
        const armor = form.elements.armor_tier;
        if (armor) {
          armor.value = requirements.full_armor ? "4"
            : requirements.three_quarter_armor ? "3"
            : requirements.half_armor ? "2"
            : requirements.quarter_armor ? "1" : "0";
        }
        form.querySelectorAll("[data-discrete-slider]").forEach(syncSlider);
      });
    });

    document.querySelectorAll("[data-party-recruitment-panel] form[method='post'], [data-party-role-group] form[method='post']").forEach((form) => {
      form.addEventListener("submit", async (event) => {
        event.preventDefault();
        const body = new URLSearchParams(new FormData(form));
        const response = await fetch(form.action, { method: "POST", body });
        if (!response.ok) return;
        window.location.reload();
      });
    });
  }

  document.addEventListener("pointerenter", async (event) => {
    const portrait = event.target.closest?.("[data-character-id]");
    if (!portrait || portrait.dataset.tagsLoaded) return;
    portrait.dataset.tagsLoaded = "true";
    const response = await fetch(`/characters/${portrait.dataset.characterId}/capabilities`);
    if (!response.ok) return;
    const { tags = [] } = await response.json();
    const target = portrait.querySelector("[data-character-tags]");
    if (target) {
      target.textContent = tags.join(" · ");
      target.hidden = tags.length === 0;
    }
  }, true);

  loadRecruitment().catch(() => {});
})();
