"use strict";

const DESTINATION_ICONS = {
    home: "✦",
    build: "▣",
    automate: "⇄",
    assist: "◎",
    observe: "◌",
};

const state = {
    loading: true,
    connected: false,
    snapshot: null,
    panels: {},
    utilityOpen: false,
    homeDetail: "",
    chatDraft: "",
    channelDraft: "",
    agentGoal: "",
    agentMode: "coordinator",
    gitCommitMessage: "",
    terminalInput: "",
    streaming: {
        conversationId: null,
        content: "",
    },
    pendingConversationId: null,
    optimisticMessages: [],
    reconnectDelay: 1000,
};

let socket = null;
let reconnectTimer = null;
let pingTimer = null;

function esc(value) {
    return String(value || "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

function renderMarkdown(text) {
    let html = esc(text);
    html = html.replace(/```([\s\S]*?)```/g, (_m, code) => `<pre><code>${code.trim()}</code></pre>`);
    html = html.replace(/`([^`\n]+)`/g, "<code>$1</code>");
    html = html.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
    html = html.replace(/\n/g, "<br>");
    return html;
}

function money(value) {
    const number = Number(value || 0);
    if (!number) return "$0.00";
    if (number < 0.01) return "<$0.01";
    return `$${number.toFixed(2)}`;
}

function formatBytes(value) {
    const bytes = Number(value || 0);
    if (!bytes) return "0 B";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function percent(value) {
    return `${Math.round(Number(value || 0) * 100)}%`;
}

function shortTime(value) {
    if (!value) return "Just now";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return esc(value);
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function duration(ms) {
    const total = Number(ms || 0);
    if (total < 1000) return `${total}ms`;
    const seconds = Math.floor(total / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainder = seconds % 60;
    return `${minutes}m ${remainder}s`;
}

function uuid4() {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
        const r = (Math.random() * 16) | 0;
        const v = c === "x" ? r : (r & 0x3) | 0x8;
        return v.toString(16);
    });
}

async function api(path, options) {
    const init = { method: "GET", ...options };
    init.headers = {
        Accept: "application/json",
        ...(options && options.body ? { "Content-Type": "application/json" } : {}),
        ...(options && options.headers ? options.headers : {}),
    };

    const response = await fetch(path, init);
    const text = await response.text();
    const data = text ? JSON.parse(text) : null;
    if (!response.ok) {
        throw new Error((data && data.error) || `Request failed (${response.status})`);
    }
    return data;
}

function showToast(message, tone) {
    let container = document.querySelector(".toast-stack");
    if (!container) {
        container = document.createElement("div");
        container.className = "toast-stack";
        document.body.appendChild(container);
    }

    const toast = document.createElement("div");
    toast.className = `toast ${tone || ""}`.trim();
    toast.textContent = message;
    container.appendChild(toast);

    setTimeout(() => {
        toast.classList.add("fade");
        setTimeout(() => toast.remove(), 220);
    }, 3600);
}

function syncSnapshot(snapshot) {
    state.snapshot = snapshot;
    if (!snapshot.is_streaming) {
        state.streaming = { conversationId: null, content: "" };
    }
    if (
        state.pendingConversationId &&
        snapshot.active_conversation === state.pendingConversationId
    ) {
        state.pendingConversationId = null;
    }
}

async function loadState() {
    const snapshot = await api("/api/state");
    syncSnapshot(snapshot);
}

async function loadPanel(panelId) {
    const response = await api(`/api/panels/${panelId}`);
    state.panels[response.panel] = response.data;
    if (response.panel === "chat" && response.data.conversation_id) {
        state.optimisticMessages = state.optimisticMessages.filter(
            (message) => message.conversationId !== response.data.conversation_id,
        );
    }
    render();
    return response.data;
}

async function ensurePanels(panelIds) {
    const unique = [...new Set(panelIds.filter(Boolean))];
    await Promise.all(unique.map((panelId) => loadPanel(panelId).catch(() => null)));
}

function connectSocket() {
    const protocol = location.protocol === "https:" ? "wss:" : "ws:";
    socket = new WebSocket(`${protocol}//${location.host}/ws`);

    socket.addEventListener("open", () => {
        state.connected = true;
        state.reconnectDelay = 1000;
        startPing();
        render();
    });

    socket.addEventListener("close", () => {
        state.connected = false;
        stopPing();
        render();
        scheduleReconnect();
    });

    socket.addEventListener("message", async (event) => {
        const payload = JSON.parse(event.data);
        await handleSocketEvent(payload);
    });
}

function scheduleReconnect() {
    if (reconnectTimer) return;
    reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connectSocket();
    }, state.reconnectDelay);
    state.reconnectDelay = Math.min(state.reconnectDelay * 2, 30000);
}

function startPing() {
    stopPing();
    pingTimer = window.setInterval(() => {
        if (socket && socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ type: "ping" }));
        }
    }, 25000);
}

function stopPing() {
    if (pingTimer) {
        window.clearInterval(pingTimer);
        pingTimer = null;
    }
}

async function handleSocketEvent(event) {
    switch (event.type) {
        case "state_snapshot":
            syncSnapshot(event);
            ensurePanels(["home", "observe", "chat", event.active_panel]);
            render();
            break;
        case "panel_data":
            state.panels[event.panel] = event.data;
            if (event.panel === "chat" && event.data.conversation_id) {
                state.optimisticMessages = state.optimisticMessages.filter(
                    (message) => message.conversationId !== event.data.conversation_id,
                );
            }
            render();
            break;
        case "stream_chunk":
            state.streaming.conversationId = event.conversation_id;
            state.streaming.content += event.chunk;
            render();
            break;
        case "stream_complete":
            if (state.streaming.conversationId === event.conversation_id) {
                state.streaming = { conversationId: null, content: "" };
            }
            await ensurePanels(["chat", "home", "observe"]);
            break;
        case "agent_status":
            await Promise.all([loadState(), loadPanel("observe")]);
            break;
        case "error":
            showToast(event.message || "Remote request failed.", "error");
            break;
        default:
            break;
    }
}

function registry() {
    return state.snapshot ? state.snapshot.panel_registry : { destinations: [], utility_panels: [] };
}

function destinationPanels(destination) {
    return registry().destinations.find((group) => group.destination === destination)?.panels || [];
}

function utilityPanels() {
    return registry().utility_panels || [];
}

function activePanelMeta() {
    const panels = [
        ...registry().destinations.flatMap((group) => group.panels),
        ...utilityPanels(),
    ];
    return panels.find((panel) => panel.id === state.snapshot?.active_panel) || null;
}

function activePanelData() {
    return state.snapshot ? state.panels[state.snapshot.active_panel] : null;
}

function activeConversationId(chatData) {
    return (
        state.pendingConversationId ||
        chatData?.conversation_id ||
        state.snapshot?.active_conversation ||
        null
    );
}

function collectChatMessages(chatData) {
    const conversationId = activeConversationId(chatData);
    const storedMessages =
        state.pendingConversationId && chatData?.conversation_id !== conversationId
            ? []
            : chatData?.messages || [];
    const optimistic = state.optimisticMessages
        .filter((message) => message.conversationId === conversationId)
        .map((message) => ({
            role: message.role,
            content: message.content,
            timestamp: message.timestamp,
            model: message.model,
            cost: message.cost,
            tokens: message.tokens,
        }));
    return storedMessages.concat(optimistic);
}

function isDesktopLayout() {
    return window.matchMedia("(min-width: 960px)").matches;
}

function titleize(value) {
    return String(value || "")
        .split("_")
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(" ");
}

function isSupportedPanel(panel) {
    return !!panel?.supported;
}

function renderApp() {
    if (state.loading || !state.snapshot) {
        return `
            <div class="loading-shell">
                <div class="loading-mark"></div>
                <p>Connecting to Hive Remote control plane...</p>
            </div>
        `;
    }

    const activeDestination = state.snapshot.active_destination;

    return `
        <div class="shell">
            ${renderTopbar()}
            <div class="shell-body">
                <aside class="destination-rail">${renderDestinationButtons(true)}</aside>
                <main class="workspace-shell">
                    <div class="destination-strip">${renderDestinationButtons(false)}</div>
                    <div class="panel-strip">${renderPanelTabs(activeDestination)}</div>
                    <div class="content-shell">
                        <section class="panel-canvas">${renderPanelCanvas()}</section>
                        ${renderContextRail()}
                    </div>
                </main>
            </div>
            ${renderUtilityDrawer()}
        </div>
    `;
}

function renderTopbar() {
    const workspace = state.snapshot.current_workspace;
    const activeMeta = activePanelMeta();

    return `
        <header class="topbar">
            <div class="brand-block">
                <div class="brand-mark">Hive</div>
                <div class="brand-copy">
                    <div class="eyebrow">Remote Control Plane</div>
                    <div class="headline">${esc(activeMeta?.label || "Remote shell")}</div>
                </div>
            </div>
            <div class="workspace-pill">
                <span class="workspace-label">${esc(workspace.name)}</span>
                <span class="workspace-path">${esc(workspace.path)}</span>
            </div>
            <div class="status-cluster">
                <span class="status-pill ${state.connected ? "online" : "offline"}">
                    ${state.connected ? "Live" : "Reconnecting"}
                </span>
                <span class="status-pill subtle">${esc(state.snapshot.current_model)}</span>
                <span class="status-pill subtle">${state.snapshot.pending_approval_count} approvals</span>
                <button class="icon-button" data-action="toggle-utility" aria-label="Open utility panels">
                    Utilities
                </button>
            </div>
        </header>
    `;
}

function renderDestinationButtons(desktopRail) {
    const destinations = registry().destinations || [];
    return destinations
        .map((group) => {
            const active = group.destination === state.snapshot.active_destination ? "active" : "";
            return `
                <button class="destination-tab ${active}" data-action="switch-destination" data-destination="${group.destination}">
                    <span class="destination-icon">${DESTINATION_ICONS[group.destination] || "•"}</span>
                    <span class="destination-copy">
                        <span class="destination-label">${esc(titleize(group.destination))}</span>
                        ${
                            desktopRail
                                ? `<span class="destination-meta">${group.panels.length} panels</span>`
                                : ""
                        }
                    </span>
                </button>
            `;
        })
        .join("");
}

function renderPanelTabs(destination) {
    const panels = destinationPanels(destination);
    return panels
        .map((panel) => {
            const active = panel.id === state.snapshot.active_panel ? "active" : "";
            const badge = panel.supported ? "" : `<span class="inline-badge">Desktop later</span>`;
            return `
                <button class="panel-tab ${active}" data-action="switch-panel" data-panel="${panel.id}">
                    <span class="panel-tab-copy">
                        <span>${esc(panel.label)}</span>
                        <span class="panel-tab-detail">${esc(panel.description)}</span>
                    </span>
                    ${badge}
                </button>
            `;
        })
        .join("");
}

function renderPanelCanvas() {
    const data = activePanelData();
    if (!data) {
        return `
            <div class="empty-state">
                <div class="empty-mark"></div>
                <h2>Loading panel</h2>
                <p>Hive Remote is hydrating the latest shell data for this surface.</p>
            </div>
        `;
    }

    switch (data.kind) {
        case "home":
            return renderHomePanel(data);
        case "observe":
            return renderObservePanel(data);
        case "chat":
            return renderChatPanel(data);
        case "history":
            return renderHistoryPanel(data);
        case "files":
            return renderFilesPanel(data);
        case "specs":
            return renderSpecsPanel(data);
        case "agents":
            return renderAgentsPanel(data);
        case "git_ops":
            return renderGitOpsPanel(data);
        case "terminal":
            return renderTerminalPanel(data);
        case "workflows":
            return renderWorkflowsPanel(data);
        case "channels":
            return renderChannelsPanel(data);
        case "network":
            return renderNetworkPanel(data);
        case "assistant":
            return renderAssistantPanel(data);
        case "settings":
            return renderSettingsPanel(data);
        case "models":
            return renderModelsPanel(data);
        case "routing":
            return renderRoutingPanel(data);
        case "skills":
            return renderSkillsPanel(data);
        case "launch":
            return renderLaunchPanel(data);
        case "help":
            return renderHelpPanel(data);
        case "handoff":
            return renderHandoffPanel(data);
        default:
            return `
                <div class="empty-state">
                    <h2>Unsupported panel payload</h2>
                    <p>Hive Remote received a panel shape it does not know how to render yet.</p>
                </div>
            `;
    }
}

function renderHomePanel(data) {
    return `
        <div class="panel-stack">
            <section class="hero-card">
                <div class="eyebrow">Home</div>
                <h1>${esc(data.project_name)}</h1>
                <p class="hero-summary">${esc(data.project_summary)}</p>
                <div class="hero-meta">
                    <span>${esc(data.project_root)}</span>
                    <span>${esc(data.current_model)}</span>
                    <span>${data.pending_approval_count} approvals waiting</span>
                </div>
                <div class="hero-composer">
                    <label for="home-detail-input">Mission detail</label>
                    <textarea id="home-detail-input" placeholder="${esc(data.launch_hint)}">${esc(state.homeDetail)}</textarea>
                </div>
                <div class="template-grid">
                    ${data.templates
                        .map(
                            (template) => `
                                <article class="template-card">
                                    <div class="eyebrow">Launch</div>
                                    <h3>${esc(template.title)}</h3>
                                    <p>${esc(template.description)}</p>
                                    <div class="template-outcome">${esc(template.outcome)}</div>
                                    <button class="primary-button" data-action="launch-template" data-template="${template.id}">
                                        Start mission
                                    </button>
                                </article>
                            `,
                        )
                        .join("")}
                </div>
            </section>

            <section class="panel-section">
                <div class="section-header">
                    <h2>What needs attention</h2>
                    <span>${data.pending_approval_count} waiting</span>
                </div>
                <div class="priority-grid">
                    ${data.priorities
                        .map(
                            (card) => `
                                <article class="priority-card tone-${esc(card.tone)}">
                                    <div class="eyebrow">${esc(card.eyebrow)}</div>
                                    <h3>${esc(card.title)}</h3>
                                    <p>${esc(card.detail)}</p>
                                    <button class="ghost-button" data-action="switch-panel" data-panel="${card.action_panel}">
                                        ${esc(card.action_label)}
                                    </button>
                                </article>
                            `,
                        )
                        .join("")}
                </div>
            </section>

            <section class="panel-section">
                <div class="section-header">
                    <h2>Status</h2>
                    <span>${data.launch_ready ? "Ready" : "Needs setup"}</span>
                </div>
                <div class="status-grid">
                    ${data.status_cards
                        .map(
                            (card) => `
                                <article class="status-card tone-${esc(card.tone)}">
                                    <div class="status-value">${esc(card.value)}</div>
                                    <h3>${esc(card.title)}</h3>
                                    <p>${esc(card.detail)}</p>
                                    ${
                                        card.action_label && card.action_panel
                                            ? `<button class="inline-link" data-action="switch-panel" data-panel="${card.action_panel}">${esc(card.action_label)}</button>`
                                            : ""
                                    }
                                </article>
                            `,
                        )
                        .join("")}
                </div>
            </section>

            <section class="panel-section split-grid">
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Next steps</h2>
                        <span>Act from anywhere</span>
                    </div>
                    <div class="stack-list">
                        ${data.next_steps
                            .map(
                                (step) => `
                                    <article class="list-card">
                                        <h3>${esc(step.title)}</h3>
                                        <p>${esc(step.detail)}</p>
                                        <button class="ghost-button" data-action="switch-panel" data-panel="${step.action_panel}">
                                            ${esc(step.action_label)}
                                        </button>
                                    </article>
                                `,
                            )
                            .join("")}
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Workspaces</h2>
                        <span>Switch context remotely</span>
                    </div>
                    <div class="workspace-list">
                        ${data.saved_workspaces
                            .map(
                                (workspace) => `
                                    <button class="workspace-row ${workspace.is_current ? "current" : ""}" data-action="switch-workspace" data-workspace="${esc(workspace.path)}">
                                        <span class="workspace-name">${esc(workspace.name)}</span>
                                        <span class="workspace-path">${esc(workspace.path)}</span>
                                    </button>
                                `,
                            )
                            .join("")}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderObservePanel(data) {
    return `
        <div class="panel-stack">
            <section class="observe-header">
                <div>
                    <div class="eyebrow">Observe</div>
                    <h1>Validation inbox</h1>
                    <p>Approvals, failures, spend, and safety stay available away from the desktop app.</p>
                </div>
                <div class="observe-summary">
                    <div class="summary-chip">${data.approvals.length} approvals</div>
                    <div class="summary-chip">${data.runtime.active_agents} active agents</div>
                    <div class="summary-chip">${money(data.spend.today_cost_usd)} today</div>
                </div>
            </section>
            <section class="observe-tabs">
                ${["inbox", "runtime", "spend", "safety"]
                    .map((view) => `
                        <button class="observe-tab ${data.current_view === view ? "active" : ""}" data-action="switch-observe-view" data-view="${view}">
                            ${esc(titleize(view))}
                        </button>
                    `)
                    .join("")}
            </section>
            <section class="panel-section">
                ${renderObserveView(data)}
            </section>
        </div>
    `;
}

function renderObserveView(data) {
    switch (data.current_view) {
        case "runtime":
            return renderObserveRuntime(data.runtime);
        case "spend":
            return renderObserveSpend(data.spend);
        case "safety":
            return renderObserveSafety(data.safety);
        case "inbox":
        default:
            return renderObserveInbox(data);
    }
}

function renderObserveInbox(data) {
    return `
        <div class="stack-list">
            ${data.approvals.length
                ? `
                    <article class="stack-card">
                        <div class="section-header">
                            <h2>Pending approvals</h2>
                            <span>${data.approvals.length} waiting</span>
                        </div>
                        <div class="approval-list">
                            ${data.approvals.map(renderApprovalCard).join("")}
                        </div>
                    </article>
                `
                : ""}
            <article class="stack-card">
                <div class="section-header">
                    <h2>Inbox</h2>
                    <span>${data.inbox.length} items</span>
                </div>
                <div class="stack-list">
                    ${data.inbox
                        .map(
                            (item) => `
                                <article class="list-card tone-${esc(item.tone)}">
                                    <div class="card-meta">
                                        <span>${esc(item.kind)}</span>
                                        <span>${esc(item.timestamp)}</span>
                                    </div>
                                    <h3>${esc(item.title)}</h3>
                                    <p>${esc(item.detail)}</p>
                                </article>
                            `,
                        )
                        .join("")}
                </div>
            </article>
        </div>
    `;
}

function renderObserveRuntime(runtime) {
    return `
        <div class="stack-list">
            <div class="status-grid">
                <article class="status-card tone-active">
                    <div class="status-value">${esc(runtime.status_label)}</div>
                    <h3>Runtime state</h3>
                    <p>${runtime.active_streams} active streams, queue ${runtime.request_queue_length}</p>
                </article>
                <article class="status-card tone-calm">
                    <div class="status-value">${runtime.active_agents}</div>
                    <h3>Active agents</h3>
                    <p>${runtime.current_run_id ? `Current run ${esc(runtime.current_run_id)}` : "No active remote run"}</p>
                </article>
                <article class="status-card tone-calm">
                    <div class="status-value">${runtime.online_providers}/${runtime.total_providers}</div>
                    <h3>Providers</h3>
                    <p>Available model backends for remote execution</p>
                </article>
            </div>
            <article class="stack-card">
                <div class="section-header">
                    <h2>Agents</h2>
                    <span>${runtime.agents.length} roles</span>
                </div>
                <div class="table-like">
                    ${runtime.agents
                        .map(
                            (agent) => `
                                <div class="table-row">
                                    <div>
                                        <strong>${esc(agent.role)}</strong>
                                        <span>${esc(agent.phase)}</span>
                                    </div>
                                    <div>
                                        <strong>${esc(agent.status)}</strong>
                                        <span>${esc(agent.model)}</span>
                                    </div>
                                    <div>${esc(agent.started_at)}</div>
                                </div>
                            `,
                        )
                        .join("")}
                </div>
            </article>
            <article class="stack-card">
                <div class="section-header">
                    <h2>Recent runs</h2>
                    <span>${runtime.recent_runs.length} tracked</span>
                </div>
                <div class="table-like">
                    ${runtime.recent_runs
                        .map(
                            (run) => `
                                <div class="table-row">
                                    <div>
                                        <strong>${esc(run.id)}</strong>
                                        <span>${esc(run.summary)}</span>
                                    </div>
                                    <div>
                                        <strong>${esc(run.status)}</strong>
                                        <span>${money(run.cost_usd)}</span>
                                    </div>
                                    <div>${esc(run.started_at)}</div>
                                </div>
                            `,
                        )
                        .join("")}
                </div>
            </article>
        </div>
    `;
}

function renderObserveSpend(spend) {
    return `
        <div class="stack-list">
            <div class="status-grid">
                <article class="status-card tone-calm">
                    <div class="status-value">${money(spend.total_cost_usd)}</div>
                    <h3>Total cost</h3>
                    <p>Remote and recent desktop-aligned activity</p>
                </article>
                <article class="status-card tone-active">
                    <div class="status-value">${money(spend.today_cost_usd)}</div>
                    <h3>Today</h3>
                    <p>Current-day burn for active workflows</p>
                </article>
                <article class="status-card tone-calm">
                    <div class="status-value">${percent(spend.quality_score)}</div>
                    <h3>Quality score</h3>
                    <p>${esc(spend.quality_trend)}</p>
                </article>
                <article class="status-card tone-calm">
                    <div class="status-value">${percent(spend.cost_efficiency)}</div>
                    <h3>Efficiency</h3>
                    <p>${spend.best_model ? `Best model ${esc(spend.best_model)}` : "Awaiting more runs"}</p>
                </article>
            </div>
            <article class="stack-card">
                <div class="section-header">
                    <h2>Model quality spread</h2>
                    <span>${spend.worst_model ? `Watch ${esc(spend.worst_model)}` : "Stable"}</span>
                </div>
                <div class="metric-pair">
                    <div>
                        <label>Best model</label>
                        <strong>${esc(spend.best_model || "N/A")}</strong>
                    </div>
                    <div>
                        <label>Weakest model</label>
                        <strong>${esc(spend.worst_model || "N/A")}</strong>
                    </div>
                </div>
                <div class="tag-list">
                    ${spend.weak_areas.map((area) => `<span class="tag">${esc(area)}</span>`).join("")}
                </div>
            </article>
        </div>
    `;
}

function renderObserveSafety(safety) {
    return `
        <div class="stack-list">
            <div class="status-grid">
                <article class="status-card tone-${safety.shield_enabled ? "active" : "warning"}">
                    <div class="status-value">${safety.shield_enabled ? "On" : "Off"}</div>
                    <h3>Shield posture</h3>
                    <p>${safety.shield_enabled ? "Remote protection rules are enforced." : "Safety policies are relaxed."}</p>
                </article>
                <article class="status-card tone-warning">
                    <div class="status-value">${safety.pii_detections}</div>
                    <h3>PII detections</h3>
                    <p>Recent privacy events caught by Hive safeguards</p>
                </article>
                <article class="status-card tone-warning">
                    <div class="status-value">${safety.secrets_blocked}</div>
                    <h3>Secrets blocked</h3>
                    <p>Potential secret leaks blocked before execution</p>
                </article>
                <article class="status-card tone-warning">
                    <div class="status-value">${safety.threats_caught}</div>
                    <h3>Threats caught</h3>
                    <p>Prompt or workflow threats stopped in time</p>
                </article>
            </div>
            <article class="stack-card">
                <div class="section-header">
                    <h2>Recent events</h2>
                    <span>${safety.recent_events.length} logged</span>
                </div>
                <div class="stack-list">
                    ${safety.recent_events
                        .map(
                            (event) => `
                                <article class="list-card tone-${esc(event.severity)}">
                                    <div class="card-meta">
                                        <span>${esc(event.event_type)}</span>
                                        <span>${esc(event.timestamp)}</span>
                                    </div>
                                    <p>${esc(event.detail)}</p>
                                </article>
                            `,
                        )
                        .join("")}
                </div>
            </article>
        </div>
    `;
}

function renderChatPanel(data) {
    const messages = collectChatMessages(data);
    const conversationId = activeConversationId(data);
    const isStreaming = state.streaming.conversationId === conversationId && !!state.streaming.content;

    return `
        <div class="chat-shell">
            <aside class="chat-sidebar">
                <div class="section-header">
                    <h2>Conversations</h2>
                    <button class="inline-link" data-action="new-conversation">New</button>
                </div>
                <div class="conversation-list">
                    ${data.conversations
                        .map(
                            (conversation) => `
                                <button class="conversation-card ${conversation.id === conversationId ? "active" : ""}" data-action="resume-conversation" data-conversation="${conversation.id}">
                                    <strong>${esc(conversation.title)}</strong>
                                    <span>${esc(conversation.preview)}</span>
                                    <div class="card-meta">
                                        <span>${esc(conversation.model)}</span>
                                        <span>${money(conversation.total_cost)}</span>
                                        <span>${shortTime(conversation.updated_at)}</span>
                                    </div>
                                </button>
                            `,
                        )
                        .join("")}
                </div>
            </aside>
            <div class="chat-main">
                <div class="chat-toolbar">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Remote chat</h1>
                    </div>
                    <div class="toolbar-actions">
                        <select id="chat-model-select">
                            ${data.available_models
                                .map(
                                    (model) => `
                                        <option value="${esc(model.id)}" ${model.id === data.current_model ? "selected" : ""}>
                                            ${esc(model.label)}
                                        </option>
                                    `,
                                )
                                .join("")}
                        </select>
                    </div>
                </div>
                ${
                    data.pending_approvals.length
                        ? `
                            <section class="approval-strip">
                                ${data.pending_approvals.map(renderApprovalCard).join("")}
                            </section>
                        `
                        : ""
                }
                <section class="chat-transcript" id="chat-transcript">
                    ${
                        messages.length || isStreaming
                            ? `
                                ${messages.map(renderMessageCard).join("")}
                                ${
                                    isStreaming
                                        ? `
                                            <article class="message-card assistant">
                                                <div class="message-body">${renderMarkdown(state.streaming.content)}<span class="streaming-dot"></span></div>
                                                <div class="message-meta">
                                                    <span>${esc(data.current_model)}</span>
                                                    <span>Streaming</span>
                                                </div>
                                            </article>
                                        `
                                        : ""
                                }
                            `
                            : `
                                <div class="empty-state compact">
                                    <div class="empty-mark"></div>
                                    <h2>Start a remote conversation</h2>
                                    <p>Use Chat for live edits, approvals, and remote follow-through without reopening the desktop shell.</p>
                                </div>
                            `
                    }
                </section>
                <section class="composer">
                    <textarea id="chat-input" placeholder="Tell Hive what to build, inspect, or approve remotely...">${esc(state.chatDraft)}</textarea>
                    <div class="composer-actions">
                        <div class="composer-meta">
                            <span>${conversationId ? `Conversation ${esc(conversationId)}` : "New conversation"}</span>
                            <span>${money(data.total_cost)} total</span>
                        </div>
                        <button class="primary-button" data-action="send-chat">
                            ${isStreaming ? "Streaming..." : "Send"}
                        </button>
                    </div>
                </section>
            </div>
        </div>
    `;
}

function renderHistoryPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>History</h1>
                    </div>
                    <span>${data.conversations.length} conversations</span>
                </div>
                <div class="conversation-list">
                    ${data.conversations.length
                        ? data.conversations
                              .map(
                                  (conversation) => `
                                    <button class="conversation-card ${conversation.id === data.active_conversation ? "active" : ""}" data-action="resume-conversation" data-conversation="${conversation.id}">
                                        <strong>${esc(conversation.title)}</strong>
                                        <span>${esc(conversation.preview)}</span>
                                        <div class="card-meta">
                                            <span>${esc(conversation.model)}</span>
                                            <span>${conversation.message_count} messages</span>
                                            <span>${money(conversation.total_cost)}</span>
                                            <span>${shortTime(conversation.updated_at)}</span>
                                        </div>
                                    </button>
                                `,
                              )
                              .join("")
                        : `
                            <div class="empty-state compact">
                                <div class="empty-mark"></div>
                                <h2>No conversations yet</h2>
                                <p>Start a remote chat or launch a Home mission to build history here.</p>
                            </div>
                        `}
                </div>
            </section>
        </div>
    `;
}

function renderFilesPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Files</h1>
                    </div>
                    <span>${esc(data.current_path)}</span>
                </div>
                <div class="breadcrumb-row">
                    ${data.breadcrumbs
                        .map(
                            (crumb) => `
                                <button class="crumb-button" data-action="file-breadcrumb" data-path="${esc(crumb.path)}">
                                    ${esc(crumb.label)}
                                </button>
                            `,
                        )
                        .join("")}
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Workspace</h2>
                            <span>${data.entries.length} entries</span>
                        </div>
                        <div class="file-list">
                            ${data.entries
                                .map(
                                    (entry) => `
                                        <button class="file-row" data-action="${entry.is_dir ? "file-breadcrumb" : "open-file"}" data-path="${esc(entry.path)}">
                                            <div>
                                                <strong>${entry.is_dir ? "Folder" : "File"}</strong>
                                                <span>${esc(entry.name)}</span>
                                            </div>
                                            <div>
                                                <strong>${entry.is_dir ? "Directory" : formatBytes(entry.size)}</strong>
                                                <span>${entry.modified ? shortTime(entry.modified) : "n/a"}</span>
                                            </div>
                                        </button>
                                    `,
                                )
                                .join("")}
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Preview</h2>
                            <span>${data.preview ? esc(data.preview.path) : "Choose a file"}</span>
                        </div>
                        ${
                            data.preview
                                ? `
                                    <div class="card-meta">
                                        <span>${formatBytes(data.preview.size)}</span>
                                        <span>${data.preview.modified ? shortTime(data.preview.modified) : "n/a"}</span>
                                    </div>
                                    <pre class="code-block">${esc(data.preview.content)}</pre>
                                `
                                : data.preview_error
                                  ? `<div class="empty-state compact"><h2>Preview unavailable</h2><p>${esc(data.preview_error)}</p></div>`
                                  : `<div class="empty-state compact"><h2>No file selected</h2><p>Select a file to preview it remotely.</p></div>`
                        }
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderSpecsPanel(data) {
    const selected = data.selected_spec;
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Specs</h1>
                    </div>
                    <span>${data.specs.length} specs</span>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Spec list</h2>
                            <span>${esc(data.workspace_root)}</span>
                        </div>
                        <div class="stack-list">
                            ${data.specs.length
                                ? data.specs
                                      .map(
                                          (spec) => `
                                            <button class="list-card ${selected && spec.id === selected.id ? "selected-card" : ""}" data-action="select-spec" data-path="${esc(spec.path)}">
                                                <div class="card-meta">
                                                    <span>${esc(spec.status)}</span>
                                                    <span>${percent(spec.completion_pct)}</span>
                                                </div>
                                                <h3>${esc(spec.title)}</h3>
                                                <p>${esc(spec.description)}</p>
                                                <div class="card-meta">
                                                    <span>${spec.checked_count}/${spec.entry_count} checked</span>
                                                    <span>${shortTime(spec.updated_at)}</span>
                                                </div>
                                            </button>
                                        `,
                                      )
                                      .join("")
                                : `<div class="empty-state compact"><h2>No specs found</h2><p>Add JSON specs under the workspace \`specs/\` directory to inspect them remotely.</p></div>`}
                        </div>
                    </div>
                    <div class="stack-card">
                        ${
                            selected
                                ? `
                                    <div class="section-header">
                                        <div>
                                            <h2>${esc(selected.title)}</h2>
                                            <span>${esc(selected.status)} · v${selected.version}</span>
                                        </div>
                                        <span>${percent(selected.completion_pct)}</span>
                                    </div>
                                    <p>${esc(selected.description)}</p>
                                    <div class="stack-list">
                                        ${selected.sections
                                            .map(
                                                (section) => `
                                                    <article class="list-card">
                                                        <div class="section-header">
                                                            <h3>${esc(section.section)}</h3>
                                                            <span>${section.entries.length} items</span>
                                                        </div>
                                                        <div class="stack-list">
                                                            ${section.entries
                                                                .map(
                                                                    (entry) => `
                                                                        <div class="list-card">
                                                                            <div class="card-meta">
                                                                                <span>${esc(entry.status)}</span>
                                                                                <span>${entry.checked ? "Checked" : "Open"}</span>
                                                                            </div>
                                                                            <h3>${esc(entry.title)}</h3>
                                                                            <p>${esc(entry.content)}</p>
                                                                        </div>
                                                                    `,
                                                                )
                                                                .join("") || `<p class="hero-summary">No entries in this section.</p>`}
                                                        </div>
                                                    </article>
                                                `,
                                            )
                                            .join("")}
                                    </div>
                                `
                                : `<div class="empty-state compact"><h2>No spec selected</h2><p>Select a spec to inspect progress, requirements, and notes.</p></div>`
                        }
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderAgentsPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Agents</h1>
                    </div>
                    <span>${data.active_runs.length} active</span>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Start run</h2>
                            <span>${esc(data.current_model)}</span>
                        </div>
                        <div class="stack-list">
                            <label class="field-label" for="agent-mode-select">Mode</label>
                            <select id="agent-mode-select">
                                ${data.orchestration_modes
                                    .map(
                                        (mode) => `
                                            <option value="${esc(mode.id)}" ${mode.id === state.agentMode ? "selected" : ""}>
                                                ${esc(mode.label)}
                                            </option>
                                        `,
                                    )
                                    .join("")}
                            </select>
                            <label class="field-label" for="agent-goal-input">Goal</label>
                            <textarea id="agent-goal-input" placeholder="Describe the task the agent should own...">${esc(state.agentGoal)}</textarea>
                            <button class="primary-button" data-action="start-agent">Start agent run</button>
                        </div>
                        ${
                            data.pending_approvals.length
                                ? `
                                    <div class="approval-list">
                                        ${data.pending_approvals.map(renderApprovalCard).join("")}
                                    </div>
                                `
                                : ""
                        }
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Active runs</h2>
                            <span>${data.active_runs.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.active_runs.length
                                ? data.active_runs
                                      .map(
                                          (run) => `
                                            <article class="list-card">
                                                <div class="card-meta">
                                                    <span>${esc(run.status)}</span>
                                                    <span>${duration(run.elapsed_ms)}</span>
                                                    <span>${money(run.cost_usd)}</span>
                                                </div>
                                                <h3>${esc(run.goal)}</h3>
                                                <p>${esc(run.detail)}</p>
                                                <div class="approval-actions">
                                                    <span class="hero-summary">${esc(run.run_id)}</span>
                                                    ${run.status === "running" || run.status === "planning" || run.status === "pending_approval"
                                                        ? `<button class="ghost-button" data-action="cancel-agent" data-run="${run.run_id}">Cancel</button>`
                                                        : ""}
                                                </div>
                                            </article>
                                        `,
                                      )
                                      .join("")
                                : `<div class="empty-state compact"><h2>No active runs</h2><p>Use the form to start a remote agent run.</p></div>`}
                        </div>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Recent runs</h2>
                        <span>${data.recent_runs.length} tracked</span>
                    </div>
                    <div class="table-like">
                        ${data.recent_runs
                            .map(
                                (run) => `
                                    <div class="table-row">
                                        <div>
                                            <strong>${esc(run.goal)}</strong>
                                            <span>${esc(run.run_id)}</span>
                                        </div>
                                        <div>
                                            <strong>${esc(run.status)}</strong>
                                            <span>${money(run.cost_usd)}</span>
                                        </div>
                                        <div>${duration(run.elapsed_ms)}</div>
                                    </div>
                                `,
                            )
                            .join("")}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderGitOpsPanel(data) {
    if (!data.is_repo) {
        return `
            <div class="empty-state">
                <div class="empty-mark"></div>
                <h2>Git repository not detected</h2>
                <p>${esc(data.error || "Open a workspace with a Git repository to use Git Ops remotely.")}</p>
            </div>
        `;
    }

    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Git Ops</h1>
                    </div>
                    <span>${esc(data.branch || "unknown branch")} · ${data.dirty_count} changed</span>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Working tree</h2>
                            <div class="approval-actions">
                                <button class="ghost-button" data-action="git-unstage-all">Unstage all</button>
                                <button class="ghost-button" data-action="git-stage-all">Stage all</button>
                            </div>
                        </div>
                        <div class="table-like">
                            ${data.files.length
                                ? data.files
                                      .map(
                                          (file) => `
                                            <div class="table-row">
                                                <div>
                                                    <strong>${esc(file.path)}</strong>
                                                    <span>${esc(file.status)}</span>
                                                </div>
                                                <div></div>
                                                <div>${esc(titleize(file.status))}</div>
                                            </div>
                                        `,
                                      )
                                      .join("")
                                : `<div class="empty-state compact"><h2>Working tree clean</h2><p>No modified files are waiting in this workspace.</p></div>`}
                        </div>
                        <div class="stack-list">
                            <label class="field-label" for="git-commit-message">Commit message</label>
                            <textarea id="git-commit-message" placeholder="Describe the checkpoint you want to create...">${esc(state.gitCommitMessage)}</textarea>
                            <button class="primary-button" data-action="git-commit">Create commit</button>
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Recent commits</h2>
                            <span>${data.commits.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.commits
                                .map(
                                    (commit) => `
                                        <article class="list-card">
                                            <div class="card-meta">
                                                <span>${esc(commit.short_hash)}</span>
                                                <span>${esc(commit.author)}</span>
                                                <span>${shortTime(commit.timestamp)}</span>
                                            </div>
                                            <p>${esc(commit.message)}</p>
                                        </article>
                                    `,
                                )
                                .join("")}
                        </div>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Diff</h2>
                        <span>${data.diff ? "Live snapshot" : "No diff"}</span>
                    </div>
                    ${data.diff ? `<pre class="code-block">${esc(data.diff)}</pre>` : `<p class="hero-summary">No diff content available.</p>`}
                </div>
            </section>
        </div>
    `;
}

function renderTerminalPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Build</div>
                        <h1>Terminal</h1>
                    </div>
                    <div class="approval-actions">
                        <span>${esc(data.cwd)}</span>
                        <button class="ghost-button" data-action="terminal-clear">Clear</button>
                        <button class="ghost-button" data-action="terminal-kill">Stop</button>
                        <button class="primary-button" data-action="terminal-start">${data.is_running ? "Running" : "Start shell"}</button>
                    </div>
                </div>
                <div class="terminal-card">
                    <div class="terminal-meta">
                        <span>${data.is_running ? "Live shell" : "Stopped"}</span>
                        <span>${data.last_exit_code === null ? "No exit code" : `Exit ${data.last_exit_code}`}</span>
                    </div>
                    <div class="terminal-output" id="terminal-output">
                        ${data.lines.length
                            ? data.lines
                                  .map(
                                      (line) => `
                                        <div class="terminal-line ${esc(line.stream)}">
                                            <span class="terminal-stream">${esc(line.stream)}</span>
                                            <code>${esc(line.content)}</code>
                                        </div>
                                    `,
                                  )
                                  .join("")
                            : `<div class="empty-state compact"><h2>No terminal output yet</h2><p>Start the shell and run a command from the current workspace.</p></div>`}
                    </div>
                    <div class="composer">
                        <textarea id="terminal-input" placeholder="Run a command remotely...">${esc(state.terminalInput)}</textarea>
                        <div class="composer-actions">
                            <span class="composer-meta">Commands run on the paired machine in the active workspace.</span>
                            <button class="primary-button" data-action="terminal-send">Send</button>
                        </div>
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderWorkflowsPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Automate</div>
                        <h1>Workflows</h1>
                    </div>
                    <span>${data.workflows.length} loaded</span>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Workflow library</h2>
                            <span>${esc(data.source_dir)}</span>
                        </div>
                        <div class="stack-list">
                            ${data.workflows.map((workflow) => `
                                <article class="list-card">
                                    <div class="card-meta">
                                        <span>${esc(workflow.status)}</span>
                                        <span>${esc(workflow.trigger)}</span>
                                        <span>${workflow.step_count} steps</span>
                                    </div>
                                    <h3>${esc(workflow.name)}</h3>
                                    <p>${esc(workflow.description)}</p>
                                    <div class="approval-actions">
                                        <span class="hero-summary">${workflow.run_count} runs${workflow.last_run ? ` · ${shortTime(workflow.last_run)}` : ""}</span>
                                        <button class="primary-button" data-action="run-workflow" data-workflow="${workflow.id}">
                                            Run workflow
                                        </button>
                                    </div>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Active runs</h2>
                            <span>${data.active_runs.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.active_runs.length ? data.active_runs.map((run) => `
                                <article class="list-card">
                                    <div class="card-meta">
                                        <span>${esc(run.status)}</span>
                                        <span>${shortTime(run.started_at)}</span>
                                    </div>
                                    <h3>${esc(run.workflow_name)}</h3>
                                    <p>${run.steps_completed} steps completed</p>
                                </article>
                            `).join("") : `<div class="empty-state compact"><h2>No active workflow runs</h2><p>Manual workflow runs from the remote shell will show up here.</p></div>`}
                        </div>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Recent runs</h2>
                        <span>${data.recent_runs.length}</span>
                    </div>
                    <div class="table-like">
                        ${data.recent_runs.map((run) => `
                            <div class="table-row">
                                <div>
                                    <strong>${esc(run.workflow_name)}</strong>
                                    <span>${esc(run.workflow_id)}</span>
                                </div>
                                <div>
                                    <strong>${esc(run.status)}</strong>
                                    <span>${run.error ? esc(run.error) : `${run.steps_completed} steps`}</span>
                                </div>
                                <div>${shortTime(run.completed_at || run.started_at)}</div>
                            </div>
                        `).join("")}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderChannelsPanel(data) {
    const channel = data.selected_channel;
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Automate</div>
                        <h1>Channels</h1>
                    </div>
                    <span>${data.channels.length} channels</span>
                </div>
                <div class="chat-shell">
                    <aside class="chat-sidebar">
                        <div class="section-header">
                            <h2>Channel list</h2>
                            <span>${esc(data.current_model)}</span>
                        </div>
                        <div class="conversation-list">
                            ${data.channels.map((entry) => `
                                <button class="conversation-card ${entry.id === data.selected_channel_id ? "active" : ""}" data-action="select-channel" data-channel="${entry.id}">
                                    <strong>${esc(entry.icon)} ${esc(entry.name)}</strong>
                                    <span>${esc(entry.description)}</span>
                                    <div class="card-meta">
                                        <span>${entry.message_count} messages</span>
                                        <span>${entry.assigned_agents.join(", ")}</span>
                                    </div>
                                </button>
                            `).join("")}
                        </div>
                    </aside>
                    <div class="chat-main">
                        ${channel ? `
                            <div class="chat-toolbar">
                                <div>
                                    <div class="eyebrow">Channel</div>
                                    <h1>${esc(channel.icon)} ${esc(channel.name)}</h1>
                                </div>
                                <div class="toolbar-actions">
                                    <span class="status-pill subtle">${channel.assigned_agents.join(", ") || "No agents assigned"}</span>
                                </div>
                            </div>
                            <section class="chat-transcript">
                                ${channel.messages.length ? channel.messages.map((message) => `
                                    <article class="message-card ${esc(message.author_type)}">
                                        <div class="message-body">${renderMarkdown(message.content)}</div>
                                        <div class="message-meta">
                                            <span>${esc(message.author_label)}</span>
                                            <span>${shortTime(message.timestamp)}</span>
                                            ${message.model ? `<span>${esc(message.model)}</span>` : ""}
                                        </div>
                                    </article>
                                `).join("") : `<div class="empty-state compact"><h2>No channel messages yet</h2><p>Send a message to seed remote collaboration in this channel.</p></div>`}
                            </section>
                            <section class="composer">
                                <textarea id="channel-input" placeholder="Send a message to this channel...">${esc(state.channelDraft)}</textarea>
                                <div class="composer-actions">
                                    <span class="composer-meta">${channel.pinned_files.length} pinned files</span>
                                    <button class="primary-button" data-action="send-channel-message" data-channel="${channel.id}">
                                        Send
                                    </button>
                                </div>
                            </section>
                        ` : `<div class="empty-state compact"><h2>Select a channel</h2><p>Choose a channel to inspect recent conversation and send a message.</p></div>`}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderNetworkPanel(data) {
    if (!data.available) {
        return `
            <div class="empty-state">
                <div class="empty-mark"></div>
                <h2>Network runtime unavailable</h2>
                <p>${esc(data.note || "This remote daemon was not attached to the desktop network runtime.")}</p>
            </div>
        `;
    }

    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Automate</div>
                        <h1>Network</h1>
                    </div>
                    <span>${data.connected_count}/${data.total_count} connected</span>
                </div>
                <div class="status-grid">
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.connected_count}</div>
                        <h3>Connected peers</h3>
                        <p>Peers currently online from the desktop runtime</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.total_count}</div>
                        <h3>Known peers</h3>
                        <p>Discovered and connected peers visible to Hive</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${esc(data.our_peer_id || "n/a")}</div>
                        <h3>Peer ID</h3>
                        <p>Identity of the paired desktop node</p>
                    </article>
                </div>
                <div class="table-like">
                    ${data.peers.map((peer) => `
                        <div class="table-row">
                            <div>
                                <strong>${esc(peer.name)}</strong>
                                <span>${esc(peer.address)}</span>
                            </div>
                            <div>
                                <strong>${esc(peer.status)}</strong>
                                <span>${peer.latency_ms == null ? "No latency" : `${peer.latency_ms} ms`}</span>
                            </div>
                            <div>${esc(peer.last_seen)}</div>
                        </div>
                    `).join("")}
                </div>
            </section>
        </div>
    `;
}

function renderAssistantPanel(data) {
    const briefing = data.briefing;
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Assist</div>
                        <h1>Assistant</h1>
                    </div>
                    <span>${data.connected_account_count} connected accounts</span>
                </div>
                ${briefing ? `
                    <div class="status-grid">
                        <article class="status-card tone-calm">
                            <div class="status-value">${briefing.event_count}</div>
                            <h3>Today's events</h3>
                            <p>${esc(briefing.date)}</p>
                        </article>
                        <article class="status-card tone-calm">
                            <div class="status-value">${briefing.unread_emails}</div>
                            <h3>Unread email</h3>
                            <p>Connected inbox summary</p>
                        </article>
                        <article class="status-card tone-calm">
                            <div class="status-value">${briefing.active_reminders}</div>
                            <h3>Active reminders</h3>
                            <p>${briefing.top_priority ? esc(briefing.top_priority) : "No top priority"}</p>
                        </article>
                    </div>
                ` : ""}
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Approvals</h2>
                            <span>${data.approvals.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.approvals.length ? data.approvals.map((approval) => `
                                <article class="approval-card severity-${esc(approval.level.toLowerCase())}">
                                    <div class="card-meta">
                                        <span>${esc(approval.requested_by)}</span>
                                        <span>${esc(approval.created_at)}</span>
                                    </div>
                                    <h3>${esc(approval.action)}</h3>
                                    <p>${esc(approval.resource)}</p>
                                    <div class="approval-actions">
                                        <button class="ghost-button" data-action="assistant-decision" data-approval="${approval.id}" data-approved="false">Reject</button>
                                        <button class="primary-button" data-action="assistant-decision" data-approval="${approval.id}" data-approved="true">Approve</button>
                                    </div>
                                </article>
                            `).join("") : `<div class="empty-state compact"><h2>No assistant approvals</h2><p>Pending personal approvals will appear here.</p></div>`}
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Reminders</h2>
                            <span>${data.reminders.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.reminders.map((reminder) => `
                                <article class="list-card ${reminder.is_overdue ? "tone-warning" : ""}">
                                    <div class="card-meta">
                                        <span>${reminder.is_overdue ? "Overdue" : "Active"}</span>
                                        <span>${esc(reminder.due)}</span>
                                    </div>
                                    <h3>${esc(reminder.title)}</h3>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Events</h2>
                            <span>${data.events.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.events.map((event) => `
                                <article class="list-card">
                                    <div class="card-meta">
                                        <span>${esc(event.time)}</span>
                                        <span>${event.location ? esc(event.location) : "No location"}</span>
                                    </div>
                                    <h3>${esc(event.title)}</h3>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Recent actions</h2>
                            <span>${data.recent_actions.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.recent_actions.map((action) => `
                                <article class="list-card">
                                    <div class="card-meta">
                                        <span>${esc(action.action_type)}</span>
                                        <span>${esc(action.timestamp)}</span>
                                    </div>
                                    <p>${esc(action.description)}</p>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderSettingsPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Utility</div>
                        <h1>Settings</h1>
                    </div>
                    <span>${esc(data.current_workspace)}</span>
                </div>
                <div class="status-grid">
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.privacy_mode ? "On" : "Off"}</div>
                        <h3>Privacy mode</h3>
                        <p>Remote shell redaction and provider-safe defaults</p>
                        <button class="inline-link" data-action="toggle-setting" data-setting="privacy_mode" data-value="${(!data.privacy_mode).toString()}">
                            ${data.privacy_mode ? "Turn off" : "Turn on"}
                        </button>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.shield_enabled ? "On" : "Off"}</div>
                        <h3>Shield</h3>
                        <p>Desktop safety posture mirrored into remote</p>
                        <button class="inline-link" data-action="toggle-setting" data-setting="shield_enabled" data-value="${(!data.shield_enabled).toString()}">
                            ${data.shield_enabled ? "Relax shield" : "Enable shield"}
                        </button>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.remote_enabled ? "On" : "Off"}</div>
                        <h3>Remote</h3>
                        <p>${data.remote_auto_start ? "Auto-start enabled" : "Start remote manually"}</p>
                        <button class="inline-link" data-action="toggle-setting" data-setting="remote_enabled" data-value="${(!data.remote_enabled).toString()}">
                            ${data.remote_enabled ? "Disable on restart" : "Enable on restart"}
                        </button>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.connected_account_count}</div>
                        <h3>Connected accounts</h3>
                        <p>Calendar, mail, and assistant integrations</p>
                    </article>
                </div>
            </section>
            <section class="panel-section split-grid">
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Runtime</h2>
                        <span>${esc(data.theme)}</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <h3>Notifications</h3>
                            <p>${data.notifications_enabled ? "Enabled" : "Disabled"}</p>
                            <button class="inline-link" data-action="toggle-setting" data-setting="notifications_enabled" data-value="${(!data.notifications_enabled).toString()}">
                                ${data.notifications_enabled ? "Disable" : "Enable"}
                            </button>
                        </article>
                        <article class="list-card">
                            <h3>Auto update</h3>
                            <p>${data.auto_update ? "Enabled" : "Disabled"}</p>
                            <button class="inline-link" data-action="toggle-setting" data-setting="auto_update" data-value="${(!data.auto_update).toString()}">
                                ${data.auto_update ? "Disable" : "Enable"}
                            </button>
                        </article>
                        <article class="list-card">
                            <h3>Remote auto-start</h3>
                            <p>${data.remote_auto_start ? "Enabled" : "Disabled"}</p>
                            <button class="inline-link" data-action="toggle-setting" data-setting="remote_auto_start" data-value="${(!data.remote_auto_start).toString()}">
                                ${data.remote_auto_start ? "Disable" : "Enable"}
                            </button>
                        </article>
                        <article class="list-card">
                            <h3>Theme</h3>
                            <p>${esc(data.theme)}</p>
                        </article>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Provider endpoints</h2>
                        <span>Local inference</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <label class="field-label" for="setting-ollama-url">Ollama URL</label>
                            <input id="setting-ollama-url" type="url" value="${esc(data.ollama_url)}">
                            <div class="composer-actions">
                                <span class="composer-meta">Primary local model runtime.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="ollama_url" data-input="setting-ollama-url">Save</button>
                            </div>
                        </article>
                        <article class="list-card">
                            <label class="field-label" for="setting-lmstudio-url">LM Studio URL</label>
                            <input id="setting-lmstudio-url" type="url" value="${esc(data.lmstudio_url)}">
                            <div class="composer-actions">
                                <span class="composer-meta">OpenAI-compatible local endpoint.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="lmstudio_url" data-input="setting-lmstudio-url">Save</button>
                            </div>
                        </article>
                        <article class="list-card">
                            <label class="field-label" for="setting-litellm-url">LiteLLM URL</label>
                            <input id="setting-litellm-url" type="url" value="${esc(data.litellm_url || "")}" placeholder="https://litellm.example/v1">
                            <div class="composer-actions">
                                <span class="composer-meta">Optional shared gateway or proxy.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="litellm_url" data-input="setting-litellm-url">Save</button>
                            </div>
                        </article>
                        <article class="list-card">
                            <label class="field-label" for="setting-local-provider-url">Custom provider URL</label>
                            <input id="setting-local-provider-url" type="url" value="${esc(data.local_provider_url || "")}" placeholder="https://provider.example/v1">
                            <div class="composer-actions">
                                <span class="composer-meta">Optional generic OpenAI-compatible endpoint.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="local_provider_url" data-input="setting-local-provider-url">Save</button>
                            </div>
                        </article>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Ports</h2>
                        <span>Remote transport</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <h3>Local API</h3>
                            <p>Port ${data.remote_local_port}</p>
                        </article>
                        <article class="list-card">
                            <h3>Web shell</h3>
                            <p>Port ${data.remote_web_port}</p>
                        </article>
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderModelsPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Utility</div>
                        <h1>Models</h1>
                    </div>
                    <span>${data.available_models.length} options</span>
                </div>
                <div class="status-grid">
                    <article class="status-card tone-calm">
                        <div class="status-value">${esc(data.current_model)}</div>
                        <h3>Current remote model</h3>
                        <p>What the remote shell will use next</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${esc(data.default_model)}</div>
                        <h3>Default desktop model</h3>
                        <p>Base model from Hive config</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.auto_routing ? "On" : "Off"}</div>
                        <h3>Auto routing</h3>
                        <p>${data.project_models.length} project model overrides</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.available_providers.length}</div>
                        <h3>Live providers</h3>
                        <p>${data.configured_providers.length} configured providers</p>
                    </article>
                </div>
            </section>
            <section class="panel-section split-grid">
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Available models</h2>
                        <span>Remote-ready</span>
                    </div>
                        <div class="stack-list">
                            ${data.available_models.map((model) => `
                                <article class="list-card">
                                    <div class="card-meta">
                                        <span>${model.id === data.current_model ? "Current" : "Available"}</span>
                                        <span>${model.id === data.default_model ? "Default" : "Optional"}</span>
                                    </div>
                                    <h3>${esc(model.label)}</h3>
                                    <p>${esc(model.id)}</p>
                                    <div class="approval-actions">
                                        ${model.id === data.current_model ? "" : `<button class="ghost-button" data-action="set-current-model" data-model="${esc(model.id)}">Use now</button>`}
                                        ${model.id === data.default_model ? "" : `<button class="primary-button" data-action="set-default-model" data-model="${esc(model.id)}">Make default</button>`}
                                    </div>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Providers</h2>
                        <span>Configured vs live</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <h3>Currently available</h3>
                            <p>${data.available_providers.length ? esc(data.available_providers.join(", ")) : "None detected"}</p>
                        </article>
                        <article class="list-card">
                            <h3>Configured</h3>
                            <p>${data.configured_providers.length ? esc(data.configured_providers.join(", ")) : "None configured"}</p>
                        </article>
                        ${data.provider_credentials.map((provider) => `
                            <article class="list-card">
                                <div class="card-meta">
                                    <span>${provider.has_key ? "Configured" : "Missing key"}</span>
                                    <span>${esc(provider.id)}</span>
                                </div>
                                <h3>${esc(provider.label)}</h3>
                                <p>${provider.has_key ? "A provider key is already stored for this runtime." : "Add a provider key to unlock remote model access."}</p>
                                <div class="field-stack">
                                    <label for="provider-key-${esc(provider.id)}">API key</label>
                                    <input id="provider-key-${esc(provider.id)}" type="password" placeholder="${provider.has_key ? "Replace or clear key" : "Paste API key"}" />
                                </div>
                                <div class="approval-actions">
                                    <button class="primary-button" data-action="save-provider-key" data-provider="${esc(provider.id)}" data-input="provider-key-${esc(provider.id)}">
                                        Save key
                                    </button>
                                </div>
                            </article>
                        `).join("")}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderRoutingPanel(data) {
    return `
        <div class="panel-stack">
            <section class="hero-card">
                <div class="eyebrow">Utility</div>
                <h1>Routing</h1>
                <p class="hero-summary">${esc(data.strategy_summary)}</p>
                <div class="hero-meta">
                    <span>${data.auto_routing ? "Auto routing enabled" : "Auto routing disabled"}</span>
                    <span>${esc(data.default_model)}</span>
                    <span>${data.available_providers.length} providers available</span>
                </div>
                <div class="approval-actions">
                    <button class="${data.auto_routing ? "ghost-button" : "primary-button"}" data-action="set-auto-routing" data-enabled="${(!data.auto_routing).toString()}">
                        ${data.auto_routing ? "Disable auto routing" : "Enable auto routing"}
                    </button>
                </div>
            </section>
            <section class="panel-section split-grid">
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Project models</h2>
                        <span>${data.project_models.length}</span>
                    </div>
                    <div class="field-row">
                        <input id="routing-project-model" type="text" placeholder="Add model override, e.g. gpt-5-mini" />
                        <button class="primary-button" data-action="add-project-model" data-input="routing-project-model">
                            Add model
                        </button>
                    </div>
                    <div class="stack-list">
                        ${data.project_models.length ? data.project_models.map((model) => `
                            <article class="list-card">
                                <h3>${esc(model)}</h3>
                                <div class="approval-actions">
                                    <button class="ghost-button" data-action="remove-project-model" data-model="${esc(model)}">
                                        Remove
                                    </button>
                                </div>
                            </article>
                        `).join("") : `<div class="empty-state compact"><h2>No project overrides</h2><p>Hive will route against the default fallback chain.</p></div>`}
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Routing notes</h2>
                        <span>${data.available_providers.length} providers</span>
                    </div>
                    <div class="stack-list">
                        ${data.notes.map((note) => `
                            <article class="list-card">
                                <p>${esc(note)}</p>
                            </article>
                        `).join("")}
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderSkillsPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Utility</div>
                        <h1>Skills</h1>
                    </div>
                    <span>${esc(data.skills_dir)}</span>
                </div>
                <div class="status-grid">
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.total_skills}</div>
                        <h3>Total skills</h3>
                        <p>${data.enabled_skills} enabled</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.builtin_skills}</div>
                        <h3>Built-in</h3>
                        <p>Bundled with Hive</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.community_skills}</div>
                        <h3>Community</h3>
                        <p>Installed from shared sources</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.custom_skills}</div>
                        <h3>Custom</h3>
                        <p>User-authored skills</p>
                    </article>
                </div>
            </section>
            <section class="panel-section">
                <div class="section-header">
                    <h2>Installed skills</h2>
                    <span>Top 24</span>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Create custom skill</h2>
                        <span>Remote authoring</span>
                    </div>
                    <div class="stack-list">
                        <label class="field-label" for="skill-create-name">Name</label>
                        <input id="skill-create-name" type="text" placeholder="release-check">
                        <label class="field-label" for="skill-create-description">Description</label>
                        <input id="skill-create-description" type="text" placeholder="Checks release readiness">
                        <label class="field-label" for="skill-create-instructions">Instructions</label>
                        <textarea id="skill-create-instructions" placeholder="Describe exactly how the skill should behave..."></textarea>
                        <div class="composer-actions">
                            <span class="composer-meta">Creates a custom TOML-backed skill in the paired Hive config root.</span>
                            <button class="primary-button" data-action="install-skill">Create custom skill</button>
                        </div>
                    </div>
                </div>
                <div class="stack-list">
                    ${data.skills.map((skill) => `
                        <article class="list-card">
                            <div class="card-meta">
                                <span>${esc(skill.source)}</span>
                                <span>${skill.enabled ? "Enabled" : "Disabled"}</span>
                            </div>
                            <h3>${esc(skill.name)}</h3>
                            <p>${esc(skill.description)}</p>
                            <div class="approval-actions">
                                <button class="inline-link" data-action="toggle-skill" data-skill="${esc(skill.name)}" data-enabled="${(!skill.enabled).toString()}">
                                    ${skill.enabled ? "Disable" : "Enable"}
                                </button>
                                ${skill.source === "BuiltIn" ? "" : `<button class="ghost-button" data-action="remove-skill" data-skill="${esc(skill.name)}">Remove</button>`}
                            </div>
                        </article>
                    `).join("")}
                </div>
            </section>
        </div>
    `;
}

function renderLaunchPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Utility</div>
                        <h1>Launch</h1>
                    </div>
                    <span>${data.remote_enabled ? "Remote enabled" : "Remote disabled"}</span>
                </div>
                <div class="status-grid">
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.remote_enabled ? "On" : "Off"}</div>
                        <h3>Remote access</h3>
                        <p>${data.remote_auto_start ? "Auto-start enabled" : "Start manually from desktop"}</p>
                        <button class="inline-link" data-action="toggle-setting" data-setting="remote_enabled" data-value="${(!data.remote_enabled).toString()}">
                            ${data.remote_enabled ? "Disable remote" : "Enable remote"}
                        </button>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.web_port}</div>
                        <h3>Web shell port</h3>
                        <p>${esc(data.web_url)}</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.local_api_port}</div>
                        <h3>Local API port</h3>
                        <p>${esc(data.local_api_url)}</p>
                    </article>
                    <article class="status-card tone-calm">
                        <div class="status-value">${data.cloud_tier ? esc(data.cloud_tier) : "Local"}</div>
                        <h3>Cloud tier</h3>
                        <p>${data.cloud_api_url ? "Cloud transport configured" : "No cloud API configured"}</p>
                    </article>
                </div>
            </section>
            <section class="panel-section split-grid">
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Endpoints</h2>
                        <span>Current runtime</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <h3>Remote web shell</h3>
                            <p>${esc(data.web_url)}</p>
                            <button class="inline-link" data-action="toggle-setting" data-setting="remote_auto_start" data-value="${(!data.remote_auto_start).toString()}">
                                ${data.remote_auto_start ? "Disable auto-start" : "Enable auto-start"}
                            </button>
                        </article>
                        <article class="list-card">
                            <h3>Remote local API</h3>
                            <p>${esc(data.local_api_url)}</p>
                        </article>
                    </div>
                </div>
                <div class="stack-card">
                    <div class="section-header">
                        <h2>Cloud</h2>
                        <span>${data.cloud_tier ? esc(data.cloud_tier) : "Not configured"}</span>
                    </div>
                    <div class="stack-list">
                        <article class="list-card">
                            <h3>Cloud API</h3>
                            <p>${data.cloud_api_url ? esc(data.cloud_api_url) : "Not configured"}</p>
                            <label class="field-label" for="launch-cloud-api-url">Cloud API URL</label>
                            <input id="launch-cloud-api-url" type="url" value="${esc(data.cloud_api_url || "")}" placeholder="https://api.hive.cloud">
                            <div class="composer-actions">
                                <span class="composer-meta">Used for cloud account and API services.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="cloud_api_url" data-input="launch-cloud-api-url">Save</button>
                            </div>
                        </article>
                        <article class="list-card">
                            <h3>Cloud relay</h3>
                            <p>${data.cloud_relay_url ? esc(data.cloud_relay_url) : "Not configured"}</p>
                            <label class="field-label" for="launch-cloud-relay-url">Cloud relay URL</label>
                            <input id="launch-cloud-relay-url" type="url" value="${esc(data.cloud_relay_url || "")}" placeholder="wss://relay.hive.cloud/ws">
                            <div class="composer-actions">
                                <span class="composer-meta">WebSocket relay for remote connectivity.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="cloud_relay_url" data-input="launch-cloud-relay-url">Save</button>
                            </div>
                        </article>
                        <article class="list-card">
                            <h3>Cloud tier</h3>
                            <p>${data.cloud_tier ? esc(data.cloud_tier) : "Not configured"}</p>
                            <label class="field-label" for="launch-cloud-tier">Cloud tier</label>
                            <input id="launch-cloud-tier" type="text" value="${esc(data.cloud_tier || "")}" placeholder="Pro">
                            <div class="composer-actions">
                                <span class="composer-meta">Used for remote plan and capability messaging.</span>
                                <button class="primary-button" data-action="save-text-setting" data-setting="cloud_tier" data-input="launch-cloud-tier">Save</button>
                            </div>
                        </article>
                    </div>
                </div>
            </section>
        </div>
    `;
}

function renderHelpPanel(data) {
    return `
        <div class="panel-stack">
            <section class="panel-section">
                <div class="section-header">
                    <div>
                        <div class="eyebrow">Utility</div>
                        <h1>Help</h1>
                    </div>
                    <span>Hive Remote v${esc(data.version)}</span>
                </div>
                <div class="split-grid">
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Guides</h2>
                            <span>${data.docs.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.docs.map((doc) => `
                                <article class="list-card">
                                    <h3>${esc(doc.title)}</h3>
                                    <p>${esc(doc.detail)}</p>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                    <div class="stack-card">
                        <div class="section-header">
                            <h2>Quick tips</h2>
                            <span>${data.quick_tips.length}</span>
                        </div>
                        <div class="stack-list">
                            ${data.quick_tips.map((tip) => `
                                <article class="list-card">
                                    <p>${esc(tip)}</p>
                                </article>
                            `).join("")}
                        </div>
                    </div>
                </div>
            </section>
            <section class="panel-section">
                <div class="section-header">
                    <h2>Troubleshooting</h2>
                    <span>${data.troubleshooting.length}</span>
                </div>
                <div class="stack-list">
                    ${data.troubleshooting.map((item) => `
                        <article class="list-card">
                            <p>${esc(item)}</p>
                        </article>
                    `).join("")}
                </div>
            </section>
        </div>
    `;
}

function renderMessageCard(message) {
    return `
        <article class="message-card ${esc(message.role)}">
            <div class="message-body">${renderMarkdown(message.content)}</div>
            <div class="message-meta">
                <span>${esc(message.role)}</span>
                <span>${shortTime(message.timestamp)}</span>
                ${message.model ? `<span>${esc(message.model)}</span>` : ""}
                ${message.cost ? `<span>${money(message.cost)}</span>` : ""}
            </div>
        </article>
    `;
}

function renderApprovalCard(card) {
    return `
        <article class="approval-card severity-${esc(card.severity)}">
            <div class="card-meta">
                <span>${esc(card.source)}</span>
                <span>${esc(card.created_at)}</span>
            </div>
            <h3>${esc(card.title)}</h3>
            <p>${esc(card.detail)}</p>
            <div class="approval-actions">
                <button class="ghost-button" data-action="deny-request" data-request="${card.id}">
                    Deny
                </button>
                <button class="primary-button" data-action="approve-request" data-request="${card.id}">
                    Approve
                </button>
            </div>
        </article>
    `;
}

function renderHandoffPanel(data) {
    return `
        <div class="empty-state">
            <div class="empty-mark"></div>
            <div class="eyebrow">${esc(data.panel)}</div>
            <h2>${esc(data.title)}</h2>
            <p>${esc(data.description)}</p>
            <button class="primary-button" data-action="open-desktop">${esc(data.action_label)}</button>
        </div>
    `;
}

function renderContextRail() {
    if (!state.snapshot) return "";
    if (!["build", "observe"].includes(state.snapshot.active_destination)) return "";

    const data = activePanelData();
    if (state.snapshot.active_destination === "build") {
        const chat = state.panels.chat && state.panels.chat.kind === "chat" ? state.panels.chat : null;
        if (!chat) return "";
        return `
            <aside class="context-rail">
                <div class="rail-card">
                    <div class="eyebrow">Build context</div>
                    <h3>Remote execution</h3>
                    <p>${state.snapshot.is_streaming ? "Hive is actively streaming a response." : "Hive is ready for the next instruction."}</p>
                </div>
                <div class="rail-card">
                    <label>Current model</label>
                    <strong>${esc(chat.current_model)}</strong>
                    <span>${chat.pending_approvals.length} pending approvals</span>
                </div>
                <div class="rail-card">
                    <div class="section-header">
                        <h3>Recent threads</h3>
                        <span>${chat.conversations.length}</span>
                    </div>
                    <div class="mini-list">
                        ${chat.conversations
                            .slice(0, 3)
                            .map(
                                (conversation) => `
                                    <button class="mini-row" data-action="resume-conversation" data-conversation="${conversation.id}">
                                        <strong>${esc(conversation.title)}</strong>
                                        <span>${esc(conversation.preview)}</span>
                                    </button>
                                `,
                            )
                            .join("")}
                    </div>
                </div>
            </aside>
        `;
    }

    if (data && data.kind === "observe") {
        return `
            <aside class="context-rail">
                <div class="rail-card">
                    <div class="eyebrow">Observe</div>
                    <h3>Queue health</h3>
                    <p>${data.runtime.status_label} with ${data.runtime.active_agents} active agents and ${data.approvals.length} approvals waiting.</p>
                </div>
                <div class="rail-card">
                    <label>Today</label>
                    <strong>${money(data.spend.today_cost_usd)}</strong>
                    <span>${percent(data.spend.quality_score)} quality score</span>
                </div>
                <div class="rail-card">
                    <label>Safety</label>
                    <strong>${data.safety.shield_enabled ? "Shield enabled" : "Shield relaxed"}</strong>
                    <span>${data.safety.threats_caught} threats caught</span>
                </div>
            </aside>
        `;
    }

    return "";
}

function renderUtilityDrawer() {
    return `
        <div class="utility-drawer ${state.utilityOpen ? "open" : ""}">
            <div class="utility-backdrop" data-action="toggle-utility"></div>
            <div class="utility-panel">
                <div class="section-header">
                    <h2>Utilities</h2>
                    <button class="icon-button" data-action="toggle-utility">Close</button>
                </div>
                <p class="utility-copy">Utility surfaces stay available without taking over the primary shell.</p>
                <div class="stack-list">
                    ${utilityPanels()
                        .map(
                            (panel) => `
                                <button class="utility-row" data-action="switch-panel" data-panel="${panel.id}">
                                    <strong>${esc(panel.label)}</strong>
                                    <span>${esc(panel.description)}</span>
                                </button>
                            `,
                        )
                        .join("")}
                </div>
            </div>
        </div>
    `;
}

function render() {
    const app = document.getElementById("app");
    if (!app) return;
    app.innerHTML = renderApp();
    hydrateAfterRender();
}

function hydrateAfterRender() {
    const chatInput = document.getElementById("chat-input");
    if (chatInput) {
        autoResize(chatInput);
        if (state.snapshot?.active_panel === "chat") {
            requestAnimationFrame(() => {
                chatInput.focus();
                chatInput.selectionStart = chatInput.value.length;
                chatInput.selectionEnd = chatInput.value.length;
            });
        }
    }

    const channelInput = document.getElementById("channel-input");
    if (channelInput) autoResize(channelInput);

    const homeInput = document.getElementById("home-detail-input");
    if (homeInput) autoResize(homeInput);

    const agentInput = document.getElementById("agent-goal-input");
    if (agentInput) autoResize(agentInput);

    const gitInput = document.getElementById("git-commit-message");
    if (gitInput) autoResize(gitInput);

    const terminalInput = document.getElementById("terminal-input");
    if (terminalInput) autoResize(terminalInput);

    const transcript = document.getElementById("chat-transcript");
    if (transcript) transcript.scrollTop = transcript.scrollHeight;

    const terminalOutput = document.getElementById("terminal-output");
    if (terminalOutput) terminalOutput.scrollTop = terminalOutput.scrollHeight;
}

async function sendChatMessage() {
    const content = state.chatDraft.trim();
    if (!content) return;

    const chatData = state.panels.chat && state.panels.chat.kind === "chat" ? state.panels.chat : null;
    const conversationId =
        state.pendingConversationId ||
        chatData?.conversation_id ||
        state.snapshot?.active_conversation ||
        uuid4();

    if (!chatData?.conversation_id && !state.snapshot?.active_conversation) {
        state.pendingConversationId = conversationId;
    }

    state.optimisticMessages.push({
        conversationId,
        role: "user",
        content,
        timestamp: new Date().toISOString(),
        model: null,
        cost: null,
        tokens: null,
    });

    const draft = content;
    state.chatDraft = "";
    render();

    try {
        const response = await api("/api/chat", {
            method: "POST",
            body: JSON.stringify({
                conversation_id: conversationId,
                content: draft,
                model: state.snapshot.current_model,
            }),
        });
        if (response.snapshot) {
            syncSnapshot(response.snapshot);
        }
        await ensurePanels(["chat", "home", "observe", response.snapshot?.active_panel]);
    } catch (error) {
        showToast(error.message, "error");
    }
}

async function applySnapshotResponse(promise, extraPanels) {
    const response = await promise;
    if (response.snapshot) {
        syncSnapshot(response.snapshot);
        await ensurePanels(["home", "observe", "chat", response.snapshot.active_panel].concat(extraPanels || []));
    }
    render();
    return response;
}

function autoResize(element) {
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 220)}px`;
}

async function boot() {
    render();
    try {
        await loadState();
        await ensurePanels(["home", "observe", "chat", state.snapshot.active_panel]);
    } catch (error) {
        showToast(error.message, "error");
    } finally {
        state.loading = false;
        render();
        connectSocket();
    }
}

document.addEventListener("click", async (event) => {
    const target = event.target.closest("[data-action]");
    if (!target) return;

    const { action } = target.dataset;

    try {
        switch (action) {
            case "toggle-utility":
                state.utilityOpen = !state.utilityOpen;
                render();
                break;
            case "switch-destination":
                state.utilityOpen = false;
                await applySnapshotResponse(
                    api("/api/navigation", {
                        method: "POST",
                        body: JSON.stringify({ destination: target.dataset.destination }),
                    }),
                );
                break;
            case "switch-panel":
                state.utilityOpen = false;
                await applySnapshotResponse(
                    api("/api/navigation", {
                        method: "POST",
                        body: JSON.stringify({ panel: target.dataset.panel }),
                    }),
                    [target.dataset.panel],
                );
                break;
            case "switch-observe-view":
                await applySnapshotResponse(
                    api("/api/navigation", {
                        method: "POST",
                        body: JSON.stringify({ observe_view: target.dataset.view }),
                    }),
                    ["observe"],
                );
                break;
            case "switch-workspace":
                await applySnapshotResponse(
                    api("/api/workspaces/switch", {
                        method: "POST",
                        body: JSON.stringify({ workspace_path: target.dataset.workspace }),
                    }),
                );
                break;
            case "launch-template":
                await applySnapshotResponse(
                    api("/api/home/launch", {
                        method: "POST",
                        body: JSON.stringify({
                            template_id: target.dataset.template,
                            detail: state.homeDetail.trim(),
                        }),
                    }),
                    ["chat", "home", "observe"],
                );
                break;
            case "resume-conversation":
                state.pendingConversationId = null;
                state.streaming = { conversationId: null, content: "" };
                await applySnapshotResponse(
                    api("/api/conversations/resume", {
                        method: "POST",
                        body: JSON.stringify({ conversation_id: target.dataset.conversation }),
                    }),
                    ["chat"],
                );
                break;
            case "file-breadcrumb":
                await applySnapshotResponse(
                    api("/api/files/navigate", {
                        method: "POST",
                        body: JSON.stringify({ path: target.dataset.path }),
                    }),
                    ["files"],
                );
                break;
            case "open-file":
                await applySnapshotResponse(
                    api("/api/files/open", {
                        method: "POST",
                        body: JSON.stringify({ path: target.dataset.path }),
                    }),
                    ["files"],
                );
                break;
            case "select-spec":
                await applySnapshotResponse(
                    api("/api/specs/select", {
                        method: "POST",
                        body: JSON.stringify({ path: target.dataset.path }),
                    }),
                    ["specs"],
                );
                break;
            case "run-workflow":
                await applySnapshotResponse(
                    api("/api/workflows/run", {
                        method: "POST",
                        body: JSON.stringify({ workflow_id: target.dataset.workflow }),
                    }),
                    ["workflows"],
                );
                break;
            case "select-channel":
                await applySnapshotResponse(
                    api("/api/channels/select", {
                        method: "POST",
                        body: JSON.stringify({ channel_id: target.dataset.channel }),
                    }),
                    ["channels"],
                );
                break;
            case "send-channel-message":
                if (!state.channelDraft.trim()) {
                    showToast("Type a channel message first.", "error");
                    break;
                }
                await applySnapshotResponse(
                    api("/api/channels/message", {
                        method: "POST",
                        body: JSON.stringify({
                            channel_id: target.dataset.channel,
                            content: state.channelDraft.trim(),
                        }),
                    }),
                    ["channels"],
                );
                state.channelDraft = "";
                break;
            case "new-conversation":
                state.pendingConversationId = uuid4();
                state.streaming = { conversationId: null, content: "" };
                state.chatDraft = "";
                render();
                break;
            case "start-agent":
                if (!state.agentGoal.trim()) {
                    showToast("Describe the agent goal first.", "error");
                    break;
                }
                await applySnapshotResponse(
                    api("/api/agents", {
                        method: "POST",
                        body: JSON.stringify({
                            goal: state.agentGoal.trim(),
                            orchestration_mode: state.agentMode,
                        }),
                    }),
                    ["agents", "observe"],
                );
                break;
            case "cancel-agent":
                await applySnapshotResponse(
                    api(`/api/agents/${target.dataset.run}/cancel`, {
                        method: "POST",
                    }),
                    ["agents", "observe"],
                );
                break;
            case "send-chat":
                await sendChatMessage();
                break;
            case "git-stage-all":
                await applySnapshotResponse(api("/api/git/stage-all", { method: "POST" }), ["git_ops"]);
                break;
            case "git-unstage-all":
                await applySnapshotResponse(api("/api/git/unstage-all", { method: "POST" }), ["git_ops"]);
                break;
            case "git-commit":
                if (!state.gitCommitMessage.trim()) {
                    showToast("Add a commit message first.", "error");
                    break;
                }
                await applySnapshotResponse(
                    api("/api/git/commit", {
                        method: "POST",
                        body: JSON.stringify({ message: state.gitCommitMessage.trim() }),
                    }),
                    ["git_ops"],
                );
                state.gitCommitMessage = "";
                break;
            case "terminal-start":
                await applySnapshotResponse(api("/api/terminal/start", { method: "POST" }), ["terminal"]);
                break;
            case "terminal-send":
                if (!state.terminalInput.trim()) {
                    showToast("Type a command first.", "error");
                    break;
                }
                await applySnapshotResponse(
                    api("/api/terminal/send", {
                        method: "POST",
                        body: JSON.stringify({
                            input: `${state.terminalInput}${state.terminalInput.endsWith("\n") ? "" : "\n"}`,
                        }),
                    }),
                    ["terminal"],
                );
                state.terminalInput = "";
                break;
            case "terminal-clear":
                await applySnapshotResponse(api("/api/terminal/clear", { method: "POST" }), ["terminal"]);
                break;
            case "terminal-kill":
                await applySnapshotResponse(api("/api/terminal/kill", { method: "POST" }), ["terminal"]);
                break;
            case "assistant-decision":
                await applySnapshotResponse(
                    api(`/api/assistant/approvals/${target.dataset.approval}/decision`, {
                        method: "POST",
                        body: JSON.stringify({ approved: target.dataset.approved === "true" }),
                    }),
                    ["assistant"],
                );
                break;
            case "toggle-setting":
                await applySnapshotResponse(
                    api("/api/settings/update", {
                        method: "POST",
                        body: JSON.stringify({
                            setting: target.dataset.setting,
                            value: target.dataset.value === "true",
                        }),
                    }),
                    ["settings", "models", "routing", "observe"],
                );
                break;
            case "save-text-setting": {
                const input = document.getElementById(target.dataset.input);
                const value = input ? input.value : "";
                await applySnapshotResponse(
                    api("/api/settings/text", {
                        method: "POST",
                        body: JSON.stringify({
                            setting: target.dataset.setting,
                            value,
                        }),
                    }),
                    ["settings", "launch", "models", "routing", "chat", "home", "observe"],
                );
                break;
            }
            case "save-provider-key": {
                const input = document.getElementById(target.dataset.input);
                const key = input ? input.value : "";
                await applySnapshotResponse(
                    api("/api/providers/key", {
                        method: "POST",
                        body: JSON.stringify({
                            provider: target.dataset.provider,
                            key,
                        }),
                    }),
                    ["models", "routing", "chat", "home", "observe"],
                );
                if (input) {
                    input.value = "";
                }
                break;
            }
            case "set-current-model":
                await applySnapshotResponse(
                    api("/api/navigation", {
                        method: "POST",
                        body: JSON.stringify({ model: target.dataset.model }),
                    }),
                    ["models", "routing", "chat", "home", "observe"],
                );
                break;
            case "set-default-model":
                await applySnapshotResponse(
                    api("/api/models/default", {
                        method: "POST",
                        body: JSON.stringify({ model: target.dataset.model }),
                    }),
                    ["models", "routing", "chat", "home", "observe"],
                );
                break;
            case "set-auto-routing":
                await applySnapshotResponse(
                    api("/api/routing/update", {
                        method: "POST",
                        body: JSON.stringify({ enabled: target.dataset.enabled === "true" }),
                    }),
                    ["routing", "models"],
                );
                break;
            case "add-project-model": {
                const input = document.getElementById(target.dataset.input);
                const model = input ? input.value : "";
                await applySnapshotResponse(
                    api("/api/routing/project-models/add", {
                        method: "POST",
                        body: JSON.stringify({ model }),
                    }),
                    ["routing", "models"],
                );
                if (input) {
                    input.value = "";
                }
                break;
            }
            case "remove-project-model":
                await applySnapshotResponse(
                    api("/api/routing/project-models/remove", {
                        method: "POST",
                        body: JSON.stringify({ model: target.dataset.model }),
                    }),
                    ["routing", "models"],
                );
                break;
            case "toggle-skill":
                await applySnapshotResponse(
                    api("/api/skills/toggle", {
                        method: "POST",
                        body: JSON.stringify({
                            name: target.dataset.skill,
                            enabled: target.dataset.enabled === "true",
                        }),
                    }),
                    ["skills"],
                );
                break;
            case "install-skill": {
                const name = document.getElementById("skill-create-name")?.value || "";
                const description =
                    document.getElementById("skill-create-description")?.value || "";
                const instructions =
                    document.getElementById("skill-create-instructions")?.value || "";
                await applySnapshotResponse(
                    api("/api/skills/install", {
                        method: "POST",
                        body: JSON.stringify({
                            name,
                            description,
                            instructions,
                        }),
                    }),
                    ["skills"],
                );
                break;
            }
            case "remove-skill":
                await applySnapshotResponse(
                    api("/api/skills/remove", {
                        method: "POST",
                        body: JSON.stringify({
                            name: target.dataset.skill,
                        }),
                    }),
                    ["skills"],
                );
                break;
            case "approve-request":
                await applySnapshotResponse(
                    api(`/api/approvals/${target.dataset.request}/decision`, {
                        method: "POST",
                        body: JSON.stringify({ approved: true }),
                    }),
                    ["chat", "observe", "home"],
                );
                break;
            case "deny-request": {
                const reason = window.prompt("Why should Hive deny this request?");
                await applySnapshotResponse(
                    api(`/api/approvals/${target.dataset.request}/decision`, {
                        method: "POST",
                        body: JSON.stringify({ approved: false, reason: reason || null }),
                    }),
                    ["chat", "observe", "home"],
                );
                break;
            }
            case "open-desktop":
                showToast("Open the desktop app on the paired machine for this surface.");
                break;
            default:
                break;
        }
    } catch (error) {
        showToast(error.message || "Action failed.", "error");
    }
});

document.addEventListener("input", (event) => {
    if (event.target.id === "home-detail-input") {
        state.homeDetail = event.target.value;
        autoResize(event.target);
    }
    if (event.target.id === "chat-input") {
        state.chatDraft = event.target.value;
        autoResize(event.target);
    }
    if (event.target.id === "channel-input") {
        state.channelDraft = event.target.value;
        autoResize(event.target);
    }
    if (event.target.id === "agent-goal-input") {
        state.agentGoal = event.target.value;
        autoResize(event.target);
    }
    if (event.target.id === "git-commit-message") {
        state.gitCommitMessage = event.target.value;
        autoResize(event.target);
    }
    if (event.target.id === "terminal-input") {
        state.terminalInput = event.target.value;
        autoResize(event.target);
    }
});

document.addEventListener("change", async (event) => {
    if (event.target.id === "agent-mode-select") {
        state.agentMode = event.target.value;
        return;
    }
    if (event.target.id !== "chat-model-select") return;
    try {
        await applySnapshotResponse(
            api("/api/navigation", {
                method: "POST",
                body: JSON.stringify({ model: event.target.value }),
            }),
            ["chat", "home", "observe"],
        );
    } catch (error) {
        showToast(error.message, "error");
    }
});

document.addEventListener("keydown", async (event) => {
    if (event.target.id === "chat-input" && event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        await sendChatMessage();
    }
    if (event.target.id === "channel-input" && event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        document.querySelector('[data-action="send-channel-message"]')?.click();
    }
    if (event.target.id === "terminal-input" && event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        document.querySelector('[data-action="terminal-send"]')?.click();
    }
});

window.addEventListener("resize", () => {
    if (!isDesktopLayout()) {
        state.utilityOpen = false;
    }
});

boot();
