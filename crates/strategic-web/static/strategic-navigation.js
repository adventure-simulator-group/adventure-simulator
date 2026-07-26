(() => {
  const HEADER = "X-Strategic-Navigation";
  let generation = 0;
  let pending;
  let announced;

  const page = () => document.querySelector("#strategic-page");
  const boundaryUrl = (url) => url.origin !== location.origin ||
    url.pathname.startsWith("/characters") || url.pathname === "/map/data-license" ||
    url.pathname.startsWith("/missions/") || url.pathname.startsWith("/tactical/");
  const focusKey = (element) => {
    if (!element || element === document.body) return null;
    if (element.id) return { kind: "id", value: element.id };
    const service = element.closest?.("[data-service-id]");
    if (service) return { kind: "service", value: service.dataset.serviceId || "" };
    const equipment = element.closest?.("[data-equipment-toggle]");
    if (equipment) return { kind: "equipment", value: equipment.dataset.inventoryItemId || "" };
    const control = element.closest?.("button,input,select,textarea");
    if (control?.name) {
      const formUrl = control.form ? new URL(control.form.action, location.href) : null;
      return {
        kind: "control",
        name: control.name,
        tag: control.tagName,
        form: formUrl && formUrl.origin === location.origin ? formUrl.pathname : "",
      };
    }
    const link = element.closest?.("a[href]");
    if (link) {
      const url = new URL(link.href, location.href);
      if (url.origin === location.origin) return { kind: "href", value: `${url.pathname}${url.search}${url.hash}` };
    }
    return null;
  };
  const restoreFocus = (root, key) => {
    let target = null;
    if (key?.kind === "id") target = document.getElementById(key.value);
    if (key?.kind === "service") {
      target = [...root.querySelectorAll("[data-service-id]")]
        .find((element) => (element.dataset.serviceId || "") === key.value);
    }
    if (key?.kind === "href") {
      target = [...root.querySelectorAll("a[href]")].find((link) => {
        const url = new URL(link.href, location.href);
        return `${url.pathname}${url.search}${url.hash}` === key.value;
      });
    }
    if (key?.kind === "equipment") {
      target = [...root.querySelectorAll("[data-equipment-toggle]")]
        .find((element) => (element.dataset.inventoryItemId || "") === key.value);
    }
    if (key?.kind === "control") {
      target = [...root.querySelectorAll("button,input,select,textarea")].find((element) => {
        const formUrl = element.form ? new URL(element.form.action, location.href) : null;
        return element.name === key.name && element.tagName === key.tag &&
          (formUrl?.pathname || "") === key.form;
      });
    }
    target ||= root.querySelector("main h1, main h2");
    if (target) {
      if (target.matches("h1,h2") && !target.hasAttribute("tabindex")) target.tabIndex = -1;
      target.focus({ preventScroll: true });
    }
  };
  const announce = (message) => {
    if (!announced) {
      announced = document.createElement("div");
      announced.className = "sr-only";
      announced.setAttribute("aria-live", "polite");
      document.body.append(announced);
    }
    announced.textContent = message;
  };
  const saveScroll = () => history.replaceState(
    {
      ...(history.state || {}),
      strategicScroll: [scrollX, scrollY],
      strategicFocus: focusKey(document.activeElement),
    }, "", location.href,
  );
  const hardBoundary = (link, url) =>
    link.hasAttribute("download") || link.dataset.hardNavigation !== undefined ||
    link.target && link.target.toLowerCase() !== "_self" ||
    url.pathname === "/live" || boundaryUrl(url);

  const commitPage = ({ replacement, title, finalUrl, historyMode = "push", restore = null, alreadyUnmounted = false }) => {
    const current = page();
    if (!current) return false;
    if (!alreadyUnmounted) document.dispatchEvent(new CustomEvent("strategic-page-unmounting"));
    current.replaceWith(replacement);
    document.title = title || `${replacement.dataset.pageTitle} - Adventure Simulator`;
    if (historyMode === "push") history.pushState({ strategicScroll: [0, 0] }, "", finalUrl);
    else if (historyMode === "replace") history.replaceState({ strategicScroll: [0, 0] }, "", finalUrl);
    document.dispatchEvent(new CustomEvent("strategic-page-mounted"));
    if (restore) {
      scrollTo(...(restore.strategicScroll || [0, 0]));
      restoreFocus(replacement, restore.strategicFocus);
    } else {
      const fragmentTarget = finalUrl.hash && document.getElementById(decodeURIComponent(finalUrl.hash.slice(1)));
      if (fragmentTarget) fragmentTarget.scrollIntoView();
      else scrollTo(0, 0);
      restoreFocus(replacement, null);
    }
    return true;
  };

  async function navigate(url, { historyMode = "push", restore = null } = {}) {
    const current = page();
    if (!current) return location.assign(url);
    const mine = ++generation;
    pending?.abort();
    pending = new AbortController();
    saveScroll();
    current.setAttribute("aria-busy", "true");
    document.dispatchEvent(new CustomEvent("strategic-soft-navigation-start"));
    document.dispatchEvent(new CustomEvent("strategic-page-unmounting"));
    announce("Loading");
    try {
      const response = await fetch(url, {
        headers: { [HEADER]: "true", Accept: "text/html" },
        credentials: "same-origin",
        signal: pending.signal,
        redirect: "follow",
      });
      if (mine !== generation) return;
      const text = await response.text();
      const parsed = new DOMParser().parseFromString(text, "text/html");
      const replacement = parsed.querySelector("#strategic-page");
      if (!response.ok || !replacement) {
        const safe = current.cloneNode(false);
        safe.innerHTML = `<main class="center-content strategic-notice-main"><section class="strategic-notice" role="alert"><h2>Unable to open this page</h2><p></p><a class="btn btn-primary" href="/">Return</a></section></main>`;
        safe.querySelector("p").textContent = response.status === 404
          ? "That destination is no longer available."
          : "The destination could not be loaded. Your current session is still active.";
        current.replaceWith(safe);
        document.dispatchEvent(new CustomEvent("strategic-page-mounted"));
        announce("Unable to open page");
        return;
      }
      const profile = response.headers.get("X-Strategic-Script-Profile");
      if (response.headers.get("X-Strategic-Response") !== "root" ||
          profile !== "strategic" || replacement.dataset.scriptProfile !== "strategic") {
        location.assign(response.url || url);
        return;
      }
      const finalUrl = new URL(
        response.headers.get("X-Strategic-Canonical-Url") || response.url || url,
        location.href,
      );
      if (boundaryUrl(finalUrl)) {
        location.assign(finalUrl);
        return;
      }
      if (finalUrl.origin !== new URL(response.url || url, location.href).origin) {
        throw new Error("The server did not return the negotiated strategic contract");
      }
      const requestedHash = new URL(response.url || url, location.href).hash || new URL(url, location.href).hash;
      if (!finalUrl.hash && requestedHash) finalUrl.hash = requestedHash;
      commitPage({ replacement, title: parsed.title, finalUrl, historyMode, restore, alreadyUnmounted: true });
      document.dispatchEvent(new CustomEvent("strategic-soft-navigation-complete"));
      announce("Page loaded");
    } catch (error) {
      if (error.name === "AbortError") return;
      location.assign(url);
    }
  }

  document.addEventListener("click", (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey ||
        event.shiftKey || event.altKey) return;
    const link = event.target.closest("a[href]");
    if (!link) return;
    const raw = link.getAttribute("href");
    if (!raw || raw.startsWith("#")) return;
    const url = new URL(link.href, location.href);
    if (hardBoundary(link, url)) return;
    event.preventDefault();
    navigate(url.href);
  });
  addEventListener("popstate", (event) => navigate(location.href, {
    historyMode: "none", restore: event.state || { strategicScroll: [0, 0] },
  }));
  addEventListener("pagehide", () => pending?.abort(), { once: true });
  if (!history.state?.strategicScroll) history.replaceState({ strategicScroll: [scrollX, scrollY] }, "");
  window.strategicNavigate = navigate;
  window.strategicCommitPage = commitPage;
  window.strategicBoundaryUrl = boundaryUrl;
  window.strategicFocusKey = focusKey;
})();
