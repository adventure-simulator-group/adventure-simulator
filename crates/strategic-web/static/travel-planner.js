(() => {
  const planner = document.querySelector("[data-travel-planner]");
  if (!planner) return;

  const route = planner.querySelector("[data-travel-planner-route]");
  const caption = planner.querySelector("[data-travel-planner-caption]");
  let currentPlan = null;

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
  }) => {
    if (!destinationName || !totalMinutes) return;
    planner.dataset.activeName = destinationName;
    planner.dataset.activeMinutes = String(totalMinutes);
    planner.dataset.activeDistance = distance;
    planner.dataset.activeCampStops = campStops.join(",");

    const reached = new Set(reachedStops);
    const nodes = [
      { icon: "house", label: originName, kind: "start", minute: 0 },
      ...uniqueSortedStops(campStops, totalMinutes).map((minute, index) => ({
        icon: "camping-tent",
        label: `Camp ${index + 1}`,
        kind: "camp",
        minute,
        reached: reached.has(minute),
      })),
      { icon: "castle", label: destinationName, kind: "destination", minute: totalMinutes },
    ];
    route.replaceChildren(...nodes.map((node, index) => {
      const element = document.createElement("div");
      element.className = `travel-plan-node travel-plan-${node.kind}${node.reached ? " reached" : ""}`;
      const pin = document.createElement("span");
      pin.className = "travel-plan-pin";
      pin.append(gameIcon(node.icon));
      const label = document.createElement("span");
      label.className = "travel-plan-label";
      label.textContent = node.label;
      element.append(pin, label);
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
    });
    return currentPlan;
  };

  const previewFor = (name, minutes, distance, stops, forecasts = "") => {
    planner.dataset.activeCampForecasts = forecasts;
    return showPlan({
      destinationName: name,
      totalMinutes: minutes,
      distance,
      campStops: parseStops(stops),
    });
  };

  document.querySelectorAll(".travel-destination-row").forEach((row) => {
    const show = () => previewFor(
      row.dataset.travelName,
      Number(row.dataset.travelMinutes),
      row.dataset.travelDistance,
      row.dataset.travelCampStops,
      row.dataset.travelCampForecasts,
    );
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
    );
  }

  const animateNextLeg = () => {
    if (!currentPlan || currentPlan.partyIndex >= currentPlan.nodes.length - 1) return 0;
    const from = currentPlan.nodes[currentPlan.partyIndex];
    const to = currentPlan.nodes[currentPlan.partyIndex + 1];
    const start = from.offsetLeft + from.offsetWidth / 2 - currentPlan.party.offsetWidth / 2;
    const end = to.offsetLeft + to.offsetWidth / 2 - currentPlan.party.offsetWidth / 2;
    currentPlan.party.animate([{ left: `${start}px` }, { left: `${end}px` }], {
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

  document.querySelectorAll("[data-rest-duration]").forEach((control) => {
    control.querySelectorAll("input[type=radio]").forEach((radio) => radio.addEventListener("change", () => {
      control.querySelectorAll(".rest-duration-unit").forEach((label) => label.classList.toggle("active", label.contains(radio) && radio.checked));
      control.querySelector("[data-rest-unit-label]").textContent = radio.value;
    }));
  });
})();
