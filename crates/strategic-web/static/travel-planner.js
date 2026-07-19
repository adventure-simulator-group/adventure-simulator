(() => {
  const DAY = 1440;
  const LUNAR_CYCLE = 42524;
  const MAX_U32 = 4294967295;
  const TRACK_START = 3;
  const TRACK_END = 97;
  let moonSequence = 0;
  const clamp = (value, low = 0, high = 1) => Math.max(low, Math.min(high, value));
  const parseStops = (value) => (value || "").split(",").map(Number).filter((n) => Number.isFinite(n) && n > 0);
  const parseSegments = (value) => (value || "").split("|").filter(Boolean).map((entry) => {
    const [kind, start, duration, movementStart, movementDuration, fatigueStart, fatigueEnd, fatigueMax, requiredRest] = entry.split(",");
    return { kind, start: Number(start), duration: Number(duration), movementStart: Number(movementStart), movementDuration: Number(movementDuration), fatigueStart: Number(fatigueStart), fatigueEnd: Number(fatigueEnd), fatigueMax: Number(fatigueMax), requiredRest: Number(requiredRest) };
  }).filter((segment) => [segment.start, segment.duration].every(Number.isFinite));
  const position = (minute, total) => TRACK_START + (TRACK_END - TRACK_START) * clamp(total > 0 ? minute / total : 0);
  const setPathRange = (path, start, end, total) => {
    if (!path) return;
    path.setAttribute("d", `M 16 ${position(start, total)} V ${position(end, total)}`);
  };
  const turnaroundElapsed = (segments, oneWay, fallback) => {
    for (const segment of segments) {
      if (segment.kind === "w" && segment.movementStart + segment.movementDuration >= oneWay) {
        const fraction = segment.movementDuration > 0 ? (oneWay - segment.movementStart) / segment.movementDuration : 0;
        return segment.start + segment.duration * clamp(fraction);
      }
    }
    return fallback;
  };
  const provisionQuantities = ({ remainingDays, target, foodDays, waterDays, members, rationKcal, skinMl }) => ({
    rations: rationKcal > 0 ? Math.ceil(Math.max(0, (remainingDays + target - foodDays) * members * 6000) / rationKcal) : 0,
    skins: skinMl > 0 ? Math.ceil(Math.max(0, (remainingDays + target - waterDays) * members * 4000) / skinMl) : 0,
  });

  const fatigueColor = (fraction) => {
    // Fatigue may legitimately exceed capacity. Keep the authoritative value
    // for warnings, but saturate the display at red so color-mix percentages
    // never become negative and invalidate the whole segment background.
    const value = clamp(fraction, 0, 1);
    if (value <= .5) return `color-mix(in srgb, #58b66b ${Math.round((1 - value * 2) * 100)}%, #e0c54f)`;
    return `color-mix(in srgb, #e0c54f ${Math.round((1 - (value - .5) * 2) * 100)}%, #d65757)`;
  };

  const moonName = (phase) => {
    if (phase < .0625 || phase >= .9375) return "new moon";
    if (phase < .1875) return "waxing crescent";
    if (phase < .3125) return "first quarter";
    if (phase < .4375) return "waxing gibbous";
    if (phase < .5625) return "full moon";
    if (phase < .6875) return "waning gibbous";
    if (phase < .8125) return "last quarter";
    return "waning crescent";
  };
  const moonGeometry = (phase) => {
    const illumination = (1 - Math.cos(Math.PI * 2 * phase)) / 2;
    const waxing = phase <= .5;
    const halfPhase = waxing ? phase : 1 - phase;
    const terminatorRadius = Math.abs(Math.cos(Math.PI * 2 * halfPhase) * 8);
    const terminatorSweep = halfPhase < .25 ? 0 : 1;
    return {
      illumination,
      path: illumination <= .0001 ? "" : illumination >= .9999 ? "M 10 2 A 8 8 0 1 1 9.999 2 Z" : `M 10 2 A 8 8 0 0 1 10 18 A ${terminatorRadius.toFixed(3)} 8 0 0 ${terminatorSweep} 10 2 Z`,
      transform: waxing ? "" : "translate(20 0) scale(-1 1)",
    };
  };

  const moonSvg = (absoluteMinute) => {
    const phase = ((absoluteMinute % LUNAR_CYCLE) + LUNAR_CYCLE) % LUNAR_CYCLE / LUNAR_CYCLE;
    const geometry = moonGeometry(phase);
    const illumination = geometry.illumination;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 20 20");
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", `${moonName(phase)}, ${Math.round(illumination * 100)}% illuminated`);
    const title = document.createElementNS(svg.namespaceURI, "title");
    title.textContent = svg.getAttribute("aria-label");
    const clipId = `travel-moon-clip-${moonSequence++}`;
    const defs = document.createElementNS(svg.namespaceURI, "defs");
    const clip = document.createElementNS(svg.namespaceURI, "clipPath");
    clip.setAttribute("id", clipId);
    const clipCircle = document.createElementNS(svg.namespaceURI, "circle");
    clipCircle.setAttribute("cx", "10"); clipCircle.setAttribute("cy", "10"); clipCircle.setAttribute("r", "8");
    clip.append(clipCircle); defs.append(clip);
    const dark = document.createElementNS(svg.namespaceURI, "circle");
    dark.setAttribute("cx", "10"); dark.setAttribute("cy", "10"); dark.setAttribute("r", "8"); dark.setAttribute("class", "moon-dark");
    const lit = document.createElementNS(svg.namespaceURI, "path");
    lit.setAttribute("d", geometry.path);
    lit.setAttribute("class", "moon-lit");
    lit.setAttribute("clip-path", `url(#${clipId})`);
    if (geometry.transform) lit.setAttribute("transform", geometry.transform);
    svg.append(title, defs, dark, lit);
    return svg;
  };

  const renderTimeRail = (planner, departure, total) => {
    const track = planner.querySelector("[data-daylight-track]");
    if (!track || total <= 0) return;
    const stops = [];
    const samples = Math.max(2, Math.min(256, Math.ceil(total / 60)));
    for (let index = 0; index <= samples; index += 1) {
      const elapsed = total * index / samples;
      const hour = ((departure + elapsed) % DAY) / 60;
      const daylight = hour >= 6 && hour < 18;
      const progress = daylight ? (hour - 6) / 12 : ((hour + 6) % 24) / 12;
      const color = daylight
        ? `color-mix(in srgb, #f5cc68 ${Math.round((1 - Math.abs(progress - .5) * 2) * 75 + 25)}%, #77a8ca)`
        : `color-mix(in srgb, #13233f ${Math.round((1 - Math.abs(progress - .5) * 2) * 70 + 30)}%, #465b78)`;
      stops.push(`${color} ${(index / samples * 100).toFixed(2)}%`);
    }
    track.style.background = `linear-gradient(to bottom, ${stops.join(",")})`;
    track.replaceChildren();
    const firstMidnight = Math.ceil(departure / DAY) * DAY;
    for (let midnight = firstMidnight; midnight <= departure + total; midnight += DAY) {
      const tick = document.createElement("span");
      tick.className = "travel-midnight-tick";
      tick.style.top = `${(midnight - departure) / total * 100}%`;
      tick.append(moonSvg(midnight));
      track.append(tick);
    }
  };

  const renderFatigue = (planner, segments, total) => {
    const track = planner.querySelector("[data-fatigue-track]");
    const summary = planner.querySelector("[data-fatigue-summary]");
    if (!track) return;
    track.replaceChildren();
    let minimum = Infinity, maximum = 0, peak = 0;
    const ordered = [...segments].sort((left, right) => left.start - right.start);
    const continuous = [];
    let cursor = 0;
    let fatigue = ordered[0]?.fatigueStart || 0;
    for (const segment of ordered) {
      if (segment.start > cursor) continuous.push({ kind: "w", start: cursor, duration: segment.start - cursor, fatigueStart: fatigue, fatigueEnd: segment.fatigueStart, fatigueMax: Math.max(fatigue, segment.fatigueStart) });
      continuous.push(segment);
      cursor = Math.max(cursor, segment.start + segment.duration);
      fatigue = segment.fatigueEnd;
    }
    if (cursor < total) continuous.push({ kind: "w", start: cursor, duration: total - cursor, fatigueStart: fatigue, fatigueEnd: fatigue, fatigueMax: fatigue });
    for (const segment of continuous) {
      minimum = Math.min(minimum, segment.fatigueStart, segment.fatigueEnd);
      maximum = Math.max(maximum, segment.fatigueStart, segment.fatigueEnd);
      peak = Math.max(peak, segment.fatigueMax, segment.fatigueStart, segment.fatigueEnd);
      const part = document.createElement("span");
      part.className = `travel-fatigue-segment ${segment.kind === "w" ? "walking" : "camp"}`;
      part.style.top = `${segment.start / total * 100}%`;
      part.style.height = `${segment.duration / total * 100}%`;
      part.style.background = `linear-gradient(to bottom, ${fatigueColor(segment.fatigueStart)}, ${fatigueColor(segment.fatigueEnd)})`;
      track.append(part);
    }
    if (summary && Number.isFinite(minimum)) {
      summary.textContent = `${Math.round(minimum * 100)}–${Math.round(maximum * 100)}% · max ${Math.round(peak * 100)}%`;
      summary.title = `Average party fatigue ranges from ${Math.round(minimum * 100)}% to ${Math.round(maximum * 100)}%; highest member reaches ${Math.round(peak * 100)}%.`;
      summary.setAttribute("aria-label", summary.title);
      summary.closest(".travel-resource-row")?.classList.toggle("warning", peak >= 1);
    }
  };

  const initializeTravelPlanner = () => {
    const planner = document.querySelector("[data-travel-planner]");
    if (!planner || planner.dataset.travelPlannerReady === "true") return;
    planner.dataset.travelPlannerReady = "true";
    const route = planner.querySelector("[data-travel-planner-route]");
    const targetInput = document.querySelector("[data-target-surplus]");
    let currentPlan;

    const showPlan = ({ name, origin = "Start", oneWay, movementTotal, elapsedTotal, completedElapsed = 0, departure = 0, segments = [], description = "", roundTrip = movementTotal > oneWay }) => {
      if (!name || elapsedTotal <= 0) { planner.hidden = true; return; }
      const turnaround = turnaroundElapsed(segments, oneWay, elapsedTotal);
      const camps = segments.filter((segment) => segment.kind !== "w");
      const nodes = [
        { kind: "start", label: "Start", title: origin, minute: 0 },
        ...camps.map((camp, index) => ({ kind: "camp", label: `Camp ${index + 1}`, title: `Camp ${index + 1}: ${Math.ceil(camp.duration / 60)}h; ${Math.round(camp.fatigueEnd * 100)}% average fatigue after rest`, minute: camp.start, duration: camp.duration, completed: camp.kind === "a" || camp.start + camp.duration <= completedElapsed, partial: camp.kind === "m" })),
        { kind: "destination", label: roundTrip ? "Quest" : "End", title: description || name, minute: roundTrip ? turnaround : elapsedTotal },
        ...(roundTrip ? [{ kind: "return", label: "End", title: origin, minute: elapsedTotal }] : []),
      ].sort((a, b) => a.minute - b.minute);
      route.replaceChildren(...nodes.map((node) => {
        const element = document.createElement("div");
        element.className = `travel-plan-node travel-plan-${node.kind}${node.completed ? " reached" : ""}${node.partial ? " partial" : ""}`;
        element.style.top = `${position(node.minute, elapsedTotal)}%`;
        element.title = node.title;
        element.setAttribute("role", "separator");
        element.setAttribute("aria-label", node.title);
        if (node.kind === "camp") {
          element.style.height = `${(TRACK_END - TRACK_START) * node.duration / elapsedTotal}%`;
          const tent = document.createElement("span"); tent.className = "travel-camp-tent"; tent.setAttribute("aria-hidden", "true");
          const brace = document.createElementNS("http://www.w3.org/2000/svg", "svg");
          brace.setAttribute("class", "travel-camp-brace"); brace.setAttribute("viewBox", "0 0 12 100"); brace.setAttribute("preserveAspectRatio", "none"); brace.setAttribute("aria-hidden", "true");
          const bracePath = document.createElementNS(brace.namespaceURI, "path");
          // Open the brace toward the rails. Its two right-hand tips mark the
          // exact camp bounds; the outside cusp is the tent's anchor.
          bracePath.setAttribute("d", "M 11 0 C 3 0 3 20 6 32 C 7 39 4 47 1 50 C 4 53 7 61 6 68 C 3 80 3 100 11 100");
          brace.append(bracePath);
          element.append(tent, brace);
        } else {
          const label = document.createElement("span"); label.className = "travel-plan-endpoint-label"; label.textContent = node.label; element.append(label);
        }
        return element;
      }));
      planner.hidden = false;
      setPathRange(planner.querySelector("[data-travel-progress]"), 0, completedElapsed, elapsedTotal);
      renderFatigue(planner, segments, elapsedTotal);
      renderTimeRail(planner, departure, elapsedTotal);
      currentPlan = { elapsedTotal, completedElapsed };
    };

    const selectedSegments = parseSegments(planner.dataset.itinerarySegments);
    const selectedOneWay = Number(planner.dataset.selectedMinutes);
    showPlan({
      name: planner.dataset.journeyDestinationName || planner.dataset.selectedName,
      origin: planner.dataset.journeyOriginName || "Start",
      oneWay: Number(planner.dataset.journeyTurnaroundMinutes) || selectedOneWay,
      movementTotal: Number(planner.dataset.journeyTotalMinutes) || (Number(planner.dataset.provisionPlanningMinutes) > selectedOneWay ? selectedOneWay * 2 : selectedOneWay),
      elapsedTotal: Number(planner.dataset.totalElapsedMinutes) || Number(planner.dataset.provisionPlanningMinutes),
      completedElapsed: Number(planner.dataset.completedElapsedMinutes) || 0,
      departure: Number(planner.dataset.departureMinute) || 0,
      segments: selectedSegments,
      description: planner.dataset.selectedDescription,
      roundTrip: Number(planner.dataset.journeyTotalMinutes) > 0
        ? Number(planner.dataset.journeyTotalMinutes) > (Number(planner.dataset.journeyTurnaroundMinutes) || selectedOneWay)
        : planner.dataset.selectedRoundTrip === "true",
    });

    const refreshProvisioning = () => {
      const total = currentPlan?.elapsedTotal || Number(planner.dataset.provisionPlanningMinutes);
      const completed = currentPlan?.completedElapsed || 0;
      const members = Number(planner.dataset.provisionLivingMembers);
      const foodDays = Number(planner.dataset.provisionFoodDays);
      const waterDays = Number(planner.dataset.provisionWaterDays);
      if (![total, members, foodDays, waterDays].every(Number.isFinite) || total <= 0 || members <= 0) return;
      const target = clamp(Number(targetInput?.value || 0), -365, 365);
      const totalDays = total / DAY;
      const completedDays = completed / DAY;
      [["food", foodDays], ["water", waterDays]].forEach(([kind, available]) => {
        const row = planner.querySelector(`.travel-resource-row.${kind}`);
        setPathRange(row?.querySelector("[data-resource-fill]"), completed, Math.min(total, completed + available * DAY), total);
        setPathRange(row?.querySelector("[data-resource-target]"), completed, Math.min(total, completed + Math.max(0, totalDays - completedDays + target) * DAY), total);
        const surplus = available - (totalDays - completedDays);
        const label = row?.querySelector("[data-surplus-summary]");
        if (label) label.textContent = surplus >= 0 ? `${Number(surplus.toFixed(1))} day${Math.abs(surplus - 1) < .05 ? "" : "s"} surplus` : `${Number(Math.abs(surplus).toFixed(1))} days short`;
      });
      const rationKcal = Number(planner.dataset.provisionRationKcal);
      const skinMl = Number(planner.dataset.provisionWaterskinMl);
      const remainingDays = Math.max(0, totalDays - completedDays);
      const { rations, skins } = provisionQuantities({ remainingDays, target, foodDays, waterDays, members, rationKcal, skinMl });
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
      const status = document.querySelector("[data-provisioning-status]");
      if (event.currentTarget.dataset.unstageable === "true") { event.preventDefault(); if (status) status.textContent = "This target exceeds the maximum transaction quantity."; }
      else if (event.currentTarget.dataset.empty === "true") { event.preventDefault(); if (status) status.textContent = "The party already meets this target; nothing was staged."; }
    });
    refreshProvisioning();

    const configuration = document.querySelector("form[data-travel-configuration]");
    if (configuration) {
      const save = async () => {
        const response = await fetch(configuration.action, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: new URLSearchParams(new FormData(configuration)) });
        if (response.ok) window.location.reload();
      };
      configuration.querySelectorAll("input").forEach((input) => input.addEventListener("change", save));
    }

    document.querySelectorAll("form[data-travel-submit]").forEach((form) => form.addEventListener("submit", async (event) => {
      event.preventDefault();
      if (form.dataset.submitting) return;
      form.dataset.submitting = "true";
      const response = await fetch(form.action, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: new URLSearchParams(new FormData(form)) });
      if (response.ok) window.location.assign(response.status === 202 ? window.location.href : (new URL(form.action).pathname === "/camp/continue" ? "/" : "/camp"));
      else { form.dataset.submitting = ""; const status = form.parentElement?.querySelector("[data-travel-action-status]"); if (status) { status.hidden = false; status.textContent = await response.text(); } }
    }));
    document.querySelectorAll("[data-rest-duration]").forEach((control) => control.querySelectorAll("input[type=radio]").forEach((radio) => radio.addEventListener("change", () => {
      control.querySelectorAll(".rest-duration-unit").forEach((label) => label.classList.toggle("active", label.contains(radio) && radio.checked));
      control.querySelector("[data-rest-unit-label]").textContent = radio.value;
    })));
  };

  initializeTravelPlanner();
  document.addEventListener("strategic-live-regions-refreshed", (event) => { if (event.detail?.regions?.includes("right-sidebar")) initializeTravelPlanner(); });
})();
