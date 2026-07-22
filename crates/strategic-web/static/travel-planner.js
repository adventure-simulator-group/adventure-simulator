(() => {
  const DAY = 1440;
  const DAYS_PER_YEAR = 365;
  const LUNAR_CYCLE = 42524;
  const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
  const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  const MONTH_DAYS = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  const MAX_U32 = 4294967295;
  const TRACK_START = 3;
  const TRACK_END = 97;
  const DAWN = 6 * 60;
  const DAYLIGHT = 8 * 60;
  const SUNSET = 18 * 60;
  const NIGHT = 20 * 60;
  let moonSequence = 0;
  const clamp = (value, low = 0, high = 1) => Math.max(low, Math.min(high, value));
  const parseStops = (value) => (value || "").split(",").map(Number).filter((n) => Number.isFinite(n) && n > 0);
  const parseSegments = (value) => (value || "").split("|").filter(Boolean).map((entry) => {
    const [kind, start, duration, movementStart, movementDuration, fatigueStart, fatigueEnd, fatigueMax, requiredRest] = entry.split(",");
    return { kind, start: Number(start), duration: Number(duration), movementStart: Number(movementStart), movementDuration: Number(movementDuration), fatigueStart: Number(fatigueStart), fatigueEnd: Number(fatigueEnd), fatigueMax: Number(fatigueMax), requiredRest: Number(requiredRest) };
  }).filter((segment) => [segment.start, segment.duration].every(Number.isFinite));
  const parseTerrain = (value) => {
    const entries = (value || "").split("|").filter(Boolean);
    const parsed = [];
    let cursor = 0;
    for (const entry of entries) {
      const fields = entry.split(",");
      if (fields.length !== 3) return [];
      const [kind, startText, durationText] = fields;
      const start = Number(startText);
      const duration = Number(durationText);
      if (!["road", "open", "sparse-woods", "deep-woods"].includes(kind)
          || !Number.isSafeInteger(start) || start < 0
          || !Number.isSafeInteger(duration) || duration <= 0
          || start !== cursor) return [];
      parsed.push({ kind, start, duration });
      cursor += duration;
    }
    return parsed;
  };
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
  const stepRangeValue = (value, direction, step, minimum, maximum) => {
    const precision = Math.max(0, (String(step).split('.')[1] || '').length);
    return Number(clamp(Number(value) + direction * step, minimum, maximum).toFixed(precision));
  };

  const fatigueBand = (fraction) => fraction >= 1 ? "stopped" : fraction >= .8 ? "red" : fraction >= .5 ? "yellow" : "green";
  const splitFatigueSegment = (segment) => {
    const delta = segment.fatigueEnd - segment.fatigueStart;
    const points = [0, 1];
    if (delta !== 0) {
      for (const threshold of [.5, .8, 1]) {
        const fraction = (threshold - segment.fatigueStart) / delta;
        if (fraction > 0 && fraction < 1) points.push(fraction);
      }
    }
    points.sort((left, right) => left - right);
    return points.slice(0, -1).map((startFraction, index) => {
      const endFraction = points[index + 1];
      const middleFatigue = segment.fatigueStart + delta * ((startFraction + endFraction) / 2);
      return {
        start: segment.start + segment.duration * startFraction,
        duration: segment.duration * (endFraction - startFraction),
        band: fatigueBand(middleFatigue),
      };
    });
  };
  const fatigueAtElapsed = (segments, elapsed) => {
    const segment = segments.find((candidate) => elapsed >= candidate.start && elapsed <= candidate.start + candidate.duration) || segments.at(-1);
    if (!segment) return 0;
    const fraction = segment.duration > 0 ? clamp((elapsed - segment.start) / segment.duration) : 1;
    return segment.fatigueStart + (segment.fatigueEnd - segment.fatigueStart) * fraction;
  };
  const timePeriodAt = (absoluteMinute) => {
    const minute = ((Math.floor(absoluteMinute) % DAY) + DAY) % DAY;
    if (minute >= DAWN && minute < DAYLIGHT) return "sunrise";
    if (minute >= DAYLIGHT && minute < SUNSET) return "day";
    if (minute >= SUNSET && minute < NIGHT) return "sunset";
    return "night";
  };
  const formatClock = (absoluteMinute) => {
    const minute = ((Math.floor(absoluteMinute) % DAY) + DAY) % DAY;
    return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
  };
  const attachRailTooltip = (track, valueAtFraction) => {
    const tooltip = document.createElement("span");
    tooltip.className = "travel-rail-tooltip";
    tooltip.setAttribute("role", "tooltip");
    tooltip.hidden = true;
    track.append(tooltip);
    track.onpointermove = (event) => {
      track.closest(".travel-resource-meters")?.querySelectorAll(".travel-rail-tooltip").forEach((candidate) => {
        if (candidate !== tooltip) candidate.hidden = true;
      });
      const rect = track.getBoundingClientRect();
      const fraction = clamp((event.clientY - rect.top) / rect.height);
      tooltip.textContent = valueAtFraction(fraction);
      tooltip.style.left = `${Math.min(event.clientX + 10, window.innerWidth - 104)}px`;
      tooltip.style.top = `${event.clientY}px`;
      tooltip.hidden = false;
    };
    track.onpointerleave = () => { tooltip.hidden = true; };
  };

  const calendarDate = (absoluteMinute) => {
    const absoluteDay = Math.floor(absoluteMinute / DAY);
    let dayOfYear = ((absoluteDay % DAYS_PER_YEAR) + DAYS_PER_YEAR) % DAYS_PER_YEAR;
    let monthIndex = 0;
    while (dayOfYear >= MONTH_DAYS[monthIndex]) {
      dayOfYear -= MONTH_DAYS[monthIndex];
      monthIndex += 1;
    }
    const weekdayIndex = ((absoluteDay % WEEKDAYS.length) + WEEKDAYS.length) % WEEKDAYS.length;
    return {
      weekday: WEEKDAYS[weekdayIndex],
      day: dayOfYear + 1,
      month: MONTHS[monthIndex],
      isSunday: weekdayIndex === 6,
    };
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
    track.replaceChildren();
    let elapsed = 0;
    while (elapsed < total) {
      const absolute = departure + elapsed;
      const dayStart = Math.floor(absolute / DAY) * DAY;
      const nextBoundary = [dayStart + DAWN, dayStart + DAYLIGHT, dayStart + SUNSET, dayStart + NIGHT, dayStart + DAY + DAWN]
        .find((boundary) => boundary > absolute);
      const end = Math.min(total, elapsed + Math.max(1, nextBoundary - absolute));
      const segment = document.createElement("span");
      segment.className = `travel-daylight-segment ${timePeriodAt(absolute)}`;
      segment.style.top = `${elapsed / total * 100}%`;
      segment.style.height = `${(end - elapsed) / total * 100}%`;
      track.append(segment);
      elapsed = end;
    }
    const firstMidnight = Math.ceil(departure / DAY) * DAY;
    for (let midnight = firstMidnight; midnight <= departure + total; midnight += DAY) {
      const tick = document.createElement("span");
      tick.className = "travel-midnight-tick";
      tick.style.top = `${(midnight - departure) / total * 100}%`;
      const date = calendarDate(midnight);
      tick.classList.toggle("sunday", date.isSunday);
      const label = document.createElement("span");
      label.className = "travel-calendar-label";
      const weekday = document.createElement("span");
      weekday.className = "travel-calendar-weekday";
      weekday.textContent = date.weekday;
      const calendarDay = document.createElement("span");
      calendarDay.className = "travel-calendar-date";
      calendarDay.textContent = `${date.day} ${date.month}`;
      label.append(weekday, calendarDay);
      tick.append(moonSvg(midnight), label);
      track.append(tick);
    }
    attachRailTooltip(track, (fraction) => {
      const absolute = departure + total * fraction;
      const period = timePeriodAt(absolute);
      return `${period[0].toUpperCase()}${period.slice(1)} · ${formatClock(absolute)}`;
    });
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
      for (const slice of splitFatigueSegment(segment)) {
        const part = document.createElement("span");
        part.className = `travel-fatigue-segment ${slice.band} ${segment.kind === "w" ? "walking" : "camp"}`;
        part.style.top = `${slice.start / total * 100}%`;
        part.style.height = `${slice.duration / total * 100}%`;
        track.append(part);
      }
    }
    attachRailTooltip(track, (fraction) => `Average fatigue · ${Math.round(fatigueAtElapsed(continuous, total * fraction) * 100)}%`);
    if (summary && Number.isFinite(minimum)) {
      summary.textContent = `${Math.round(minimum * 100)}–${Math.round(maximum * 100)}% · max ${Math.round(peak * 100)}%`;
      summary.title = `Average party fatigue ranges from ${Math.round(minimum * 100)}% to ${Math.round(maximum * 100)}%; highest member reaches ${Math.round(peak * 100)}%.`;
      summary.setAttribute("aria-label", summary.title);
      summary.closest(".travel-resource-row")?.classList.toggle("warning", peak >= 1);
    }
  };

  const renderTerrain = (planner, terrain, itinerary, total, movementTotal, roundTrip) => {
    const track = planner.querySelector("[data-terrain-track]");
    const summary = planner.querySelector("[data-terrain-summary]");
    const description = planner.querySelector("[data-terrain-course-description]");
    if (!track) return;
    track.replaceChildren();
    const labels = { road: "Road", open: "Open", "sparse-woods": "Sparse woods", "deep-woods": "Deep woods" };
    const pieces = terrainPieces(terrain, itinerary, movementTotal, roundTrip);
    for (const piece of pieces) {
      const node = document.createElement("span");
      node.className = `travel-terrain-segment ${piece.kind}`;
      node.style.top = `${piece.start / total * 100}%`;
      node.style.height = `${piece.duration / total * 100}%`;
      node.title = piece.kind === "stopped" ? "Camp · stopped" : labels[piece.kind];
      node.tabIndex = 0;
      node.setAttribute("aria-label", `${node.title}, elapsed minute ${Math.round(piece.start)} to ${Math.round(piece.start + piece.duration)}`);
      track.append(node);
    }
    if (description) {
      description.replaceChildren();
      for (const piece of pieces) {
        const item = document.createElement("li");
        const label = piece.kind === "stopped" ? "Camp, stopped" : labels[piece.kind];
        item.textContent = `${label}: elapsed minute ${Math.round(piece.start)} to ${Math.round(piece.start + piece.duration)}`;
        description.append(item);
      }
    }
    const ordered = [...new Set(terrain.map((span) => labels[span.kind]))];
    if (summary) summary.textContent = ordered.length ? `Terrain: ${ordered.join(", ")}` : "Terrain unavailable; legacy estimate";
    attachRailTooltip(track, (fraction) => Array.from(track.children).find((node) => fraction * 100 >= parseFloat(node.style.top) && fraction * 100 <= parseFloat(node.style.top) + parseFloat(node.style.height))?.title || "Terrain unavailable");
  };

  const terrainPieces = (terrain, itinerary, movementTotal, roundTrip) => {
    const pieces = [];
    for (const elapsed of itinerary) {
      if (elapsed.kind !== "w") {
        pieces.push({ kind: "stopped", start: elapsed.start, duration: elapsed.duration });
        continue;
      }
      for (const span of terrain) {
        const starts = [span.start];
        if (roundTrip) starts.push(movementTotal - span.start - span.duration);
        for (const routeStart of starts) {
          const overlapStart = Math.max(routeStart, elapsed.movementStart);
          const overlapEnd = Math.min(routeStart + span.duration, elapsed.movementStart + elapsed.movementDuration);
          if (overlapEnd <= overlapStart) continue;
          const ratio = elapsed.movementDuration > 0 ? elapsed.duration / elapsed.movementDuration : 0;
          pieces.push({ kind: span.kind, start: elapsed.start + (overlapStart - elapsed.movementStart) * ratio, duration: (overlapEnd - overlapStart) * ratio });
        }
      }
    }
    return pieces.sort((left, right) => left.start - right.start || left.kind.localeCompare(right.kind));
  };

  const initializeTravelPlanner = () => {
    const planner = document.querySelector("[data-travel-planner]");
    if (!planner || planner.dataset.travelPlannerReady === "true") return;
    planner.dataset.travelPlannerReady = "true";
    const route = planner.querySelector("[data-travel-planner-route]");
    const targetInput = document.querySelector("[data-target-surplus]");
    const targetDisplay = document.querySelector("[data-target-surplus-display]");
    const targetFromUrl = Number(new URLSearchParams(location.search).get("target_surplus"));
    if (targetInput && targetDisplay && Number.isFinite(targetFromUrl)) {
      const initialTarget = clamp(targetFromUrl, -365, 365);
      targetInput.value = String(initialTarget);
      targetDisplay.textContent = String(initialTarget);
    }
    let currentPlan;
    const terrain = parseTerrain(planner.dataset.terrainSpans);

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
          bracePath.setAttribute("d", "M 12 0 C 3 0 3 20 6 32 C 7 39 4 47 1 50 C 4 53 7 61 6 68 C 3 80 3 100 12 100");
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
      renderTerrain(planner, terrain, segments, elapsedTotal, movementTotal, roundTrip);
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
      const ordinaryWaterDays = Number(planner.dataset.provisionOrdinaryWaterDays);
      const emergencyAlcoholDays = Number(planner.dataset.provisionEmergencyAlcoholDays);
      if (![total, members, foodDays, waterDays].every(Number.isFinite) || total <= 0 || members <= 0) return;
      const target = clamp(Number(targetInput?.value || 0), -365, 365);
      const returnUrl = new URL(location.href);
      if (target) returnUrl.searchParams.set("target_surplus", String(Number(target.toFixed(2))));
      else returnUrl.searchParams.delete("target_surplus");
      history.replaceState(history.state, "", `${returnUrl.pathname}${returnUrl.search}${returnUrl.hash}`);
      const totalDays = total / DAY;
      const completedDays = completed / DAY;
      [["food", foodDays], ["water", waterDays]].forEach(([kind, available]) => {
        const row = planner.querySelector(`.travel-resource-row.${kind}`);
        setPathRange(row?.querySelector("[data-resource-fill]"), completed, Math.min(total, completed + available * DAY), total);
        setPathRange(row?.querySelector("[data-resource-target]"), completed, Math.min(total, completed + Math.max(0, totalDays - completedDays + target) * DAY), total);
        const surplus = available - (totalDays - completedDays);
        const label = row?.querySelector("[data-surplus-summary]");
        if (label) label.textContent = surplus >= 0 ? `${Number(surplus.toFixed(1))} day${Math.abs(surplus - 1) < .05 ? "" : "s"} surplus` : `${Number(Math.abs(surplus).toFixed(1))} days short`;
        if (kind === "water" && label && Number.isFinite(ordinaryWaterDays) && Number.isFinite(emergencyAlcoholDays)) {
          label.textContent += ` (${Number(ordinaryWaterDays.toFixed(1))} ordinary water + ${Number(emergencyAlcoholDays.toFixed(1))} emergency alcohol)`;
        }
      });
      const alcoholSummary = planner.querySelector("[data-emergency-alcohol-summary]");
      if (alcoholSummary && Number.isFinite(emergencyAlcoholDays)) alcoholSummary.textContent = `+${Number(emergencyAlcoholDays.toFixed(2))} d`;
      const rationKcal = Number(planner.dataset.provisionRationKcal);
      const skinMl = Number(planner.dataset.provisionWaterskinMl);
      const remainingDays = Math.max(0, totalDays - completedDays);
      const { rations, skins } = provisionQuantities({ remainingDays, target, foodDays, waterDays, members, rationKcal, skinMl });
      const buy = document.querySelector("[data-provision-buy]");
      if (buy) {
        const params = new URLSearchParams({ inventory_scope: "party" });
        params.set("return_to", `${returnUrl.pathname}${returnUrl.search}${returnUrl.hash}`);
        const stageable = rations <= MAX_U32 && skins <= MAX_U32;
        if (stageable && rations) params.set("provision_rations", String(rations));
        if (stageable && skins) params.set("provision_waterskins", String(skins));
        buy.href = stageable && (rations || skins) ? `${buy.dataset.marketPath}?${params}` : buy.dataset.marketPath;
        buy.dataset.empty = String(!(rations || skins));
        buy.dataset.unstageable = String(!stageable);
      }
    };
    targetInput?.addEventListener("input", refreshProvisioning);
    const formatDays = (value) => String(Number(Number(value).toFixed(2)));
    const parseDays = (value) => {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : null;
    };
    const editTargetSurplus = () => {
      if (!targetInput || !targetDisplay || !window.StrategicNumericEditor) return;
      window.StrategicNumericEditor.open({
        display: targetDisplay,
        initialValue: Number(targetInput.value || 0),
        parse: parseDays,
        format: formatDays,
        step: .25,
        minimum: -365,
        maximum: 365,
        groupLabel: "Edit target surplus",
        inputLabel: "Target surplus in days",
        increaseLabel: "Increase target surplus by one quarter day",
        decreaseLabel: "Decrease target surplus by one quarter day",
        saveLabel: "Save target surplus",
        cancelLabel: "Cancel target surplus edit",
        onCommit: (value) => {
          targetInput.value = formatDays(value);
          targetDisplay.textContent = formatDays(value);
          targetInput.dispatchEvent(new Event("input", { bubbles: true }));
        },
      });
    };
    targetDisplay?.addEventListener("click", editTargetSurplus);
    targetDisplay?.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        editTargetSurplus();
      }
    });
    document.querySelector("[data-provision-buy]")?.addEventListener("click", (event) => {
      const status = document.querySelector("[data-provisioning-status]");
      if (event.currentTarget.dataset.unstageable === "true") { event.preventDefault(); if (status) status.textContent = "This target exceeds the maximum transaction quantity."; }
      else if (event.currentTarget.dataset.empty === "true") { event.preventDefault(); if (status) status.textContent = "The party already meets this target; nothing was staged."; }
    });
    refreshProvisioning();

    const configuration = document.querySelector("form[data-travel-configuration]");
    if (configuration) {
      const walkingHours = configuration.querySelector("[data-walking-hours]");
      const walkingOutput = configuration.querySelector("[data-walking-hours-output]");
      walkingHours?.addEventListener("input", () => {
        if (walkingOutput) walkingOutput.textContent = formatDays(walkingHours.value);
      });
      const save = async () => {
        const response = await fetch(configuration.action, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: new URLSearchParams(new FormData(configuration)) });
        if (response.ok) window.location.reload();
      };
      let walkingWheelSave;
      walkingHours?.addEventListener("wheel", (event) => {
        event.preventDefault();
        const next = stepRangeValue(
          walkingHours.value,
          event.deltaY < 0 ? 1 : -1,
          Number(walkingHours.step) || .25,
          Number(walkingHours.min) || 0,
          Number(walkingHours.max) || 24,
        );
        if (String(next) === walkingHours.value) return;
        walkingHours.value = String(next);
        walkingHours.dispatchEvent(new Event("input", { bubbles: true }));
        window.clearTimeout(walkingWheelSave);
        walkingWheelSave = window.setTimeout(() => {
          walkingHours.dispatchEvent(new Event("change", { bubbles: true }));
        }, 250);
      }, { passive: false });
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
