const preferenceKey = "blueengine.sidebar.collapsed";

export function initSidebar() {
  const toggle = document.getElementById("sidebar-toggle");
  if (!toggle || toggle.dataset.initialized) return;
  toggle.dataset.initialized = "true";

  const compactViewport = window.matchMedia("(max-width: 650px)");
  let preference = null;
  try {
    const saved = window.localStorage.getItem(preferenceKey);
    if (saved === "true" || saved === "false") preference = saved === "true";
  } catch {
    // The rail remains usable when storage is unavailable.
  }

  function apply(collapsed) {
    document.documentElement.dataset.sidebarCollapsed = String(collapsed);
    const label = collapsed ? "Expand sidebar" : "Collapse sidebar";
    toggle.setAttribute("aria-expanded", String(!collapsed));
    toggle.setAttribute("aria-label", label);
    toggle.title = label;
    toggle.querySelector("span").textContent = collapsed ? "›" : "‹";
  }

  apply(preference ?? compactViewport.matches);
  toggle.addEventListener("click", () => {
    preference = document.documentElement.dataset.sidebarCollapsed !== "true";
    apply(preference);
    try {
      window.localStorage.setItem(preferenceKey, String(preference));
    } catch {
      // Keep the current session's choice even if it cannot be saved.
    }
  });
  compactViewport.addEventListener("change", () => {
    if (preference === null) apply(compactViewport.matches);
  });
}
