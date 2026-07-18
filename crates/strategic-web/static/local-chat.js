(() => {
  const CHANNEL_LABELS = Object.freeze({
    local: "Local",
    party: "Party",
    settlement: "Settlement",
    dm: "DM",
    guild: "Guild",
    info: "Info",
  });

  const chatTimestamp = (row) => {
    const value = Number(row.dataset.chatCreatedMicros);
    return Number.isFinite(value) && value > 0 ? value : Number.POSITIVE_INFINITY;
  };

  const mergeChannelRows = (existingRows, channel, replacementRows, pendingRows = []) => {
    const rows = [
      ...existingRows.filter((row) => row.dataset.chatChannel !== channel),
      ...replacementRows,
      ...pendingRows,
    ];
    return rows
      .map((row, index) => ({
        row,
        index,
        messageId: row.dataset.chatMessageId || "",
        timestamp: chatTimestamp(row),
      }))
      .sort((left, right) => left.timestamp - right.timestamp
        || left.messageId.localeCompare(right.messageId, undefined, { numeric: true })
        || left.index - right.index)
      .map(({ row }) => row);
  };

  const applyChannelVisibility = (rows, visibleChannels) => {
    rows.forEach((message) => {
      message.hidden = !visibleChannels.has(message.dataset.chatChannel);
    });
  };

  const channelBadge = (channel, ownerDocument) => {
    const badge = ownerDocument.createElement("span");
    badge.className = "chat-channel-badge";
    badge.textContent = `[${CHANNEL_LABELS[channel] || channel}] `;
    return badge;
  };

  const decorateMessage = (message, ownerDocument) => {
    if (message.querySelector(".chat-channel-badge")) return;
    const badge = channelBadge(message.dataset.chatChannel, ownerDocument);
    const timestamp = message.querySelector(".chat-timestamp");
    if (timestamp) timestamp.after(badge);
    else message.prepend(badge);
  };

  if (typeof module !== "undefined" && module.exports) {
    module.exports = {
      applyChannelVisibility,
      chatTimestamp,
      decorateMessage,
      mergeChannelRows,
    };
  }
  if (typeof document === "undefined") return;

  document.querySelectorAll(".settlement-chat").forEach((panel) => {
    const messages = panel.querySelector(".settlement-chat-messages");
    const filters = [...panel.querySelectorAll("[data-chat-filter]")];
    if (!messages || !filters.length) return;

    const applyFilters = () => {
      const visibleChannels = new Set(filters
        .filter((filter) => filter.checked)
        .map((filter) => filter.dataset.chatFilter));
      const rows = [...messages.querySelectorAll("[data-chat-channel]")];
      rows.forEach((message) => decorateMessage(message, document));
      applyChannelVisibility(rows, visibleChannels);
    };
    filters.forEach((filter) => filter.addEventListener("change", applyFilters));
    new MutationObserver(applyFilters).observe(messages, { childList: true, subtree: true });
    applyFilters();
  });

  const incomingHost = document.createElement("div");
  incomingHost.className = "local-chat-incoming";
  document.querySelector("main.center-content")?.append(incomingHost);
  const refreshIncoming = async () => {
    const response = await window.strategicBackgroundFetch("local-chat-incoming", "/api/local-chat/incoming", {
      headers: { Accept: "application/json" },
    });
    if (!response.ok) return;
    const players = await response.json();
    incomingHost.replaceChildren(...players.map((player) => {
      const link = document.createElement("a");
      link.className = "local-chat-incoming-portrait";
      const match = location.pathname.match(/^\/locations\/(settlement|quest)\/[^/]+/);
      link.href = `${match?.[0] || ""}/players/${player.id}`;
      link.title = `Talk to ${player.name}`;
      link.textContent = player.name.charAt(0) || "?";
      return link;
    }));
  };
  window.queueStrategicInitialLoad(refreshIncoming);
  document.addEventListener("strategic-live-update", refreshIncoming);

  const chat = document.querySelector(".settlement-chat[data-local-chat-kind][data-local-chat-subject]");
  if (!chat) return;
  const kind = chat.dataset.localChatKind;
  const subject = chat.dataset.localChatSubject;
  const messages = chat.querySelector(".settlement-chat-messages");
  const input = chat.querySelector(".settlement-chat-composer input");
  const send = chat.querySelector(".settlement-chat-composer button");
  const endpoint = `/api/local-chat/${encodeURIComponent(kind)}/${encodeURIComponent(subject)}`;
  let lastSignature = "";

  const refresh = async () => {
    const response = await window.strategicBackgroundFetch(`local-chat:${endpoint}`, endpoint, {
      headers: { Accept: "application/json" },
    });
    if (!response.ok) return;
    const data = await response.json();
    const signature = data.messages.map((message) => message.id).join(",");
    if (signature === lastSignature) return;
    lastSignature = signature;

    // Quest dialogue is assembled locally because its links carry actions. When
    // the persisted copy of that line arrives over SSE, retain the matching DOM
    // row so reconciliation does not replace the anchor and its click handler
    // with plain text. Unmatched rows are pending writes and stay at the end.
    const existingRows = [...messages.children];
    const interactiveRows = [...new Set(
      [...messages.querySelectorAll(".chat-quest-link")]
        .map((anchor) => anchor.closest(".chat-npc-message, .chat-player-message"))
        .filter((row) => row?.dataset.chatChannel === "local" && row.dataset.localChatBody),
    )];
    const interactiveByMessage = new Map();
    const pendingInteractiveRows = [];
    for (const row of interactiveRows) {
      let match = -1;
      for (let index = data.messages.length - 1; index >= 0; index -= 1) {
        const message = data.messages[index];
        if (!interactiveByMessage.has(index)
          && row.dataset.localChatBody === message.body
          && row.dataset.localChatSpeaker === message.sender_name) {
          match = index;
          break;
        }
      }
      if (match >= 0) interactiveByMessage.set(match, row);
      else pendingInteractiveRows.push(row);
    }
    const localRows = data.messages.map((message, index) => {
      const interactiveRow = interactiveByMessage.get(index);
      if (interactiveRow) {
        const row = interactiveRow;
        row.dataset.chatChannel = "local";
        row.dataset.chatMessageId = String(message.id);
        row.dataset.chatCreatedMicros = String(message.created_micros);
        const time = row.querySelector(".chat-timestamp");
        if (time) {
          time.textContent = `[${new Date(message.created_micros / 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}] `;
        }
        decorateMessage(row, document);
        return row;
      }
      const row = document.createElement("div");
      row.className = message.sender_id === 0 ? "chat-npc-message" : "chat-player-message";
      row.dataset.chatChannel = "local";
      row.dataset.chatMessageId = String(message.id);
      row.dataset.chatCreatedMicros = String(message.created_micros);
      const time = document.createElement("span");
      time.className = "chat-timestamp";
      time.textContent = `[${new Date(message.created_micros / 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}] `;
      const badge = channelBadge("local", document);
      const name = document.createElement("strong");
      name.textContent = `${message.sender_name}: `;
      row.append(time, badge, name, document.createTextNode(message.body));
      return row;
    });
    messages.replaceChildren(...mergeChannelRows(existingRows, "local", localRows, pendingInteractiveRows));
    messages.scrollTop = messages.scrollHeight;
  };
  const submit = async () => {
    const body = input.value.trim();
    if (!body) return;
    const form = new URLSearchParams({ body });
    const response = await window.strategicFetch(endpoint, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: form });
    if (response.ok) { input.value = ""; await refresh(); }
  };
  send?.addEventListener("click", submit);
  input?.addEventListener("keydown", (event) => { if (event.key === "Enter") { event.preventDefault(); submit(); } });
  window.queueStrategicInitialLoad(refresh).finally(() => {
    chat.dataset.localChatReady = "true";
    chat.dispatchEvent(new Event("local-chat-ready"));
  });
  document.addEventListener("strategic-live-update", refresh);

})();
