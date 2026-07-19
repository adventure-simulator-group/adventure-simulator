(() => {
  const planner = document.querySelector("[data-travel-planner]");
  if (!planner) return;

  const route = planner.querySelector("[data-travel-planner-route]");
  const caption = planner.querySelector("[data-travel-planner-caption]");
  const targetInput = document.querySelector("[data-target-surplus]");
  let currentPlan = null;
  const MAX_U32 = 4294967295;
  const HORIZONTAL_PATH_START = 3;
  const HORIZONTAL_PATH_END = 97;

  const gameIcon = (name) => {
    const icon = document.createElement("span");
    icon.className = "game-icon";
    icon.style.setProperty("--game-icon", `url('/static/icons/game/${name}.svg')`);
    icon.setAttribute("aria-hidden", "true");
    return icon;
  };

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
  }) => {
    if (!destinationName || !totalMinutes) return;
    planner.dataset.activeName = destinationName;
    planner.dataset.activeMinutes = String(totalMinutes);
    planner.dataset.activeDistance = distance;
    planner.dataset.activeCampStops = campStops.join(",");

    const reached = new Set(reachedStops);
    const roundTrip = turnaroundMinutes > 0 && totalMinutes > turnaroundMinutes;
    const nodes = [
      { icon: "house", label: originName, kind: "start", minute: 0 },
      ...uniqueSortedStops(campStops, totalMinutes).map((minute, index) => ({
        icon: "camping-tent",
        label: `Camp ${index + 1}`,
        kind: "camp",
        minute,
        reached: reached.has(minute),
      })),
      { icon: "castle", label: destinationName, kind: "destination", minute: roundTrip ? turnaroundMinutes : totalMinutes },
      ...(roundTrip ? [{ icon: "house", label: originName, kind: "return", minute: totalMinutes }] : []),
    ];
    planner.classList.toggle("round-trip", roundTrip);
    route.replaceChildren(...nodes.sort((a, b) => a.minute - b.minute).map((node, index) => {
      const element = document.createElement("div");
      element.className = `travel-plan-node travel-plan-${node.kind}${node.reached ? " reached" : ""}`;
      const pin = document.createElement("span");
      pin.className = "travel-plan-pin";
      pin.append(gameIcon(node.icon));
      const label = document.createElement("span");
      label.className = "travel-plan-label";
      label.textContent = node.label;
      element.append(pin, label);
      const progress = node.minute / totalMinutes;
      const horizontal = 5 + progress * 90;
      element.style.left = `${horizontal}%`;
      element.style.top = "8%";
      if (index < nodes.length - 1) element.dataset.connects = "true";
      return element;
    }));
    const party = document.createElement("span");
    party.className = "travel-party-pin";
    party.setAttribute("role", "img");
    party.setAttribute("aria-label", "Traveling party");
    party.title = "Traveling party";
    party.append(gameIcon("person"));
    route.append(party);
    planner.hidden = false;

    const partyIndex = Math.max(0, nodes.findIndex((node) => node.minute === completedMinutes));
    const campCount = nodes.filter((node) => node.kind === "camp").length;
    caption.textContent = `${distance ? `${distance} · ` : ""}${campCount ? `${campCount} camp${campCount === 1 ? "" : "s"}` : "arrive before camp"}`;
    currentPlan = {
      party,
      nodes: [...route.querySelectorAll(".travel-plan-node")],
      partyIndex,
      campStops,
      originName,
    };
    requestAnimationFrame(() => {
      const currentNode = currentPlan.nodes[currentPlan.partyIndex] || currentPlan.nodes[0];
      party.style.left = `${currentNode.offsetLeft + currentNode.offsetWidth / 2 - party.offsetWidth / 2}px`;
      party.style.top = `${currentNode.offsetTop + currentNode.offsetHeight / 2 - party.offsetHeight / 2}px`;
    });
    return currentPlan;
  };

  const previewFor = (name, minutes, distance, stops, forecasts = "", roundTrip = false) => {
    planner.dataset.activeCampForecasts = forecasts;
    planner.dataset.activeRoundTrip = String(roundTrip);
    return showPlan({
      destinationName: name,
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
    });
    return true;
  };

  if (!showPersistedJourney()) {
    previewFor(
      planner.dataset.selectedName,
      Number(planner.dataset.selectedMinutes),
      "",
      planner.dataset.selectedCampStops,
      planner.dataset.selectedCampForecasts,
      Number(planner.dataset.provisionPlanningMinutes) > Number(planner.dataset.selectedMinutes),
    );
  }

  const animateNextLeg = () => {
    if (!currentPlan || currentPlan.partyIndex >= currentPlan.nodes.length - 1) return 0;
    const from = currentPlan.nodes[currentPlan.partyIndex];
    const to = currentPlan.nodes[currentPlan.partyIndex + 1];
    const start = from.offsetLeft + from.offsetWidth / 2 - currentPlan.party.offsetWidth / 2;
    const end = to.offsetLeft + to.offsetWidth / 2 - currentPlan.party.offsetWidth / 2;
    const startTop = from.offsetTop + from.offsetHeight / 2 - currentPlan.party.offsetHeight / 2;
    const endTop = to.offsetTop + to.offsetHeight / 2 - currentPlan.party.offsetHeight / 2;
    currentPlan.party.animate([
      { left: `${start}px`, top: `${startTop}px` },
      { left: `${end}px`, top: `${endTop}px` },
    ], {
      duration: 650,
      easing: "ease-in-out",
      fill: "forwards",
    });
    return 650;
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
    const status = travelConfiguration.querySelector("[data-travel-configuration-status]");
    let lastSavedValue = slider.value;
    const refreshPreview = () => {
      planner.dataset.campFatiguePercent = slider.value;
      value.textContent = `${slider.value}%`;
      const forecasts = parseForecasts(planner.dataset.activeCampForecasts || planner.dataset.selectedCampForecasts);
      previewFor(
        planner.dataset.activeName || planner.dataset.selectedName,
        Number(planner.dataset.activeMinutes || planner.dataset.selectedMinutes),
        planner.dataset.activeDistance,
        (forecasts.get(slider.value) || parseStops(planner.dataset.activeCampStops)).join(","),
        planner.dataset.activeCampForecasts,
        planner.dataset.activeRoundTrip === "true",
      );
    };
    const save = async () => {
      if (slider.value === lastSavedValue) return;
      status.textContent = "Saving…";
      try {
        const response = await fetch(travelConfiguration.action, {
          method: "POST",
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
          body: new URLSearchParams(new FormData(travelConfiguration)),
        });
        if (!response.ok) throw new Error("save failed");
        lastSavedValue = slider.value;
        status.textContent = "Saved.";
      } catch (_) {
        status.textContent = "Could not save the fatigue setting.";
      }
    };
    slider.addEventListener("input", refreshPreview);
    slider.addEventListener("pointerup", save);
    slider.addEventListener("change", save);
  }

  const formatDays = (days) => {
    if (Math.abs(days) < 0.0001) return "0";
    const rounded = Math.max(0.01, Math.round(Math.abs(days) * 100) / 100);
    return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(2).replace(/0+$/, "");
  };
  const setPathProgress = (path, percent) => {
    const progress = Math.max(0, Math.min(100, percent)) / 100;
    const end = HORIZONTAL_PATH_START + (HORIZONTAL_PATH_END - HORIZONTAL_PATH_START) * progress;
    path.setAttribute("d", `M ${HORIZONTAL_PATH_START} 16 H ${end}`);
    path.style.removeProperty("stroke-dasharray");
  };
  const targetDescription = (target, roundTrip) => {
    if (target < 0) return `Target: ${formatDays(target)} day${Math.abs(target) === 1 ? "" : "s"} short`;
    const endpoint = roundTrip ? "return" : "arrival";
    if (target > 0) return `Target: +${formatDays(target)} day${target === 1 ? "" : "s"} after ${endpoint}`;
    return `Target: exact ${endpoint}`;
  };
  const refreshProvisioning = () => {
    const journeyMinutes = Number(planner.dataset.provisionPlanningMinutes);
    const members = Number(planner.dataset.provisionLivingMembers);
    const foodDays = Number(planner.dataset.provisionFoodDays);
    const waterDays = Number(planner.dataset.provisionWaterDays);
    if (![journeyMinutes, members, foodDays, waterDays].every(Number.isFinite) || journeyMinutes <= 0 || members <= 0) {
      planner.querySelector("[data-travel-resource-meters]")?.setAttribute("hidden", "");
      return;
    }
    planner.querySelector("[data-travel-resource-meters]")?.removeAttribute("hidden");
    const journeyDays = journeyMinutes / 1440;
    const target = Math.max(-365, Math.min(365, Number(targetInput?.value || 0)));
    const roundTrip = planner.classList.contains("round-trip");
    [["food", foodDays], ["water", waterDays]].forEach(([kind, available]) => {
      const row = planner.querySelector(`.travel-resource-row.${kind}`);
      setPathProgress(row.querySelector("[data-resource-fill]"), available / journeyDays * 100);
      setPathProgress(row.querySelector("[data-resource-target]"), (journeyDays + target) / journeyDays * 100);
      const sign = target < 0 ? "negative" : target > 0 ? "positive" : "zero";
      row.dataset.targetSign = sign;
      row.querySelector("[data-resource-target-label]").textContent = targetDescription(target, roundTrip);
      const surplus = available - journeyDays;
      const amount = formatDays(surplus);
      row.querySelector("[data-resource-label]").textContent = `${amount} day${amount === "1" ? "" : "s"} ${surplus >= 0 ? "surplus" : "shortfall"}`;
    });
    const rationKcal = Number(planner.dataset.provisionRationKcal);
    const skinMl = Number(planner.dataset.provisionWaterskinMl);
    const rations = rationKcal > 0 ? Math.ceil(Math.max(0, (journeyDays + target - foodDays) * members * 6000) / rationKcal) : 0;
    const skins = skinMl > 0 ? Math.ceil(Math.max(0, (journeyDays + target - waterDays) * members * 4000) / skinMl) : 0;
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
