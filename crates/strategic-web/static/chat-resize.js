(() => {
  const STORAGE_KEY = "adventuresim.chat-height";
  const MIN_HEIGHT = 128;
  const MIN_STAGE_HEIGHT = 128;
  const CHAT_BOTTOM_GAP = 5;
  const KEYBOARD_STEP = 24;

  const chat = document.querySelector(".settlement-chat");
  const handle = chat?.querySelector(".settlement-chat-resize");
  const container = chat?.closest(".settlement-main");
  if (!chat || !handle || !container) return;

  const maximumHeight = () => Math.max(MIN_HEIGHT, container.clientHeight - MIN_STAGE_HEIGHT - CHAT_BOTTOM_GAP);
  const clampHeight = (height) => Math.min(maximumHeight(), Math.max(MIN_HEIGHT, height));

  const setHeight = (height, persist = true) => {
    const next = Math.round(clampHeight(height));
    chat.style.setProperty("--chat-height", `${next}px`);
    handle.setAttribute("aria-valuenow", String(next));
    handle.setAttribute("aria-valuemax", String(Math.round(maximumHeight())));
    if (persist) localStorage.setItem(STORAGE_KEY, String(next));
  };

  const savedHeight = Number.parseInt(localStorage.getItem(STORAGE_KEY) || "", 10);
  if (Number.isFinite(savedHeight)) setHeight(savedHeight, false);
  else setHeight(chat.getBoundingClientRect().height, false);

  let startY = 0;
  let startHeight = 0;

  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    startY = event.clientY;
    startHeight = chat.getBoundingClientRect().height;
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("is-resizing");
    document.body.classList.add("chat-resizing");
    event.preventDefault();
  });

  handle.addEventListener("pointermove", (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    setHeight(startHeight + startY - event.clientY, false);
  });

  const finishResize = (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    handle.releasePointerCapture(event.pointerId);
    handle.classList.remove("is-resizing");
    document.body.classList.remove("chat-resizing");
    setHeight(chat.getBoundingClientRect().height);
  };

  handle.addEventListener("pointerup", finishResize);
  handle.addEventListener("pointercancel", finishResize);

  handle.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    const direction = event.key === "ArrowUp" ? 1 : -1;
    setHeight(chat.getBoundingClientRect().height + direction * KEYBOARD_STEP);
    event.preventDefault();
  });

  window.addEventListener("resize", () => setHeight(chat.getBoundingClientRect().height, false));
})();
