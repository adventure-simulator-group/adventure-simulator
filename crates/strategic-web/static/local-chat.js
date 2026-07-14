(() => {
  const incomingHost = document.createElement("div");
  incomingHost.className = "local-chat-incoming";
  document.querySelector("main.center-content")?.append(incomingHost);
  const refreshIncoming = async () => {
    const response = await fetch("/api/local-chat/incoming", { headers: { Accept: "application/json" } });
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
  refreshIncoming();
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
    const response = await fetch(endpoint, { headers: { Accept: "application/json" } });
    if (!response.ok) return;
    const data = await response.json();
    const signature = data.messages.map((message) => message.id).join(",");
    if (signature === lastSignature) return;
    lastSignature = signature;
    messages.replaceChildren();
    for (const message of data.messages) {
      const row = document.createElement("div");
      row.className = message.sender_id === 0 ? "chat-npc-message" : "chat-player-message";
      const time = document.createElement("span");
      time.className = "chat-timestamp";
      time.textContent = `[${new Date(message.created_micros / 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}] `;
      const name = document.createElement("strong");
      name.textContent = `${message.sender_name}: `;
      row.append(time, name, document.createTextNode(message.body));
      messages.append(row);
    }
    messages.scrollTop = messages.scrollHeight;
  };
  const submit = async () => {
    const body = input.value.trim();
    if (!body) return;
    const form = new URLSearchParams({ body });
    const response = await fetch(endpoint, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" }, body: form });
    if (response.ok) { input.value = ""; await refresh(); }
  };
  send?.addEventListener("click", submit);
  input?.addEventListener("keydown", (event) => { if (event.key === "Enter") { event.preventDefault(); submit(); } });
  refresh().finally(() => {
    chat.dataset.localChatReady = "true";
    chat.dispatchEvent(new Event("local-chat-ready"));
  });
  document.addEventListener("strategic-live-update", refresh);

})();
