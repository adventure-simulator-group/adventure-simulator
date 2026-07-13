(() => {
  const baseTitle = document.title.replace(/^\(\d+\)\s*/, "");

  async function refreshPartyNotifications() {
    try {
      const response = await fetch("/party-notifications", {
        headers: { Accept: "application/json" },
      });
      if (!response.ok) return;

      const { pending_join_requests: count = 0 } = await response.json();
      document.title = count > 0 ? `(${count}) ${baseTitle}` : baseTitle;
      document.querySelectorAll("[data-party-notification-badge]").forEach((badge) => {
        badge.textContent = String(count);
        badge.hidden = count === 0;
      });
    } catch {
      // Notifications are supplementary; page navigation should remain unaffected.
    }
  }

  refreshPartyNotifications();
  window.setInterval(refreshPartyNotifications, 5000);
})();
