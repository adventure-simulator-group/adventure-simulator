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

    panel.querySelectorAll("[data-rating-toggle]").forEach((toggle) => {
      const select = panel.querySelector(`[data-rating-select="${toggle.dataset.ratingToggle}"]`);
      const sync = () => { if (select) select.disabled = !toggle.checked; };
      toggle.addEventListener("change", sync);
      sync();
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
          else if (Number(value) > 0) {
            const toggle = form.querySelector(`[data-rating-toggle="${name}"]`);
            if (toggle) toggle.checked = true;
            field.disabled = false;
            field.value = String(value);
          }
        });
      });
    });

    panel.querySelectorAll("form[method='post']").forEach((form) => {
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
