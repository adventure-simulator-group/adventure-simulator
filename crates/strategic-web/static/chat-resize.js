(() => {
  const STORAGE_KEY = "adventuresim.chat-height";
  const DESKTOP_MIN_HEIGHT = 128;
  const MOBILE_MIN_HEIGHT = 160;
  const MIN_STAGE_HEIGHT = 260;
  const CHAT_BOTTOM_GAP = 5;
  const KEYBOARD_STEP = 24;
  let lifecycle;
  let activeHandle;
  const unmount = () => {
    lifecycle?.abort();
    activeHandle?.classList.remove("is-resizing");
    document.body.classList.remove("chat-resizing");
    activeHandle = null;
  };
  const mount = () => {
    unmount();
    lifecycle = new AbortController();
    const { signal } = lifecycle;
    const chat = document.querySelector("#strategic-page .settlement-chat");
    const handle = chat?.querySelector(".settlement-chat-resize");
    const container = chat?.closest(".settlement-main");
    if (!chat || !handle || !container) return;
    activeHandle = handle;
    const minimum = () => matchMedia("(max-width: 768px)").matches ? MOBILE_MIN_HEIGHT : DESKTOP_MIN_HEIGHT;
    const maximum = () => Math.max(minimum(), container.clientHeight - MIN_STAGE_HEIGHT - CHAT_BOTTOM_GAP);
    const setHeight = (height, persist = true) => {
      const value = Math.round(Math.max(minimum(), Math.min(maximum(), height)));
      chat.style.setProperty("--chat-height", `${value}px`);
      container.style.setProperty("--chat-panel-height", `${value}px`);
      handle.setAttribute("aria-valuemin", String(minimum()));
      handle.setAttribute("aria-valuenow", String(value));
      handle.setAttribute("aria-valuemax", String(Math.round(maximum())));
      if (persist) localStorage.setItem(STORAGE_KEY, String(value));
    };
    setHeight(Number(localStorage.getItem(STORAGE_KEY)) || chat.getBoundingClientRect().height, false);
    let startY = 0;
    let startHeight = 0;
    const finish = (event) => {
      if (!handle.hasPointerCapture?.(event.pointerId)) return;
      handle.releasePointerCapture(event.pointerId);
      handle.classList.remove("is-resizing");
      document.body.classList.remove("chat-resizing");
      setHeight(chat.getBoundingClientRect().height);
    };
    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      startY = event.clientY;
      startHeight = chat.getBoundingClientRect().height;
      handle.setPointerCapture(event.pointerId);
      handle.classList.add("is-resizing");
      document.body.classList.add("chat-resizing");
      event.preventDefault();
    }, { signal });
    handle.addEventListener("pointermove", (event) => {
      if (handle.hasPointerCapture(event.pointerId)) setHeight(startHeight + startY - event.clientY, false);
    }, { signal });
    handle.addEventListener("pointerup", finish, { signal });
    handle.addEventListener("pointercancel", finish, { signal });
    handle.addEventListener("keydown", (event) => {
      if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const current = chat.getBoundingClientRect().height;
      setHeight(event.key === "Home" ? minimum() : event.key === "End" ? maximum() :
        current + (event.key === "ArrowUp" ? KEYBOARD_STEP : -KEYBOARD_STEP));
    }, { signal });
    addEventListener("resize", () => setHeight(chat.getBoundingClientRect().height, false), { signal });
  };
  mount();
  document.addEventListener("strategic-page-mounted", mount);
  document.addEventListener("strategic-page-unmounting", unmount);
})();
