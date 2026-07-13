(() => {
  const baseTitle = document.title.replace(/^\(\d+\)\s*/, "");

  async function refreshPartyNotifications() {
    try {
      const response = await fetch("/party-notifications", {
        headers: { Accept: "application/json" },
      });
      if (!response.ok) return;

      const { pending_join_requests: count = 0, role_join_requests: roles = [] } = await response.json();
      document.title = count > 0 ? `(${count}) ${baseTitle}` : baseTitle;
      const counts = new Map(roles.map((role) => [String(role.role_id), role.count]));
      document.querySelectorAll("[data-party-role-notification-badge]").forEach((badge) => {
        const roleCount = counts.get(badge.dataset.roleId) || 0;
        badge.textContent = String(roleCount);
        badge.hidden = roleCount === 0;
      });
    } catch {
      // Notifications are supplementary; page navigation should remain unaffected.
    }
  }

  refreshPartyNotifications();
  window.setInterval(refreshPartyNotifications, 5000);
})();
