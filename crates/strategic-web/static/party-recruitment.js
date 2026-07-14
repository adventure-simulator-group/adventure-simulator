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
      overlay.prepend(leaderPortrait);
      const crown = document.createElement("span");
      crown.className = "party-leader-crown";
      crown.textContent = "♛";
      crown.title = "Party leader";
      leaderPortrait.append(crown);
    }

    const aggregateChecks = panel.querySelector("[data-party-aggregate-checks]");
    if (aggregateChecks && leaderPortrait) overlay.insertBefore(aggregateChecks, leaderPortrait);

    panel.querySelectorAll("[data-party-role-group]").forEach((group) => {
      group.hidden = false;
      overlay.append(group);
    });

    const leftSidebar = document.querySelector(".left-sidebar");
    const rightSidebar = document.querySelector(".right-sidebar");
    const center = document.querySelector("main.center-content");
    const clearRoleInspection = () => {
      document.querySelectorAll("[data-party-role-group].selected").forEach((group) => {
        group.classList.remove("selected");
        group.querySelectorAll("[data-select-party-role]").forEach((button) => button.setAttribute("aria-pressed", "false"));
      });
      document.querySelectorAll("[data-role-inspection-panel]").forEach((detail) => detail.remove());
      document.querySelectorAll("[data-applicant-inspection-preview]").forEach((detail) => detail.remove());
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
      const rightDetail = rightSidebar?.querySelector("[data-role-inspection-panel]");
      rightDetail?.querySelectorAll("[data-select-role-applicant]").forEach((button) => {
        button.addEventListener("click", () => {
          const applicant = button.closest(".role-request-detail");
          const leftTemplate = applicant?.querySelector("[data-applicant-left-template]");
          const centerTemplate = applicant?.querySelector("[data-applicant-center-template]");
          leftSidebar?.querySelector("[data-role-inspection-panel]")?.remove();
          if (leftSidebar && leftTemplate) {
            const detail = document.createElement("div");
            detail.dataset.roleInspectionPanel = "true";
            detail.className = "role-inspection-panel";
            detail.append(leftTemplate.content.cloneNode(true));
            leftSidebar.append(detail);
          }
          center?.querySelectorAll("[data-applicant-inspection-preview]").forEach((preview) => preview.remove());
          center?.querySelectorAll(":scope > .service-visual").forEach((visual) => visual.classList.add("role-inspection-hidden"));
          if (center && centerTemplate) {
            const preview = document.createElement("div");
            preview.dataset.applicantInspectionPreview = "true";
            preview.className = "applicant-inspection-preview";
            preview.append(centerTemplate.content.cloneNode(true));
            center.prepend(preview);
          }
          rightDetail.querySelectorAll("[data-select-role-applicant]").forEach((candidate) => {
            candidate.setAttribute("aria-pressed", String(candidate === button));
          });
        });
      });
    };
    overlay.querySelectorAll("[data-select-party-role]").forEach((button) => {
      button.addEventListener("click", () => inspectRole(button.closest("[data-party-role-group]")));
    });

    if (panel.dataset.canManage === "true" && leaderPortrait) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "party-recruitment-add";
      button.textContent = "+";
      button.title = "Add a recruitment role";
      button.setAttribute("aria-label", "Add a recruitment role");
      const firstRole = overlay.querySelector("[data-party-role-group]");
      if (firstRole) overlay.insertBefore(button, firstRole); else overlay.append(button);
      const dialog = panel.querySelector("[data-recruitment-dialog]");
      button.addEventListener("click", (event) => {
        event.preventDefault();
        dialog?.showModal();
      });
    }

    const setPartyCheckTarget = (track, value) => {
      const target = Math.max(0, Math.min(8, Math.round(value)));
      const name = track.dataset.checkName;
      const label = track.dataset.checkLabel;
      const current = Number(track.dataset.checkCurrent) || 0;
      panel.querySelectorAll(`[data-party-check-target-form] input[name="${name}"]`).forEach((input) => {
        input.value = String(target);
      });
      track.dataset.checkTarget = String(target);
      track.title = `${label}: ${current.toFixed(2)}; target ${target}`;
      const exact = track.querySelector(".party-check-exact");
      if (exact) exact.textContent = `${label}: ${current.toFixed(2)} · target ${target}`;
      const handle = track.querySelector("[data-party-check-target-handle]");
      if (handle) {
        handle.style.left = `${target / 8 * 100}%`;
        handle.setAttribute("aria-valuenow", String(target));
        handle.title = `${label} target: ${target}`;
      }
      track.closest("[data-party-check]")?.classList.toggle("deficient", target > 0 && current < target);
      return target;
    };
    const savePartyCheckTarget = async (track) => {
      const form = track.closest("[data-party-check-target-form]");
      if (!form) return;
      const body = new URLSearchParams(new FormData(form));
      const response = await fetch(form.action, { method: "POST", body });
      form.toggleAttribute("data-save-error", !response.ok);
    };
    const targetFromPointer = (track, event) => {
      const rect = track.getBoundingClientRect();
      return (event.clientX - rect.left) / rect.width * 8;
    };
    panel.querySelectorAll(".party-check-track-editable").forEach((track) => {
      track.addEventListener("click", (event) => {
        if (event.target.closest("[data-party-check-target-handle]")) return;
        setPartyCheckTarget(track, targetFromPointer(track, event));
        savePartyCheckTarget(track);
      });
      const handle = track.querySelector("[data-party-check-target-handle]");
      if (!handle) return;
      handle.addEventListener("pointerdown", (event) => {
        event.preventDefault();
        const move = (pointerEvent) => setPartyCheckTarget(track, targetFromPointer(track, pointerEvent));
        const finish = () => {
          handle.removeEventListener("pointermove", move);
          handle.removeEventListener("pointerup", finish);
          handle.removeEventListener("pointercancel", finish);
          savePartyCheckTarget(track);
        };
        handle.setPointerCapture(event.pointerId);
        handle.addEventListener("pointermove", move);
        handle.addEventListener("pointerup", finish);
        handle.addEventListener("pointercancel", finish);
        move(event);
      });
      handle.addEventListener("keydown", (event) => {
        const steps = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1 };
        if (!(event.key in steps) && event.key !== "Home" && event.key !== "End") return;
        event.preventDefault();
        const current = Number(track.dataset.checkTarget) || 0;
        setPartyCheckTarget(track, event.key === "Home" ? 0 : event.key === "End" ? 8 : current + steps[event.key]);
        savePartyCheckTarget(track);
      });
    });

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

  loadRecruitment().catch(() => {});
})();
