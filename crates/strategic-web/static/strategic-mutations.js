(() => {
  let generation = 0;
  let pending;
  const SAFE_NOTICE_SELECTOR = "[data-strategic-safe-message]";
  const mutationFormAction = (form, submitter) =>
    submitter?.hasAttribute?.("formaction") ? submitter.formAction : form.action;

  const extractStrategicNoticeMessage = (root) => {
    const message = root?.querySelector?.(SAFE_NOTICE_SELECTOR)?.textContent
      ?.trim()
      .replace(/\s+/g, " ");
    return message && message.length <= 512 ? message : null;
  };

  const safeStrategicErrorMessage = async (response, origin, parseHtml) => {
    const contentType = response.headers?.get?.("Content-Type") || "";
    if (!contentType.toLowerCase().includes("text/html")) return null;
    let responseUrl;
    try {
      responseUrl = new URL(response.url || origin, origin);
    } catch {
      return null;
    }
    if (responseUrl.origin !== origin) return null;
    try {
      const parsed = parseHtml(await response.text());
      return extractStrategicNoticeMessage(parsed);
    } catch (error) {
      if (error?.name === "AbortError") throw error;
      return null;
    }
  };

  if (typeof module !== "undefined") {
    module.exports = {
      extractStrategicNoticeMessage,
      mutationFormAction,
      safeStrategicErrorMessage,
    };
  }
  if (typeof document === "undefined") return;

  const hardBoundary = (form, url) =>
    form.target && form.target.toLowerCase() !== "_self" ||
    form.dataset.hardNavigation !== undefined ||
    url.origin !== location.origin ||
    window.strategicBoundaryUrl?.(url);

  const notice = (message) => {
    let region = document.querySelector("[data-strategic-mutation-status]");
    if (!region) {
      region = document.createElement("div");
      region.dataset.strategicMutationStatus = "true";
      region.className = "strategic-notice";
      region.setAttribute("role", "alert");
      document.querySelector("#strategic-page main")?.prepend(region);
    }
    region.textContent = message;
  };

  const submitMutation = async (url, {
    body,
    originPage = document.querySelector("#strategic-page"),
    errorMessageFromResponse,
  } = {}) => {
    const mine = ++generation;
    const restore = {
      strategicScroll: [scrollX, scrollY],
      strategicFocus: window.strategicFocusKey?.(document.activeElement) || null,
    };
    pending?.abort();
    pending = new AbortController();
    const response = await fetch(url, {
      method: "POST",
      headers: {
        "X-Strategic-Navigation": "true",
        "X-Strategic-Current-Url": `${location.pathname}${location.search}${location.hash}`,
      },
      body,
      credentials: "same-origin",
      redirect: "error",
      signal: pending.signal,
    });
    if (mine !== generation || originPage !== document.querySelector("#strategic-page")) return false;
    const hardTarget = response.headers.get("X-Strategic-Hard-Navigation");
    if (hardTarget) {
      const target = new URL(hardTarget, location.href);
      if (target.origin !== location.origin && !["http:", "https:"].includes(target.protocol)) {
        throw new Error("The server returned an unsafe navigation target.");
      }
      location.assign(target);
      return true;
    }
    if (!response.ok) {
      let message;
      try {
        if (errorMessageFromResponse) {
          message = await errorMessageFromResponse(response);
        } else {
          const safeMessage = await safeStrategicErrorMessage(
            response,
            location.origin,
            (html) => new DOMParser().parseFromString(html, "text/html"),
          );
          message = safeMessage || (response.status === 409
            ? "The world changed before that action completed. Review the page and try again."
            : "The action could not be completed.");
        }
      } catch (error) {
        if (mine !== generation || originPage !== document.querySelector("#strategic-page")) {
          return false;
        }
        throw error;
      }
      if (mine !== generation || originPage !== document.querySelector("#strategic-page")) {
        return false;
      }
      throw new Error(message);
    }
    const text = await response.text();
    if (mine !== generation || originPage !== document.querySelector("#strategic-page")) return false;
    const parsed = new DOMParser().parseFromString(text, "text/html");
    const replacement = parsed.querySelector("#strategic-page");
    const profile = response.headers.get("X-Strategic-Script-Profile");
    const canonical = new URL(
      response.headers.get("X-Strategic-Canonical-Url") || location.href,
      location.href,
    );
    if (window.strategicBoundaryUrl?.(canonical) ||
        profile !== "strategic" || replacement?.dataset.scriptProfile !== "strategic") {
      location.assign(canonical);
      return true;
    }
    if (response.headers.get("X-Strategic-Response") !== "root" || !replacement) {
      throw new Error("The server did not return the negotiated mutation root.");
    }
    window.strategicCommitPage({
      replacement,
      title: `${replacement.dataset.pageTitle} - Fabelgeist`,
      finalUrl: canonical,
      historyMode: response.headers.get("X-Strategic-Redirected") === "true" ? "push" : "replace",
      restore: response.headers.get("X-Strategic-Redirected") === "true" ? null : restore,
    });
    document.dispatchEvent(new CustomEvent("strategic-mutation-complete", {
      detail: { canonicalUrl: canonical.href, status: response.status },
    }));
    return true;
  };

  document.addEventListener("submit", async (event) => {
    const form = event.target.closest?.("#strategic-page form[method='post' i]");
    if (!form || event.defaultPrevented) return;
    const submitter = event.submitter;
    const url = new URL(mutationFormAction(form, submitter), location.href);
    if (hardBoundary(form, url)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (form.dataset.strategicSubmitting) return;
    form.dataset.strategicSubmitting = "true";
    const body = new URLSearchParams(new FormData(form));
    if (submitter?.name) body.append(submitter.name, submitter.value);
    try {
      await submitMutation(url, { body, originPage: form.closest("#strategic-page") });
    } catch (error) {
      if (error.name !== "AbortError") {
        notice(error.message || "The action could not be completed.");
        window.reportStrategicError?.(error, "strategic action");
      }
    } finally {
      delete form.dataset.strategicSubmitting;
    }
  }, true);
  document.addEventListener("strategic-page-unmounting", () => {
    generation += 1;
    pending?.abort();
  });
  window.strategicSubmitMutation = submitMutation;
})();
