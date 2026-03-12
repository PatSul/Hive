// Hive Remote — Vanilla JS SPA
// Connects to HiveDaemon via WebSocket, provides chat + agent monitor.
// No dependencies, no build step. Embeddable via include_str!().

"use strict";

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/** Minimal HTML-escape to prevent XSS when rendering user/AI content. */
function esc(str) {
    if (!str) return "";
    return str
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

/** Render markdown-like formatting: bold, italic, inline code, code blocks. */
function renderMarkdown(text) {
    if (!text) return "";
    var s = esc(text);

    // Code blocks: ```...```
    s = s.replace(/```(\w*)\n?([\s\S]*?)```/g, function (_m, _lang, code) {
        return "<pre><code>" + code.trim() + "</code></pre>";
    });

    // Inline code: `...`
    s = s.replace(/`([^`\n]+)`/g, "<code>$1</code>");

    // Bold: **...**
    s = s.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");

    // Italic: *...*
    s = s.replace(/\*(.+?)\*/g, "<em>$1</em>");

    // Line breaks
    s = s.replace(/\n/g, "<br>");

    return s;
}

/** Format milliseconds to human-readable duration. */
function formatDuration(ms) {
    if (ms < 1000) return ms + "ms";
    var s = Math.floor(ms / 1000);
    if (s < 60) return s + "s";
    var m = Math.floor(s / 60);
    s = s % 60;
    return m + "m " + s + "s";
}

/** Format a cost in USD. */
function formatCost(usd) {
    if (usd == null || usd === 0) return "$0.00";
    if (usd < 0.01) return "<$0.01";
    return "$" + usd.toFixed(2);
}

/** Generate a simple UUID v4 (good enough for conversation IDs). */
function uuid4() {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function (c) {
        var r = (Math.random() * 16) | 0;
        var v = c === "x" ? r : (r & 0x3) | 0x8;
        return v.toString(16);
    });
}

/** Timestamp string in HH:MM format. */
function timeStr(dateOrIso) {
    var d = dateOrIso instanceof Date ? dateOrIso : new Date(dateOrIso || Date.now());
    var h = d.getHours().toString().padStart(2, "0");
    var m = d.getMinutes().toString().padStart(2, "0");
    return h + ":" + m;
}

// ---------------------------------------------------------------------------
// Toast notification system
// ---------------------------------------------------------------------------

var toastContainer = null;

function showToast(msg, type) {
    if (!toastContainer) {
        toastContainer = document.createElement("div");
        toastContainer.className = "toast-container";
        document.body.appendChild(toastContainer);
    }
    var toast = document.createElement("div");
    toast.className = "toast" + (type === "error" ? " error" : "");
    toast.textContent = msg;
    toastContainer.appendChild(toast);
    setTimeout(function () {
        toast.style.opacity = "0";
        setTimeout(function () {
            if (toast.parentNode) toast.parentNode.removeChild(toast);
        }, 200);
    }, 4000);
}

// ---------------------------------------------------------------------------
// HiveConnection — WebSocket with auto-reconnect
// ---------------------------------------------------------------------------

function HiveConnection() {
    this.ws = null;
    this.listeners = [];
    this.connected = false;
    this.reconnectDelay = 1000;
    this.maxReconnectDelay = 30000;
    this.reconnectTimer = null;
    this.pingTimer = null;
}

HiveConnection.prototype.connect = function () {
    var self = this;
    var proto = location.protocol === "https:" ? "wss:" : "ws:";
    var url = proto + "//" + location.host + "/ws";

    try {
        this.ws = new WebSocket(url);
    } catch (e) {
        this._scheduleReconnect();
        return;
    }

    this.ws.onopen = function () {
        self.connected = true;
        self.reconnectDelay = 1000;
        self._notifyConnection(true);
        self._startPing();
    };

    this.ws.onclose = function () {
        self.connected = false;
        self._notifyConnection(false);
        self._stopPing();
        self._scheduleReconnect();
    };

    this.ws.onerror = function () {
        // onclose will fire after onerror
    };

    this.ws.onmessage = function (e) {
        try {
            var event = JSON.parse(e.data);
            self._dispatch(event);
        } catch (err) {
            console.error("Failed to parse WS message:", err);
        }
    };
};

HiveConnection.prototype.send = function (event) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify(event));
    }
};

HiveConnection.prototype.onEvent = function (callback) {
    this.listeners.push(callback);
};

HiveConnection.prototype._dispatch = function (event) {
    for (var i = 0; i < this.listeners.length; i++) {
        try {
            this.listeners[i](event);
        } catch (err) {
            console.error("Event listener error:", err);
        }
    }
};

HiveConnection.prototype._notifyConnection = function (connected) {
    this._dispatch({ type: "_connection", connected: connected });
};

HiveConnection.prototype._scheduleReconnect = function () {
    var self = this;
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(function () {
        self.reconnectTimer = null;
        self.connect();
    }, this.reconnectDelay);
    // Exponential backoff
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay);
};

HiveConnection.prototype._startPing = function () {
    var self = this;
    this._stopPing();
    this.pingTimer = setInterval(function () {
        self.send({ type: "ping" });
    }, 25000);
};

HiveConnection.prototype._stopPing = function () {
    if (this.pingTimer) {
        clearInterval(this.pingTimer);
        this.pingTimer = null;
    }
};

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------

var state = {
    connected: false,
    activePanel: "chat",
    conversationId: null,
    messages: [],
    streamingContent: "",
    isStreaming: false,
    agentRuns: [],
    selectedModel: "auto",
    showAgentModal: false,
    timestamp: null,
};

// Available models for the selector
var models = [
    { id: "auto", label: "Auto" },
    { id: "gpt-4o", label: "GPT-4o" },
    { id: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
    { id: "claude-opus-4-20250514", label: "Claude Opus 4" },
    { id: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
    { id: "deepseek-chat", label: "DeepSeek V3" },
];

// Navigation panels
var panels = [
    { id: "chat", icon: "\u{1F4AC}", label: "Chat" },
    { id: "agents", icon: "\u{1F916}", label: "Agents" },
    { id: "files", icon: "\u{1F4C1}", label: "Files" },
    { id: "terminal", icon: "\u{1F5A5}\uFE0F", label: "Terminal" },
    { id: "settings", icon: "\u2699\uFE0F", label: "Settings" },
];

// ---------------------------------------------------------------------------
// Connection + Event handling
// ---------------------------------------------------------------------------

var conn = new HiveConnection();

conn.onEvent(function (event) {
    switch (event.type) {
        case "_connection":
            state.connected = event.connected;
            if (event.connected) {
                showToast("Connected to Hive");
            }
            render();
            break;

        case "state_snapshot":
            // Full state from server
            if (event.active_panel) state.activePanel = event.active_panel;
            if (event.active_conversation) state.conversationId = event.active_conversation;
            if (event.agent_runs) state.agentRuns = event.agent_runs;
            if (event.timestamp) state.timestamp = event.timestamp;
            render();
            break;

        case "stream_chunk":
            state.isStreaming = true;
            state.streamingContent += event.chunk;
            renderChatStreaming();
            break;

        case "stream_complete":
            if (state.isStreaming && state.streamingContent) {
                state.messages.push({
                    role: "assistant",
                    content: state.streamingContent,
                    time: new Date(),
                    model: state.selectedModel,
                    tokens: (event.prompt_tokens || 0) + (event.completion_tokens || 0),
                    cost: event.cost_usd,
                });
            }
            state.isStreaming = false;
            state.streamingContent = "";
            render();
            break;

        case "agent_status":
            updateAgentStatus(event);
            render();
            break;

        case "panel_data":
            // Future: handle panel-specific data
            break;

        case "error":
            showToast("Error: " + (event.message || "Unknown error"), "error");
            break;

        case "pong":
            // Heartbeat response
            break;
    }
});

function updateAgentStatus(event) {
    var found = false;
    for (var i = 0; i < state.agentRuns.length; i++) {
        if (state.agentRuns[i].run_id === event.run_id) {
            state.agentRuns[i].status = event.status;
            state.agentRuns[i].detail = event.detail;
            found = true;
            break;
        }
    }
    if (!found) {
        state.agentRuns.push({
            run_id: event.run_id,
            goal: event.detail || "Agent task",
            status: event.status,
            detail: event.detail,
            cost_usd: 0,
            elapsed_ms: 0,
        });
    }
}

// ---------------------------------------------------------------------------
// User actions
// ---------------------------------------------------------------------------

function sendMessage() {
    var input = document.getElementById("chat-input");
    if (!input) return;
    var content = input.value.trim();
    if (!content) return;

    if (!state.conversationId) {
        state.conversationId = uuid4();
    }

    // Add user message to local state
    state.messages.push({
        role: "user",
        content: content,
        time: new Date(),
    });

    // Send to daemon
    conn.send({
        type: "send_message",
        conversation_id: state.conversationId,
        content: content,
        model: state.selectedModel,
    });

    input.value = "";
    autoResizeTextarea(input);
    render();

    // Scroll to bottom after render
    setTimeout(scrollChatToBottom, 50);
}

function switchPanel(panelId) {
    state.activePanel = panelId;
    conn.send({ type: "switch_panel", panel: panelId });
    render();
}

function cancelAgent(runId) {
    conn.send({ type: "cancel_agent_task", run_id: runId });
}

function startAgentTask(goal, mode) {
    conn.send({
        type: "start_agent_task",
        goal: goal,
        orchestration_mode: mode || "auto",
    });
    state.showAgentModal = false;
    render();
}

function scrollChatToBottom() {
    var el = document.getElementById("chat-messages");
    if (el) el.scrollTop = el.scrollHeight;
}

function autoResizeTextarea(el) {
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 150) + "px";
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function render() {
    var app = document.getElementById("app");
    if (!app) return;

    var html = "";
    html += renderHeader();
    html += '<div class="app-layout">';
    html += renderSidebar();
    html += '<div class="main-content">';
    html += renderPanel();
    html += "</div>";
    html += "</div>";

    app.innerHTML = html;
    bindEvents();
}

function renderHeader() {
    var dotClass = state.connected ? "connection-dot connected" : "connection-dot";
    var statusText = state.connected ? "Connected" : "Disconnected";

    return (
        '<div class="header">' +
        '<div class="header-logo">HIVE<span>remote</span></div>' +
        '<div class="connection-badge">' +
        '<div class="' + dotClass + '"></div>' +
        "<span>" + statusText + "</span>" +
        "</div>" +
        "</div>"
    );
}

function renderSidebar() {
    var html = '<div class="sidebar"><nav class="sidebar-nav">';
    for (var i = 0; i < panels.length; i++) {
        var p = panels[i];
        var active = p.id === state.activePanel ? " active" : "";
        html +=
            '<button class="nav-item' + active + '" data-panel="' + p.id + '">' +
            '<span class="nav-icon">' + p.icon + "</span>" +
            '<span class="nav-label">' + p.label + "</span>" +
            "</button>";
    }
    html += "</nav>";
    html += '<div class="sidebar-footer">Hive Remote v0.2</div>';
    html += "</div>";
    return html;
}

function renderPanel() {
    switch (state.activePanel) {
        case "chat":
            return renderChat();
        case "agents":
            return renderAgents();
        case "terminal":
            return renderTerminal();
        case "files":
            return renderFiles();
        default:
            return renderPlaceholder(state.activePanel);
    }
}

// ---------------------------------------------------------------------------
// Chat Panel
// ---------------------------------------------------------------------------

function renderChat() {
    var html = '<div class="chat-panel">';

    // Messages area
    html += '<div class="chat-messages" id="chat-messages">';

    if (state.messages.length === 0 && !state.isStreaming) {
        html +=
            '<div class="chat-empty">' +
            '<div class="chat-empty-icon">\u{1F41D}</div>' +
            "<p>No messages yet. Start a conversation!</p>" +
            "</div>";
    } else {
        for (var i = 0; i < state.messages.length; i++) {
            html += renderMessage(state.messages[i]);
        }

        // Show streaming message
        if (state.isStreaming && state.streamingContent) {
            html +=
                '<div class="message assistant">' +
                '<div class="message-bubble">' +
                renderMarkdown(state.streamingContent) +
                '<span class="streaming-indicator"></span>' +
                "</div>" +
                "</div>";
        }
    }

    html += "</div>";

    // Input area
    html += renderChatInput();
    html += "</div>";
    return html;
}

function renderMessage(msg) {
    var roleClass = msg.role === "user" ? "user" : "assistant";
    var html =
        '<div class="message ' + roleClass + '">' +
        '<div class="message-bubble">' +
        renderMarkdown(msg.content) +
        "</div>" +
        '<div class="message-meta">';

    if (msg.model && msg.role === "assistant") {
        html += '<span class="model-badge">' + esc(msg.model) + "</span>";
    }

    if (msg.time) {
        html += "<span>" + timeStr(msg.time) + "</span>";
    }

    if (msg.cost != null && msg.cost > 0) {
        html += "<span>" + formatCost(msg.cost) + "</span>";
    }

    html += "</div></div>";
    return html;
}

function renderChatInput() {
    var disabled = !state.connected ? " disabled" : "";

    var html =
        '<div class="chat-input-area">' +
        '<div class="chat-input-row">' +
        '<textarea class="chat-input" id="chat-input" placeholder="Type a message..."' +
        disabled +
        ' rows="1"></textarea>' +
        '<button class="chat-send-btn" id="chat-send-btn"' + disabled + ">Send</button>" +
        "</div>" +
        '<div class="chat-input-options">' +
        '<select class="model-select" id="model-select">';

    for (var i = 0; i < models.length; i++) {
        var sel = models[i].id === state.selectedModel ? " selected" : "";
        html += '<option value="' + models[i].id + '"' + sel + ">" + esc(models[i].label) + "</option>";
    }

    html += "</select></div></div>";
    return html;
}

/** Partial re-render for streaming — only updates the message area without resetting input. */
function renderChatStreaming() {
    var el = document.getElementById("chat-messages");
    if (!el) return;

    // Find or create the streaming bubble
    var streamBubble = el.querySelector(".message.assistant:last-child .streaming-indicator");
    if (streamBubble) {
        // Update existing streaming message
        var bubble = streamBubble.parentNode;
        bubble.innerHTML =
            renderMarkdown(state.streamingContent) +
            '<span class="streaming-indicator"></span>';
    } else {
        // Append new streaming message
        var div = document.createElement("div");
        div.className = "message assistant";
        div.innerHTML =
            '<div class="message-bubble">' +
            renderMarkdown(state.streamingContent) +
            '<span class="streaming-indicator"></span>' +
            "</div>";
        el.appendChild(div);
    }

    scrollChatToBottom();
}

// ---------------------------------------------------------------------------
// Agents Panel
// ---------------------------------------------------------------------------

function renderAgents() {
    var html =
        '<div class="agents-panel">' +
        '<div class="agents-header">' +
        "<h2>Agent Monitor</h2>" +
        '<button class="agent-start-btn" id="agent-start-btn">+ New Task</button>' +
        "</div>";

    if (state.agentRuns.length === 0) {
        html +=
            '<div class="agents-empty">' +
            '<div class="agents-empty-icon">\u{1F916}</div>' +
            "<p>No agent tasks running.</p>" +
            "<p>Start a new task to see it here.</p>" +
            "</div>";
    } else {
        html += '<div class="agent-cards">';
        for (var i = 0; i < state.agentRuns.length; i++) {
            html += renderAgentCard(state.agentRuns[i]);
        }
        html += "</div>";
    }

    html += "</div>";

    // Modal
    if (state.showAgentModal) {
        html += renderAgentModal();
    }

    return html;
}

function renderAgentCard(run) {
    var statusClass = run.status || "planning";

    var html =
        '<div class="agent-card">' +
        '<div class="agent-card-header">' +
        '<span class="agent-goal" title="' + esc(run.goal) + '">' + esc(run.goal) + "</span>" +
        '<span class="agent-status-badge ' + esc(statusClass) + '">' + esc(run.status) + "</span>" +
        "</div>";

    if (run.detail) {
        html += '<div class="agent-detail">' + esc(run.detail) + "</div>";
    }

    html +=
        '<div class="agent-metrics">' +
        '<div class="agent-metric">Cost: <span class="agent-metric-value">' +
        formatCost(run.cost_usd) +
        "</span></div>" +
        '<div class="agent-metric">Time: <span class="agent-metric-value">' +
        formatDuration(run.elapsed_ms || 0) +
        "</span></div>" +
        "</div>";

    // Cancel button for active tasks
    if (run.status === "planning" || run.status === "running") {
        html +=
            '<button class="agent-cancel-btn" data-run-id="' +
            esc(run.run_id) +
            '">Cancel</button>';
    }

    html += "</div>";
    return html;
}

function renderAgentModal() {
    return (
        '<div class="modal-overlay" id="agent-modal-overlay">' +
        '<div class="modal">' +
        "<h3>Start Agent Task</h3>" +
        '<div class="modal-field">' +
        "<label>Goal</label>" +
        '<textarea id="agent-goal-input" placeholder="Describe what the agent should accomplish..." rows="3"></textarea>' +
        "</div>" +
        '<div class="modal-field">' +
        "<label>Orchestration Mode</label>" +
        '<select id="agent-mode-select">' +
        '<option value="auto">Auto</option>' +
        '<option value="coordinator">Coordinator</option>' +
        '<option value="hivemind">HiveMind</option>' +
        "</select>" +
        "</div>" +
        '<div class="modal-actions">' +
        '<button class="modal-btn" id="agent-modal-cancel">Cancel</button>' +
        '<button class="modal-btn primary" id="agent-modal-submit">Start</button>' +
        "</div>" +
        "</div>" +
        "</div>"
    );
}

// ---------------------------------------------------------------------------
// Terminal Panel
// ---------------------------------------------------------------------------

if (!state.terminalLines) state.terminalLines = [];
if (!state.terminalRunning) state.terminalRunning = false;

function renderTerminal() {
    var html = '<div class="terminal-panel">';
    html += '<div class="terminal-header">';
    html += '<span class="terminal-title">\u{1F5A5}\uFE0F Terminal</span>';
    html += '<span class="terminal-status ' + (state.terminalRunning ? "running" : "") + '">' +
        (state.terminalRunning ? "Running" : "Idle") + '</span>';
    html += '</div>';
    html += '<div class="terminal-output" id="terminal-output">';
    for (var i = 0; i < state.terminalLines.length; i++) {
        var line = state.terminalLines[i];
        var cls = "term-line";
        if (line.kind === "stderr") cls += " term-stderr";
        else if (line.kind === "stdin") cls += " term-stdin";
        else if (line.kind === "system") cls += " term-system";
        var prefix = line.kind === "stdin" ? "$ " : line.kind === "system" ? "# " : "";
        html += '<div class="' + cls + '">' + esc(prefix + line.content) + '</div>';
    }
    html += '</div>';
    html += '<div class="terminal-input-bar">';
    html += '<span class="term-prompt">$</span>';
    html += '<input type="text" class="terminal-input" id="terminal-input" ' +
        'placeholder="Type a command and press Enter..." />';
    html += '</div>';
    html += '</div>';
    return html;
}

// ---------------------------------------------------------------------------
// Files Panel
// ---------------------------------------------------------------------------

function renderFiles() {
    var html = '<div class="files-panel">';
    html += '<div class="status-panel">';
    html += '<div class="status-icon">\u{1F4C1}</div>';
    html += '<p>File browser is available in the desktop app.</p>';
    html += '<p>The desktop app includes a built-in code viewer with syntax highlighting.</p>';
    html += '</div>';
    html += '</div>';
    return html;
}

// ---------------------------------------------------------------------------
// Placeholder Panels
// ---------------------------------------------------------------------------

function renderPlaceholder(panelId) {
    var name = panelId.charAt(0).toUpperCase() + panelId.slice(1);
    return (
        '<div class="status-panel">' +
        '<div class="status-icon">\u{1F6A7}</div>' +
        "<p>" + esc(name) + " panel coming soon.</p>" +
        "<p>Use the desktop app for full access.</p>" +
        "</div>"
    );
}

// ---------------------------------------------------------------------------
// Event binding (after each render)
// ---------------------------------------------------------------------------

function bindEvents() {
    // Navigation
    var navItems = document.querySelectorAll(".nav-item[data-panel]");
    for (var i = 0; i < navItems.length; i++) {
        navItems[i].addEventListener("click", function (e) {
            var panel = this.getAttribute("data-panel");
            if (panel) switchPanel(panel);
        });
    }

    // Chat input
    var chatInput = document.getElementById("chat-input");
    if (chatInput) {
        chatInput.addEventListener("keydown", function (e) {
            if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                sendMessage();
            }
        });
        chatInput.addEventListener("input", function () {
            autoResizeTextarea(this);
        });
        // Auto-focus on chat panel
        if (state.activePanel === "chat") {
            chatInput.focus();
        }
    }

    // Send button
    var sendBtn = document.getElementById("chat-send-btn");
    if (sendBtn) {
        sendBtn.addEventListener("click", sendMessage);
    }

    // Model selector
    var modelSelect = document.getElementById("model-select");
    if (modelSelect) {
        modelSelect.addEventListener("change", function () {
            state.selectedModel = this.value;
        });
    }

    // Agent cancel buttons
    var cancelBtns = document.querySelectorAll(".agent-cancel-btn[data-run-id]");
    for (var j = 0; j < cancelBtns.length; j++) {
        cancelBtns[j].addEventListener("click", function () {
            var runId = this.getAttribute("data-run-id");
            if (runId) cancelAgent(runId);
        });
    }

    // Agent start button
    var startBtn = document.getElementById("agent-start-btn");
    if (startBtn) {
        startBtn.addEventListener("click", function () {
            state.showAgentModal = true;
            render();
        });
    }

    // Agent modal
    var modalOverlay = document.getElementById("agent-modal-overlay");
    if (modalOverlay) {
        modalOverlay.addEventListener("click", function (e) {
            if (e.target === modalOverlay) {
                state.showAgentModal = false;
                render();
            }
        });
    }

    var modalCancel = document.getElementById("agent-modal-cancel");
    if (modalCancel) {
        modalCancel.addEventListener("click", function () {
            state.showAgentModal = false;
            render();
        });
    }

    var modalSubmit = document.getElementById("agent-modal-submit");
    if (modalSubmit) {
        modalSubmit.addEventListener("click", function () {
            var goalInput = document.getElementById("agent-goal-input");
            var modeSelect = document.getElementById("agent-mode-select");
            var goal = goalInput ? goalInput.value.trim() : "";
            var mode = modeSelect ? modeSelect.value : "auto";
            if (goal) {
                startAgentTask(goal, mode);
            }
        });
    }

    // Terminal input
    var termInput = document.getElementById("terminal-input");
    if (termInput) {
        termInput.addEventListener("keydown", function (e) {
            if (e.key === "Enter") {
                e.preventDefault();
                var cmd = this.value.trim();
                if (cmd) {
                    state.terminalLines.push({ kind: "stdin", content: cmd });
                    this.value = "";
                    // Send to daemon via WebSocket if connected
                    if (conn && conn.ws && conn.ws.readyState === WebSocket.OPEN) {
                        conn.send({ type: "TerminalInput", command: cmd });
                    }
                    render();
                    // Auto-scroll terminal output
                    var termOut = document.getElementById("terminal-output");
                    if (termOut) termOut.scrollTop = termOut.scrollHeight;
                }
            }
        });
        if (state.activePanel === "terminal") {
            termInput.focus();
        }
    }

    // Scroll chat to bottom on fresh render
    if (state.activePanel === "chat") {
        scrollChatToBottom();
    }
}

// ---------------------------------------------------------------------------
// Initialize
// ---------------------------------------------------------------------------

(function init() {
    render();
    conn.connect();
})();
