(function () {
  const CONFIG = {
    workspaceManagerUrl:
      window.GITEA_CODE_SPACE_CONFIG?.workspaceManagerUrl || "",
  };


  function getRepoContext(panel) {
    const parts = location.pathname.split("/").filter(Boolean);
    const repo = parts.length >= 2 ? `${parts[0]}/${parts[1]}` : location.pathname;

    const cloneUrl =
      panel.querySelector(".repo-clone-https")?.dataset.link ||
      panel.querySelector(".js-clone-url")?.value ||
      "";

    const branch =
      document.querySelector(".branch-dropdown-button")?.textContent?.trim() ||
      document.querySelector(".repo-branch-button")?.textContent?.trim() ||
      "master";

    return { repo, branch, cloneUrl };
  }

  function wait(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  function withAuthReturnTo(loginUrl) {
    const url = new URL(loginUrl);
    url.searchParams.set("return_to", window.location.href);
    url.searchParams.set("gitea_base_url", window.location.origin);
    url.searchParams.set("popup", "true");
    return url.toString();
  }

  async function waitForAuth() {
    for (let attempt = 0; attempt < 120; attempt += 1) {
      await wait(1000);

      try {
        const res = await fetch(new URL("/api/v1/me", CONFIG.workspaceManagerUrl), {
          credentials: "include",
        });

        if (res.ok) return true;
      } catch (error) {
        console.debug("Auth check failed:", error);
      }
    }

    return false;
  }

  async function openAuthWindow(loginUrl) {
    loginUrl = withAuthReturnTo(loginUrl);
    const authWindow = window.open(
      loginUrl,
      "gitea-code-space-auth",
      "popup=yes,width=980,height=720"
    );
    if (authWindow) {
      authWindow.location.href = loginUrl;
    }

    if (!authWindow) {
      window.location.href = loginUrl;
      return false;
    }

    return waitForAuth();
  }

  async function workspaceFetch(path, options = {}) {
    const url = new URL(path, CONFIG.workspaceManagerUrl);
    const headers = new Headers(options.headers || {});

    if (options.body && !headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }

    const res = await fetch(url, {
      ...options,
      headers,
      credentials: "include",
    });

    return res;
  }

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  function containerUrl(space) {
    return space.container_ip ? `http://${space.container_ip}:8080/` : "";
  }

  function renderActionMenu(space) {
    const mappedUrl = space.url ? escapeHtml(space.url) : "";
    const directUrl = containerUrl(space);
    const escapedDirectUrl = directUrl ? escapeHtml(directUrl) : "";
    const workspaceId = escapeHtml(space.workspace_id);

    return `
      <div class="gcs-action-menu">
        <button class="ui small primary button gcs-action-menu-button" type="button">
          Actions ▾
        </button>
        <div class="gcs-action-menu-list">
          ${mappedUrl ? `<a href="${mappedUrl}" target="_blank" rel="noopener">Port mapping</a>` : ""}
          ${escapedDirectUrl ? `<a href="${escapedDirectUrl}" target="_blank" rel="noopener">Container IP</a>` : ""}
          <button class="js-gcs-delete" type="button" data-workspace-id="${workspaceId}">Delete</button>
        </div>
      </div>
    `;
  }

  function renderSpacesPanel(panel, state, data) {
    const spacePanel = panel.querySelector('[data-gcs-panel="spaces"]');
    if (!spacePanel) return;

    const workspaces = data?.workspaces || [];
    const isLoading = state === "loading";
    const isError = state === "error";
    const isAuthRequired = state === "auth";

    const listHtml = workspaces.length
      ? workspaces
          .map((space) => {
            const password = space.code_server_password ? escapeHtml(space.code_server_password) : "";
            const workspaceId = escapeHtml(space.workspace_id);
            const status = escapeHtml(space.status);
            const containerIp = space.container_ip ? escapeHtml(space.container_ip) : "";
            return `
              <div class="gcs-space-item">
                <div class="gcs-space-item-main">
                  <div class="gcs-space-name">${workspaceId}</div>
                  <div class="gcs-space-meta">
                    ${status}
                  </div>
                  ${containerIp ? `<div class="gcs-space-meta">Container IP: <code>${containerIp}</code></div>` : ""}
                  ${password ? `<div class="gcs-space-meta">Password: <code>${password}</code> <button class="ui mini button js-gcs-copy-password" type="button" data-password="${password}">Copy</button></div>` : ""}
                </div>

                <div class="gcs-space-item-actions">
                  ${renderActionMenu(space)}
                </div>
              </div>
            `;
          })
          .join("")
      : `
        <div class="gcs-empty">
          暂无 Code Space
        </div>
      `;

    spacePanel.innerHTML = `
      <div class="gcs-space-panel">
        <div class="gcs-space-header">
          <div class="gcs-space-title">Code Spaces</div>

          <button class="ui primary small button js-gcs-create" ${isLoading ? "disabled" : ""}>
            New
          </button>
        </div>

        <div class="divider"></div>

        ${
          isLoading
            ? `<div class="gcs-empty">正在加载...</div>`
            : isAuthRequired
              ? `<div class="gcs-empty"><button class="ui primary small button js-gcs-authorize" type="button">Authorize</button></div>`
              : isError
                ? `<div class="gcs-empty">加载失败</div>`
                : `<div class="gcs-space-list">${listHtml}</div>`
        }
      </div>
    `;
  }

  async function loadWorkspace(panel) {
    if (!CONFIG.workspaceManagerUrl) {
      renderSpacesPanel(panel, "ready", { workspaces: [] });
      return;
    }

    renderSpacesPanel(panel, "loading");

    try {
      const ctx = getRepoContext(panel);
      const url = new URL("/api/v1/workspaces", CONFIG.workspaceManagerUrl);

      url.searchParams.set("repo", ctx.repo);
      url.searchParams.set("branch", ctx.branch);

      const res = await workspaceFetch(url);
      if (!res) return;
      if (res.status === 401) {
        const data = await res.json().catch(() => ({}));
        if (data.login_url) panel.dataset.gcsLoginUrl = data.login_url;
        renderSpacesPanel(panel, "auth");
        return;
      }
      if (!res.ok) throw new Error("HTTP " + res.status);

      const data = await res.json();
      renderSpacesPanel(panel, "ready", data);
    } catch (error) {
      console.error("Load workspaces failed:", error);
      renderSpacesPanel(panel, "error");
    }
  }

  function switchTab(panel, name) {
    panel.querySelectorAll("[data-gcs-tab]").forEach((tab) => {
      tab.classList.toggle("active", tab.dataset.gcsTab === name);
    });

    panel.querySelectorAll("[data-gcs-panel]").forEach((content) => {
      content.hidden = content.dataset.gcsPanel !== name;
    });

    if (name === "spaces" && panel.dataset.gcsSpacesLoaded !== "1") {
      panel.dataset.gcsSpacesLoaded = "1";
      loadWorkspace(panel);
    }
  }

  function enhanceClonePanel(panel) {
    if (panel.dataset.gcsEnhanced === "1") return;
    panel.dataset.gcsEnhanced = "1";

    const originalNodes = Array.from(panel.childNodes);

    const wrapper = document.createElement("div");
    wrapper.className = "gcs-wrapper";

    const tabs = document.createElement("div");
    tabs.className = "gcs-tabs";
    tabs.innerHTML = `
      <button class="gcs-tab active" type="button" data-gcs-tab="local">Local</button>
      <button class="gcs-tab" type="button" data-gcs-tab="spaces">Code Spaces</button>
    `;

    const localPanel = document.createElement("div");
    localPanel.dataset.gcsPanel = "local";

    const spacesPanel = document.createElement("div");
    spacesPanel.dataset.gcsPanel = "spaces";
    spacesPanel.hidden = true;

    originalNodes.forEach((node) => localPanel.appendChild(node));

    wrapper.appendChild(tabs);
    wrapper.appendChild(localPanel);
    wrapper.appendChild(spacesPanel);
    panel.appendChild(wrapper);

    renderSpacesPanel(panel, "idle");
  }

  document.addEventListener("click", async function (event) {
    const tab = event.target.closest("[data-gcs-tab]");
    if (tab) {
      const panel = tab.closest(".clone-panel-popup");
      if (panel) switchTab(panel, tab.dataset.gcsTab);
      return;
    }

    const copyPasswordButton = event.target.closest(".js-gcs-copy-password");
    if (copyPasswordButton) {
      const password = copyPasswordButton.dataset.password || "";
      if (password && navigator.clipboard) {
        await navigator.clipboard.writeText(password);
      }
      return;
    }

    const authorizeButton = event.target.closest(".js-gcs-authorize");
    if (authorizeButton) {
      const panel = authorizeButton.closest(".clone-panel-popup");
      const loginUrl = panel?.dataset.gcsLoginUrl;
      if (!panel || !loginUrl) return;

      const authenticated = await openAuthWindow(loginUrl);
      if (authenticated) {
        const pendingAction = panel.dataset.gcsPendingAction;
        delete panel.dataset.gcsPendingAction;
        if (pendingAction === "create") {
          await createWorkspace(panel);
        } else {
          await loadWorkspace(panel);
        }
      }
      return;
    }

    const createButton = event.target.closest(".js-gcs-create");
    if (createButton) {
      const panel = createButton.closest(".clone-panel-popup");
      if (!panel) return;

      if (!CONFIG.workspaceManagerUrl) {
        renderSpacesPanel(panel, "loading");

        setTimeout(() => {
          renderSpacesPanel(panel, "ready", {
            workspaces: [
              {
                workspace_id: "placeholder",
                status: "running",
                url: "#",
              },
            ],
          });
        }, 800);

        return;
      }

      await createWorkspace(panel);
      return;
    }

    const deleteButton = event.target.closest(".js-gcs-delete");
    if (deleteButton) {
      const panel = deleteButton.closest(".clone-panel-popup");
      const workspaceId = deleteButton.dataset.workspaceId;

      if (!panel || !workspaceId) return;

      await deleteWorkspace(panel, workspaceId);
      return;
    }
  });

  async function createWorkspace(panel) {
    renderSpacesPanel(panel, "loading");

    try {
      const ctx = getRepoContext(panel);

      const res = await workspaceFetch("/api/v1/workspaces", {
        method: "POST",
        body: JSON.stringify({
          repo: ctx.repo,
          branch: ctx.branch,
          clone_url: ctx.cloneUrl,
        }),
      });

      if (!res) return;
      if (res.status === 401) {
        const data = await res.json().catch(() => ({}));
        if (data.login_url) panel.dataset.gcsLoginUrl = data.login_url;
        panel.dataset.gcsPendingAction = "create";
        renderSpacesPanel(panel, "auth");
        return;
      }
      if (!res.ok) throw new Error("HTTP " + res.status);

      await loadWorkspace(panel);
    } catch (error) {
      console.error("Create workspace failed:", error);
      renderSpacesPanel(panel, "error");
    }
  }

  async function deleteWorkspace(panel, workspaceId) {
    const confirmed = window.confirm(`Delete code space ${workspaceId}?`);
    if (!confirmed) return;

    renderSpacesPanel(panel, "loading");

    try {
      const res = await workspaceFetch(`/api/v1/workspaces/${workspaceId}`, {
        method: "DELETE",
      });

      if (!res) return;

      if (!res.ok) throw new Error(`HTTP ${res.status}`);

      await loadWorkspace(panel);
    } catch (error) {
      console.error("Delete workspace failed:", error);
      renderSpacesPanel(panel, "error");
    }
  }

  const observer = new MutationObserver(function () {
    document.querySelectorAll(".clone-panel-popup").forEach(enhanceClonePanel);
  });

  observer.observe(document.body, {
    childList: true,
    subtree: true,
  });

  document.querySelectorAll(".clone-panel-popup").forEach(enhanceClonePanel);
})();
