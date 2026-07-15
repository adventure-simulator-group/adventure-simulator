(() => {
  let recruitmentSignature = "";
  let refreshGeneration = 0;

  async function loadRecruitment() {
    const overlay = document.querySelector(".party-portrait-overlay");
    if (!overlay) return;
    const portraitMembers = overlay.querySelector("[data-party-portrait-members]") || overlay;

    const generation = ++refreshGeneration;
    const response = await window.strategicBackgroundFetch("party-recruitment", "/party-recruitment/panel", {
      headers: { Accept: "text/html" },
    });
    if (!response.ok) return;
    const host = document.createElement("div");
    host.innerHTML = await response.text();
    const panel = host.firstElementChild;
    if (!panel || generation !== refreshGeneration) return;
    const nextSignature = panel.outerHTML;
    if (nextSignature === recruitmentSignature) return;
    recruitmentSignature = nextSignature;

    document.querySelectorAll("[data-role-inspection-panel], [data-applicant-inspection-preview]").forEach((element) => element.remove());
    document.querySelectorAll(".role-inspection-hidden").forEach((element) => element.classList.remove("role-inspection-hidden"));
    overlay.querySelectorAll("[data-party-role-group], [data-party-aggregate-checks], .party-recruitment-add, .party-leader-crown").forEach((element) => element.remove());
    document.querySelector("[data-party-recruitment-panel]")?.remove();
    document.body.append(panel);

    const leaderId = panel.dataset.leaderId;
    const leaderPortrait = overlay.querySelector(`[data-character-id="${leaderId}"]`);
    if (leaderPortrait) {
      portraitMembers.prepend(leaderPortrait);
      const crown = document.createElement("span");
      crown.className = "party-leader-crown";
      crown.textContent = "♛";
      crown.title = "Party leader";
      leaderPortrait.append(crown);
    }

    const aggregateChecks = panel.querySelector("[data-party-aggregate-checks]");
    const partyChest = overlay.querySelector(".party-inventory-portrait");
    if (partyChest && leaderPortrait) portraitMembers.insertBefore(partyChest, leaderPortrait);
    if (aggregateChecks) {
      overlay.insertBefore(aggregateChecks, portraitMembers);
    }

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

    const dialog = panel.querySelector("[data-recruitment-dialog]");
    const roleBuilder = panel.querySelector("[data-role-builder]");
    const builderHeading = panel.querySelector("[data-role-builder-heading]");
    const builderSubmit = panel.querySelector("[data-role-builder-submit]");
    const builderHelp = panel.querySelector("[data-role-builder-help]");
    const resetBuilderMode = () => {
      if (!roleBuilder) return;
      roleBuilder.reset();
      roleBuilder.action = "/party-recruitment/roles";
      roleBuilder.elements.quantity.min = "1";
      roleBuilder.elements.quantity.value = "1";
      if (builderHeading) builderHeading.textContent = "Recruit party roles";
      if (builderSubmit) builderSubmit.textContent = "Add role";
      if (builderHelp) builderHelp.textContent = "Create one visually grouped portrait per slot.";
    };
    const populateBuilder = (source) => {
      if (!roleBuilder) return;
      roleBuilder.elements.name.value = source.dataset.roleName || "";
      const requirements = JSON.parse(source.dataset.roleRequirements || "{}");
      Object.entries(requirements).forEach(([name, value]) => {
        const field = roleBuilder.elements[name];
        if (!field) return;
        if (typeof value === "boolean") field.checked = value;
        else field.value = String(value);
      });
      if (roleBuilder.elements.weapon_precision) {
        roleBuilder.elements.weapon_precision.value = source.dataset.roleWeaponPrecision || "0";
      }
      const armor = roleBuilder.elements.armor_tier;
      if (armor) {
        armor.value = requirements.full_armor ? "4"
          : requirements.three_quarter_armor ? "3"
          : requirements.half_armor ? "2"
          : requirements.quarter_armor ? "1" : "0";
      }
      roleBuilder.querySelectorAll("[data-discrete-slider]").forEach(syncSlider);
    };

    if (panel.dataset.canManage === "true" && leaderPortrait) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "party-recruitment-add";
      button.textContent = "+";
      button.title = "Add a recruitment role";
      button.setAttribute("aria-label", "Add a recruitment role");
      const firstRole = overlay.querySelector("[data-party-role-group]");
      if (firstRole) overlay.insertBefore(button, firstRole); else overlay.append(button);
      button.addEventListener("click", (event) => {
        event.preventDefault();
        resetBuilderMode();
        roleBuilder?.querySelectorAll("[data-discrete-slider]").forEach(syncSlider);
        dialog?.showModal();
      });
    }

    const setPartyCheckTarget = (track, value) => {
      const target = Math.max(0, Math.min(5, Math.round(value)));
      const name = track.dataset.checkName;
      const label = track.dataset.checkLabel;
      const current = Number(track.dataset.checkCurrent) || 0;
      aggregateChecks?.querySelectorAll(`[data-party-check-target-form] input[name="${name}"]`).forEach((input) => {
        input.value = String(target);
      });
      track.dataset.checkTarget = String(target);
      track.title = `${label}: ${current.toFixed(1)}; target ${target}`;
      const exact = track.querySelector(".party-check-exact");
      if (exact) exact.textContent = `${label}: ${current.toFixed(1)} · target ${target}`;
      const handle = track.querySelector("[data-party-check-target-handle]");
      if (handle) {
        handle.style.left = `${target / 5 * 100}%`;
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
      const response = await window.strategicFetch(form.action, { method: "POST", body });
      form.toggleAttribute("data-save-error", !response.ok);
    };
    const targetFromPointer = (track, event) => {
      const rect = track.getBoundingClientRect();
      return (event.clientX - rect.left) / rect.width * 5;
    };
    aggregateChecks?.querySelectorAll(".party-check-track-editable").forEach((track) => {
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
        setPartyCheckTarget(track, event.key === "Home" ? 0 : event.key === "End" ? 5 : current + steps[event.key]);
        savePartyCheckTarget(track);
      });
    });

    const syncSlider = (slider) => {
      const labels = (slider.dataset.sliderLabels || "").split("|");
      const output = panel.querySelector(`[data-slider-output="${slider.name}"]`);
      const step = Number(slider.step) || 1;
      const min = Number(slider.min) || 0;
      const max = Number(slider.max) || 1;
      const value = Number(slider.value);
      const index = Math.round((value - min) / step);
      if (output) output.textContent = labels[index] || slider.value;
      slider.closest(".role-slider-control")?.style.setProperty("--role-slider-progress", `${(value - min) / (max - min) * 100}%`);
    };
    panel.querySelectorAll("[data-discrete-slider]").forEach((slider) => {
      slider.addEventListener("input", () => syncSlider(slider));
      syncSlider(slider);
    });

    panel.querySelectorAll("[data-load-saved-role]").forEach((button) => {
      button.addEventListener("click", () => {
        resetBuilderMode();
        populateBuilder(button);
      });
    });

    panel.querySelectorAll("[data-edit-current-role]").forEach((button) => {
      button.addEventListener("click", () => {
        if (!roleBuilder) return;
        roleBuilder.reset();
        roleBuilder.action = `/party-recruitment/roles/${button.dataset.roleId}`;
        roleBuilder.elements.quantity.min = button.dataset.roleFilled || "0";
        roleBuilder.elements.quantity.value = button.dataset.roleQuantity || "0";
        populateBuilder(button);
        if (builderHeading) builderHeading.textContent = `Edit ${button.dataset.roleName || "role"}`;
        if (builderSubmit) builderSubmit.textContent = "Save changes";
        if (builderHelp) builderHelp.textContent = "Slot count cannot be reduced below the number already filled.";
        dialog?.showModal();
      });
    });

    const saveRoleDialog = panel.querySelector("[data-save-role-dialog]");
    const saveRoleForm = panel.querySelector("[data-save-role-form]");
    panel.querySelector("[data-save-current-role]")?.addEventListener("click", () => {
      if (!roleBuilder || !saveRoleDialog || !saveRoleForm) return;
      const fields = saveRoleForm.querySelector("[data-saved-role-fields]");
      fields.replaceChildren();
      const values = new FormData(roleBuilder);
      values.delete("name");
      values.delete("quantity");
      values.delete("save_role");
      values.forEach((value, name) => {
        const input = document.createElement("input");
        input.type = "hidden";
        input.name = name;
        input.value = value;
        fields.append(input);
      });
      saveRoleForm.elements.name.value = roleBuilder.elements.name.value || "";
      saveRoleDialog.showModal();
      saveRoleForm.elements.name.focus();
      saveRoleForm.elements.name.select();
    });

    const renameRoleDialog = panel.querySelector("[data-rename-role-dialog]");
    const renameRoleForm = panel.querySelector("[data-rename-role-form]");
    panel.querySelectorAll("[data-rename-saved-role]").forEach((button) => {
      button.addEventListener("click", () => {
        if (!renameRoleDialog || !renameRoleForm) return;
        renameRoleForm.action = `/party-recruitment/saved/${button.dataset.roleId}/rename`;
        renameRoleForm.elements.name.value = button.dataset.roleName || "";
        renameRoleDialog.showModal();
        renameRoleForm.elements.name.focus();
        renameRoleForm.elements.name.select();
      });
    });

    panel.querySelectorAll("[data-cancel-role-name]").forEach((button) => {
      button.addEventListener("click", () => button.closest("dialog")?.close());
    });

  }

  document.addEventListener("submit", async (event) => {
    const form = event.target.closest?.("[data-party-recruitment-panel] form[method='post'], [data-party-role-group] form[method='post'], [data-role-inspection-panel] form[method='post']");
    if (!form) return;
    event.preventDefault();
    const body = new URLSearchParams(new FormData(form));
    const response = await window.strategicFetch(form.action, { method: "POST", body });
    if (!response.ok) return;
    form.closest("dialog")?.close();
    loadRecruitment().catch((error) => window.reportStrategicError(error, "party recruitment"));
  });

  window.queueStrategicInitialLoad(loadRecruitment).catch((error) => window.reportStrategicError(error, "party recruitment"));
  document.addEventListener("strategic-live-update", () => loadRecruitment().catch((error) => window.reportStrategicError(error, "party recruitment")));
  document.addEventListener("strategic-live-regions-refreshed", () => {
    recruitmentSignature = "";
    loadRecruitment().catch((error) => window.reportStrategicError(error, "party recruitment"));
  });
})();
