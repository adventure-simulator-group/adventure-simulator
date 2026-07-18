(() => {
  const baseTitle = document.title.replace(/^\(\d+\)\s*/, "");

  async function refreshPartyNotifications() {
    try {
      const response = await window.strategicBackgroundFetch("party-notifications", "/party-notifications", {
        headers: { Accept: "application/json" },
      });
      if (!response.ok) return;

      const { pending_join_requests: joins = 0, role_join_requests: roles = [], action_requests: actions = [], succession_required: succession = false, leader_votes: votes = [] } = await response.json();
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
        for (const [verb, label, glyph] of [["approve", "Approve", "✓"], ["deny", "Deny", "×"]]) {
          const form = document.createElement("form");
          form.method = "post";
          form.action = `/party-action-requests/${request.id}/${verb}`;
          const button = document.createElement("button");
          button.title = label;
          button.setAttribute("aria-label", label);
          button.textContent = glyph;
          form.append(button);
          item.append(form);
        }
        list.append(item);
      }
      const activePortrait = document.querySelector("[data-active-character]");
      if (activePortrait?.dataset.characterAlive === "true") {
        const ownVote = votes.find((vote) => String(vote.voter_id) === activePortrait.dataset.characterId);
        document.querySelectorAll('.party-portrait[data-character-id][data-character-alive="true"]').forEach((portrait) => {
          if (portrait.dataset.characterId === activePortrait.dataset.characterId) return;
          const form = document.createElement("form");
          form.method = "post";
          form.action = `/party-leader-votes/${portrait.dataset.characterId}`;
          form.className = "party-succession-vote";
          form.dataset.partySuccessionVote = "true";
          form.innerHTML = `<button title="Assign leadership vote to this member" aria-label="Assign leadership vote to this member" aria-pressed="${String(ownVote?.candidate_id) === portrait.dataset.characterId}" class="${String(ownVote?.candidate_id) === portrait.dataset.characterId ? "selected" : ""}">✓</button>`;
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
})();
