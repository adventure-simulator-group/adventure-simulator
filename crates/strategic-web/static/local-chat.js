(() => {
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

  const pendingLocalRows = (existingRows, pendingInteractiveRows = []) => [
    ...pendingInteractiveRows,
    ...existingRows.filter((row) => row.dataset.chatChannel === "local"
      && row.dataset.privateDialogue === "true"),
  ];

  const applyChannelVisibility = (rows, visibleChannels) => {
    rows.forEach((message) => {
      message.hidden = !visibleChannels.has(message.dataset.chatChannel);
    });
  };

  const createChannelRow = (channel, content, options = {}, ownerDocument = document) => {
    const row = ownerDocument.createElement("div");
    row.className = options.className || (channel === "info" ? "chat-system-message" : "chat-player-message");
    row.dataset.chatChannel = channel;
    if (options.messageId !== undefined) row.dataset.chatMessageId = String(options.messageId);
    if (options.createdMicros !== undefined) row.dataset.chatCreatedMicros = String(options.createdMicros);

    const timestamp = ownerDocument.createElement("span");
    timestamp.className = "chat-timestamp";
    timestamp.textContent = options.timestamp || "[--:--] ";
    row.append(timestamp);
    if (options.speaker) {
      const name = ownerDocument.createElement("strong");
      name.textContent = `${options.speaker}: `;
      row.append(name);
    }
    row.append(typeof content === "string" ? ownerDocument.createTextNode(content) : content);
    return row;
  };

  const appendChannelRow = (panel, channel, content, options = {}) => {
    const messages = panel?.matches?.(".settlement-chat-messages")
      ? panel
      : panel?.querySelector?.(".settlement-chat-messages");
    if (!messages) return null;
    const row = createChannelRow(channel, content, options, messages.ownerDocument || document);
    messages.append(row);
    messages.scrollTop = messages.scrollHeight;
    return row;
  };

  const appendInfo = (panel, content, options = {}) => appendChannelRow(panel, "info", content, options);
  const localChatEndpoint = (node) => {
    const kind = node.dataset.localChatKind || "";
    const subject = node.dataset.localChatSubject || "";
    if (!kind || !subject) return null;
    const locationId = node.dataset.localChatLocation || "";
    if (kind === "npc" && !locationId) return null;
    const endpoint = `/api/local-chat/${encodeURIComponent(kind)}/${encodeURIComponent(subject)}`;
    return kind === "npc"
      ? `${endpoint}?location_id=${encodeURIComponent(locationId)}`
      : endpoint;
  };
  const localChatForm = (node, body) => new URLSearchParams({
    body,
    location_id: node.dataset.localChatKind === "npc"
      ? (node.dataset.localChatLocation || "")
      : "",
  });

  if (typeof module !== "undefined" && module.exports) {
    module.exports = {
      applyChannelVisibility,
      appendChannelRow,
      appendInfo,
      chatTimestamp,
      createChannelRow,
      mergeChannelRows,
      pendingLocalRows,
      localChatEndpoint,
      localChatForm,
    };
  }
  if (typeof document === "undefined") return;

  window.strategicChat = Object.freeze({ appendChannelRow, appendInfo, createChannelRow });

  let lifecycle;
  const mount = () => {
  lifecycle?.abort();
  lifecycle = new AbortController();
  const { signal } = lifecycle;
  document.querySelectorAll(".settlement-chat").forEach((panel) => {
    const messages = panel.querySelector(".settlement-chat-messages");
    const filters = [...panel.querySelectorAll("[data-chat-filter]")];
    if (!messages || !filters.length) return;

    const applyFilters = () => {
      const visibleChannels = new Set(filters
        .filter((filter) => filter.checked)
        .map((filter) => filter.dataset.chatFilter));
      const rows = [...messages.querySelectorAll("[data-chat-channel]")];
      applyChannelVisibility(rows, visibleChannels);
    };
    filters.forEach((filter) => filter.addEventListener("change", applyFilters, { signal }));
    const observer = new MutationObserver(applyFilters);
    observer.observe(messages, { childList: true, subtree: true });
    signal.addEventListener("abort", () => observer.disconnect(), { once: true });
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
      const match = location.pathname.match(/^\/locations\/(settlement|case-site)\/[^/]+/);
      link.href = `${match?.[0] || ""}/players/${player.id}`;
      link.title = `Talk to ${player.name}`;
      link.textContent = player.name.charAt(0) || "?";
      return link;
    }));
  };
  window.queueStrategicInitialLoad(refreshIncoming);
  document.addEventListener("strategic-live-update", refreshIncoming, { signal });

  const chat = document.querySelector(".settlement-chat[data-local-chat-kind][data-local-chat-subject]");
  if (!chat) return;
  const messages = chat.querySelector(".settlement-chat-messages");
  const input = chat.querySelector(".settlement-chat-composer input");
  const send = chat.querySelector(".settlement-chat-composer button");
  let lastSignature = "";

  const refresh = async () => {
    const endpoint = localChatEndpoint(chat);
    if (!endpoint) return;
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
        .filter((row) => row?.dataset.chatChannel === "local"
          && row.dataset.privateDialogue !== "true"
          && row.dataset.localChatBody),
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
      const name = document.createElement("strong");
      name.textContent = `${message.sender_name}: `;
      row.append(time, name, document.createTextNode(message.body));
      return row;
    });
    messages.replaceChildren(...mergeChannelRows(
      existingRows,
      "local",
      localRows,
      pendingLocalRows(existingRows, pendingInteractiveRows),
    ));
    messages.scrollTop = messages.scrollHeight;
  };
  const submit = async () => {
    const body = input.value.trim();
    if (!body) return;
    const form = localChatForm(chat, body);
    const endpoint = localChatEndpoint(chat);
    if (!endpoint) return;
    const response = await window.strategicFetch(endpoint, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: form });
    if (response.ok) { input.value = ""; await refresh(); }
  };
  send?.addEventListener("click", submit, { signal });
  input?.addEventListener("keydown", (event) => { if (event.key === "Enter") { event.preventDefault(); submit(); } }, { signal });
  window.queueStrategicInitialLoad(refresh).finally(() => {
    chat.dataset.localChatReady = "true";
    chat.dispatchEvent(new Event("local-chat-ready"));
  });
  document.addEventListener("strategic-live-update", refresh, { signal });
  chat.addEventListener("local-chat-subject-changed", () => { lastSignature = ""; refresh(); }, { signal });

  };
  mount();
  document.addEventListener("strategic-page-mounted", mount);
  document.addEventListener("strategic-page-unmounting", () => lifecycle?.abort());
})();
