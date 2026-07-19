(() => {
  const planner = document.querySelector("[data-travel-planner]");
  if (!planner) return;

  const route = planner.querySelector("[data-travel-planner-route]");
  const targetInput = document.querySelector("[data-target-surplus]");
  let currentPlan = null;
  const MAX_U32 = 4294967295;
  const VERTICAL_PATH_START = 3;
  const VERTICAL_PATH_END = 97;

  const parseStops = (value) => (value || "")
    .split(",")
    .map(Number)
    .filter((minute) => Number.isFinite(minute) && minute > 0);

  const parseForecasts = (value) => new Map((value || "")
    .split("|")
    .filter(Boolean)
    .map((entry) => {
      const [fatiguePercent, stops = ""] = entry.split(":", 2);
      return [fatiguePercent, parseStops(stops)];
    }));

  const uniqueSortedStops = (stops, totalMinutes) => [...new Set(stops)]
    .filter((minute) => minute > 0 && minute < totalMinutes)
    .sort((left, right) => left - right);

  const showPlan = ({
    destinationName,
    totalMinutes,
    distance = "",
    campStops = [],
    completedMinutes = 0,
    originName = "Start",
    reachedStops = [],
    turnaroundMinutes = 0,
    destinationDescription = "",
  }) => {
    if (!destinationName || !totalMinutes) return;
    planner.dataset.activeName = destinationName;
    planner.dataset.activeDescription = destinationDescription;
    planner.dataset.activeMinutes = String(totalMinutes);
    planner.dataset.activeDistance = distance;
    planner.dataset.activeCampStops = campStops.join(",");

    const reached = new Set(reachedStops);
    const roundTrip = turnaroundMinutes > 0 && totalMinutes > turnaroundMinutes;
    const nodes = [
      { label: originName, displayLabel: "Start", kind: "start", minute: 0 },
      ...uniqueSortedStops(campStops, totalMinutes).map((minute, index) => ({
        label: `Camp ${index + 1}`,
        kind: "camp",
        minute,
        reached: reached.has(minute),
      })),
      { label: destinationName, displayLabel: roundTrip ? "Quest" : "End", kind: "destination", minute: roundTrip ? turnaroundMinutes : totalMinutes, description: destinationDescription },
      ...(roundTrip ? [{ label: originName, displayLabel: "End", kind: "return", minute: totalMinutes }] : []),
    ];
    planner.classList.toggle("round-trip", roundTrip);
    route.replaceChildren(...nodes.sort((a, b) => a.minute - b.minute).map((node, index) => {
      const element = document.createElement("div");
      element.className = `travel-plan-node travel-plan-${node.kind}${node.reached ? " reached" : ""}`;
      if (node.displayLabel) {
        const label = document.createElement("span");
        label.className = "travel-plan-endpoint-label";
        label.textContent = node.displayLabel;
        element.append(label);
      }
      element.setAttribute("role", "separator");
      element.setAttribute("aria-label", node.label);
      element.title = node.description || node.label;
      const progress = node.minute / totalMinutes;
      const vertical = 5 + progress * 90;
      element.style.top = `${vertical}%`;
      if (index < nodes.length - 1) element.dataset.connects = "true";
      return element;
    }));
    planner.hidden = false;

    currentPlan = {
      nodes: [...route.querySelectorAll(".travel-plan-node")],
      campStops,
      originName,
      totalMinutes,
      completedMinutes,
    };
    return currentPlan;
  };

  const previewFor = (name, description, minutes, distance, stops, forecasts = "", roundTrip = false) => {
    planner.dataset.activeCampForecasts = forecasts;
    planner.dataset.activeRoundTrip = String(roundTrip);
    return showPlan({
      destinationName: name,
      destinationDescription: description,
      totalMinutes: roundTrip ? minutes * 2 : minutes,
      turnaroundMinutes: roundTrip ? minutes : 0,
      distance,
      campStops: parseStops(stops),
    });
  };

  document.querySelectorAll(".travel-destination-row").forEach((row) => {
    const show = () => {
      const payload = row.querySelector("[data-provision-payload]");
      if (payload) {
        Object.assign(planner.dataset, {
          provisionPlanningMinutes: payload.dataset.planningMinutes,
          provisionLivingMembers: payload.dataset.livingMembers,
          provisionFoodDays: payload.dataset.foodDays,
          provisionWaterDays: payload.dataset.waterDays,
          provisionRationKcal: payload.dataset.rationKcal,
          provisionWaterskinMl: payload.dataset.waterskinMl,
        });
      }
      previewFor(
        row.dataset.travelName,
        row.dataset.travelDescription || "",
        Number(row.dataset.travelMinutes),
        row.dataset.travelDistance,
        row.dataset.travelCampStops,
        row.dataset.travelCampForecasts,
        row.dataset.travelRoundTrip === "true",
      );
      targetInput?.dispatchEvent(new Event("input"));
    };
    row.addEventListener("pointerenter", show);
    row.addEventListener("focus", show);
    row.addEventListener("click", show);
  });

  const showPersistedJourney = () => {
    const totalMinutes = Number(planner.dataset.journeyTotalMinutes);
    if (!Number.isFinite(totalMinutes) || totalMinutes <= 0) return false;
    const reachedStops = parseStops(planner.dataset.journeyCampStops);
    const forecastStops = parseStops(planner.dataset.journeyForecastStops);
    showPlan({
      destinationName: planner.dataset.journeyDestinationName,
      totalMinutes,
      completedMinutes: Number(planner.dataset.journeyCompletedMinutes) || 0,
      originName: planner.dataset.journeyOriginName || "Start",
      campStops: uniqueSortedStops([...reachedStops, ...forecastStops], totalMinutes),
      reachedStops,
      turnaroundMinutes: Number(planner.dataset.journeyTurnaroundMinutes) || 0,
    });
    return true;
  };

  if (!showPersistedJourney()) {
    previewFor(
      planner.dataset.selectedName,
      planner.dataset.selectedDescription,
      Number(planner.dataset.selectedMinutes),
      "",
      planner.dataset.selectedCampStops,
      planner.dataset.selectedCampForecasts,
      Number(planner.dataset.provisionPlanningMinutes) > Number(planner.dataset.selectedMinutes),
    );
  }

  const animateNextLeg = () => {
    return 0;
  };

  document.querySelectorAll("form[data-travel-submit]").forEach((form) => form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (form.dataset.submitting) return;
    form.dataset.submitting = "true";
    const status = form.parentElement?.querySelector("[data-travel-action-status]");
    const showStatus = (message) => {
      if (!status) return;
      status.textContent = message;
      status.hidden = false;
    };
    if (status) {
      status.hidden = true;
      status.textContent = "";
    }
    const delay = animateNextLeg();
    const data = new URLSearchParams(new FormData(form));
    const submitter = event.submitter;
    if (submitter && submitter.name) data.set(submitter.name, submitter.value);
    window.setTimeout(async () => {
      try {
        const response = await fetch(form.action, {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: data,
        });
        const responseText = await response.text();
        if (!response.ok) throw new Error(responseText || "Unable to begin this journey.");
        if (response.status === 202) {
          form.dataset.submitting = "";
          showStatus("Travel request sent to the party leader.");
          return;
        }
        const fallback = window.setTimeout(() => window.location.assign("/camp"), 1800);
        document.addEventListener("strategic-navigation-start", () => window.clearTimeout(fallback), { once: true });
      } catch (error) {
        form.dataset.submitting = "";
        showStatus(error.message || "Unable to begin this journey.");
      }
    }, delay);
  }));

  const travelConfiguration = document.querySelector("form[data-travel-configuration]");
  if (travelConfiguration) {
    const slider = travelConfiguration.querySelector("input[type=range]");
    const value = travelConfiguration.querySelector("[data-camp-fatigue-value]");
    let lastSavedValue = slider.value;
    const refreshPreview = () => {
      planner.dataset.campFatiguePercent = slider.value;
      value.textContent = `${slider.value}%`;
      const forecasts = parseForecasts(planner.dataset.activeCampForecasts || planner.dataset.selectedCampForecasts);
      previewFor(
        planner.dataset.activeName || planner.dataset.selectedName,
        planner.dataset.activeDescription || planner.dataset.selectedDescription,
        Number(planner.dataset.activeMinutes || planner.dataset.selectedMinutes),
        planner.dataset.activeDistance,
        (forecasts.get(slider.value) || parseStops(planner.dataset.activeCampStops)).join(","),
        planner.dataset.activeCampForecasts,
        planner.dataset.activeRoundTrip === "true",
      );
    };
    const save = async () => {
      if (slider.value === lastSavedValue) return;
      try {
        const response = await fetch(travelConfiguration.action, {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: new URLSearchParams(new FormData(travelConfiguration)),
        });
        if (!response.ok) throw new Error("save failed");
        lastSavedValue = slider.value;
      } catch (_) {
        // The next slider change retries the save.
      }
    };
    slider.addEventListener("input", refreshPreview);
    slider.addEventListener("pointerup", save);
    slider.addEventListener("change", save);
  }

  const setPathRange = (path, startPercent, endPercent) => {
    const clamp = (value) => Math.max(0, Math.min(100, value)) / 100;
    const start = VERTICAL_PATH_START + (VERTICAL_PATH_END - VERTICAL_PATH_START) * clamp(startPercent);
    const end = VERTICAL_PATH_START + (VERTICAL_PATH_END - VERTICAL_PATH_START) * clamp(endPercent);
    path.setAttribute("d", `M 16 ${start} V ${end}`);
    path.style.removeProperty("stroke-dasharray");
  };
  const refreshProvisioning = () => {
    const journeyMinutes = Number(planner.dataset.provisionPlanningMinutes);
    const members = Number(planner.dataset.provisionLivingMembers);
    const foodDays = Number(planner.dataset.provisionFoodDays);
    const waterDays = Number(planner.dataset.provisionWaterDays);
    const totalMinutes = currentPlan?.totalMinutes || journeyMinutes;
    const completedMinutes = currentPlan?.completedMinutes || 0;
    const progressPercent = totalMinutes > 0 ? completedMinutes / totalMinutes * 100 : 0;
    setPathRange(planner.querySelector("[data-travel-progress]"), 0, progressPercent);
    if (![journeyMinutes, members, foodDays, waterDays, totalMinutes].every(Number.isFinite) || journeyMinutes <= 0 || totalMinutes <= 0 || members <= 0) {
      planner.querySelector("[data-travel-resource-meters]")?.setAttribute("hidden", "");
      return;
    }
    planner.querySelector("[data-travel-resource-meters]")?.removeAttribute("hidden");
    const journeyDays = totalMinutes / 1440;
    const target = Math.max(-365, Math.min(365, Number(targetInput?.value || 0)));
    [["food", foodDays], ["water", waterDays]].forEach(([kind, available]) => {
      const row = planner.querySelector(`.travel-resource-row.${kind}`);
      const availableEnd = progressPercent + available / journeyDays * 100;
      const remainingDays = Math.max(0, (totalMinutes - completedMinutes) / 1440);
      const targetEnd = progressPercent + (remainingDays + target) / journeyDays * 100;
      setPathRange(row.querySelector("[data-resource-fill]"), progressPercent, availableEnd);
      setPathRange(row.querySelector("[data-resource-target]"), progressPercent, targetEnd);
      const sign = target < 0 ? "negative" : target > 0 ? "positive" : "zero";
      row.dataset.targetSign = sign;
    });
    const rationKcal = Number(planner.dataset.provisionRationKcal);
    const skinMl = Number(planner.dataset.provisionWaterskinMl);
    const remainingDays = Math.max(0, (totalMinutes - completedMinutes) / 1440);
    const rations = rationKcal > 0 ? Math.ceil(Math.max(0, (remainingDays + target - foodDays) * members * 6000) / rationKcal) : 0;
    const skins = skinMl > 0 ? Math.ceil(Math.max(0, (remainingDays + target - waterDays) * members * 4000) / skinMl) : 0;
    const buy = document.querySelector("[data-provision-buy]");
    if (buy) {
      const params = new URLSearchParams({ inventory_scope: "party" });
      const stageable = rations <= MAX_U32 && skins <= MAX_U32;
      if (stageable && rations) params.set("provision_rations", String(rations));
      if (stageable && skins) params.set("provision_waterskins", String(skins));
      buy.href = stageable && (rations || skins) ? `${buy.dataset.marketPath}?${params}` : buy.dataset.marketPath;
      buy.dataset.empty = String(!(rations || skins));
      buy.dataset.unstageable = String(!stageable);
    }
  };
  targetInput?.addEventListener("input", refreshProvisioning);
  document.querySelector("[data-provision-buy]")?.addEventListener("click", (event) => {
    if (event.currentTarget.dataset.unstageable === "true") {
      event.preventDefault();
      document.querySelector("[data-provisioning-status]").textContent = "This target exceeds the maximum quantity that can be staged in one transaction.";
      return;
    }
    if (event.currentTarget.dataset.empty !== "true") return;
    event.preventDefault();
    document.querySelector("[data-provisioning-status]").textContent = "The party already meets this target; nothing was staged.";
  });
  refreshProvisioning();

  document.querySelectorAll("[data-rest-duration]").forEach((control) => {
    control.querySelectorAll("input[type=radio]").forEach((radio) => radio.addEventListener("change", () => {
      control.querySelectorAll(".rest-duration-unit").forEach((label) => label.classList.toggle("active", label.contains(radio) && radio.checked));
      control.querySelector("[data-rest-unit-label]").textContent = radio.value;
    }));
  });
})();
