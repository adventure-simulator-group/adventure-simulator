(() => {
  let baseTitle = document.title.replace(/^\(\d+\)\s*/, "");

  const gameIcon = (name) => {
    const icon = document.createElement("span");
    icon.className = "game-icon";
    icon.style.setProperty("--game-icon", `url('/static/icons/game/${name}.svg')`);
    icon.setAttribute("aria-hidden", "true");
    return icon;
  };

  async function refreshPartyNotifications() {
    try {
      const response = await window.strategicBackgroundFetch("party-notifications", "/party-notifications", {
        headers: { Accept: "application/json" },
      });
      if (!response.ok) return;

      const { pending_join_requests: joins = 0, role_join_requests: roles = [], action_requests: actions = [], succession_required: succession = false, leader_id: leaderId = null, leader_votes: votes = [] } = await response.json();
      const count = joins + actions.length;
      document.title = count > 0 ? `(${count}) ${baseTitle}` : baseTitle;
      const counts = new Map(roles.map((role) => [String(role.role_id), role.count]));
      document.querySelectorAll("[data-party-role-notification-badge]").forEach((badge) => {
        const roleCount = counts.get(badge.dataset.roleId) || 0;
        badge.textContent = String(roleCount);
        badge.hidden = roleCount === 0;
      });
      document.querySelectorAll("[data-party-action-notifications], [data-party-succession-vote]").forEach((node) => node.remove());
      for (const request of actions) {
        const portrait = document.querySelector(`[data-character-id="${request.requester_id}"]`);
        if (!portrait) continue;
        let list = portrait.querySelector("[data-party-action-notifications]");
        if (!list) {
          list = document.createElement("div");
          list.className = "party-action-notifications";
          list.dataset.partyActionNotifications = "true";
          portrait.append(list);
        }
        const item = document.createElement("div");
        item.className = "party-action-request";
        const summary = document.createElement("span");
        summary.textContent = request.summary;
        item.append(summary);
        for (const [verb, label, icon] of [["approve", "Approve", "check-mark"], ["deny", "Deny", "cross-mark"]]) {
          const form = document.createElement("form");
          form.method = "post";
          form.action = `/party-action-requests/${request.id}/${verb}`;
          const button = document.createElement("button");
          button.title = label;
          button.setAttribute("aria-label", label);
          button.append(gameIcon(icon));
          form.append(button);
          item.append(form);
        }
        list.append(item);
      }
      const activePortrait = document.querySelector("[data-active-character]");
      if (activePortrait?.dataset.characterAlive === "true") {
        const ownVote = votes.find((vote) => String(vote.voter_id) === activePortrait.dataset.characterId);
        document.querySelectorAll('.party-portrait[data-character-id][data-character-alive="true"]').forEach((portrait) => {
          const selected = String(ownVote?.candidate_id) === portrait.dataset.characterId;
          const currentLeader = String(leaderId) === portrait.dataset.characterId;
          const voteLabel = `Vote for ${portrait.title} as party leader`;
          if (selected && currentLeader) {
            const indicator = document.createElement("span");
            indicator.className = "party-succession-vote selected current-leader vote-indicator";
            indicator.dataset.partySuccessionVote = "true";
            indicator.title = `Your leadership vote is assigned to ${portrait.title}`;
            indicator.setAttribute("role", "img");
            indicator.setAttribute("aria-label", indicator.title);
            indicator.append(gameIcon("crown"));
            portrait.prepend(indicator);
            return;
          }
          const form = document.createElement("form");
          form.method = "post";
          form.action = `/party-leader-votes/${portrait.dataset.characterId}`;
          form.className = `party-succession-vote${selected ? " selected" : ""}${currentLeader ? " current-leader" : ""}`;
          form.dataset.partySuccessionVote = "true";
          const voteButton = document.createElement("button");
          voteButton.setAttribute("aria-pressed", String(selected));
          voteButton.append(gameIcon("crown"));
          form.append(voteButton);
          voteButton.title = voteLabel;
          voteButton.setAttribute("aria-label", voteLabel);
          form.addEventListener("submit", async (event) => {
            event.preventDefault();
            if (form.classList.contains("dropping")) return;
            form.classList.add("dropping");
            voteButton.setAttribute("aria-pressed", "true");
            await new Promise((resolve) => window.setTimeout(resolve, 240));
            try {
              const voteResponse = await fetch(form.action, { method: "POST" });
              if (!voteResponse.ok) throw new Error(`Leadership vote failed: ${voteResponse.status}`);
              await refreshPartyNotifications();
            } catch {
              form.classList.remove("dropping");
              voteButton.setAttribute("aria-pressed", String(selected));
            }
          });
          portrait.prepend(form);
        });
      }
    } catch {
      // Notifications are supplementary; page navigation should remain unaffected.
    }
  }

  window.queueStrategicInitialLoad(refreshPartyNotifications);
  const requested = new URLSearchParams(location.search).get("party-requested");
  if (requested) {
    const notice = document.createElement("div");
    notice.className = "party-requested-toast";
    notice.textContent = `Requested: ${requested.replaceAll("_", " ")}`;
    document.body.append(notice);
    window.setTimeout(() => notice.remove(), 3000);
  }
  document.addEventListener("strategic-live-update", refreshPartyNotifications);
  document.addEventListener("strategic-page-mounted", () => {
    baseTitle = document.title.replace(/^\(\d+\)\s*/, "");
    refreshPartyNotifications();
  });
  document.addEventListener("strategic-live-regions-refreshed", (event) => {
    if (event.detail?.regions?.includes("party-portraits")) refreshPartyNotifications();
  });
})();
