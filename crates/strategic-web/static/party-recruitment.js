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
      overlay.append(group);
    });

    const leftSidebar = document.querySelector(".left-sidebar");
    const rightSidebar = document.querySelector(".right-sidebar");
    const clearRoleInspection = () => {
      document.querySelectorAll("[data-party-role-group].selected").forEach((group) => {
        group.classList.remove("selected");
        group.querySelectorAll("[data-select-party-role]").forEach((button) => button.setAttribute("aria-pressed", "false"));
      });
      document.querySelectorAll("[data-role-inspection-panel]").forEach((detail) => detail.remove());
      document.querySelectorAll(".role-inspection-hidden").forEach((element) => element.classList.remove("role-inspection-hidden"));
    };
    const inspectRole = (group) => {
      const wasSelected = group.classList.contains("selected");
      clearRoleInspection();
      if (wasSelected) return;
      group.classList.add("selected");
      group.querySelectorAll("[data-select-party-role]").forEach((button) => button.setAttribute("aria-pressed", "true"));
      [[leftSidebar, group.querySelector("[data-role-left-template]")], [rightSidebar, group.querySelector("[data-role-right-template]")]].forEach(([sidebar, template]) => {
        if (!sidebar || !template) return;
        Array.from(sidebar.children).forEach((child) => child.classList.add("role-inspection-hidden"));
        const detail = document.createElement("div");
        detail.dataset.roleInspectionPanel = "true";
        detail.className = "role-inspection-panel";
        detail.append(template.content.cloneNode(true));
        sidebar.append(detail);
      });
    };
    overlay.querySelectorAll("[data-select-party-role]").forEach((button) => {
      button.addEventListener("click", () => inspectRole(button.closest("[data-party-role-group]")));
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
      const step = Number(slider.step) || 1;
      const index = Math.round((Number(slider.value) - Number(slider.min || 0)) / step);
      if (output) output.textContent = labels[index] || slider.value;
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
        if (form.elements.weapon_precision) {
          form.elements.weapon_precision.value = button.dataset.roleWeaponPrecision || "0";
        }
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

    document.addEventListener("submit", async (event) => {
      const form = event.target.closest?.("[data-party-recruitment-panel] form[method='post'], [data-party-role-group] form[method='post'], [data-role-inspection-panel] form[method='post']");
      if (!form) return;
      event.preventDefault();
      const body = new URLSearchParams(new FormData(form));
      const response = await fetch(form.action, { method: "POST", body });
      if (!response.ok) return;
      window.location.reload();
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
