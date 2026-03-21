//! Visual Workflow Builder — drag-and-drop node canvas for wiring agents,
//! steps, and conditions into executable automation workflows.

use gpui::prelude::FluentBuilder;
use gpui::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use tracing::{error, info};

use hive_agents::automation::{
    ActionType, Condition, TriggerType, Workflow, WorkflowStatus, WorkflowStep,
};
use hive_agents::personas::PersonaKind;
use hive_ui_core::{AppTheme, HiveTheme};

// ---------------------------------------------------------------------------
// Canvas data model
// ---------------------------------------------------------------------------

/// The kind of node on the workflow canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Starting point — defines the trigger that kicks off the workflow.
    Trigger,
    /// A concrete action step (run command, call API, send notification, etc.).
    Action,
    /// A conditional branch — routes execution based on a condition.
    Condition,
    /// Terminal output node — marks the end of a branch.
    Output,
}

/// A visual node on the workflow canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub action: Option<ActionType>,
    pub trigger: Option<TriggerType>,
    pub conditions: Vec<Condition>,
    pub persona: Option<PersonaKind>,
    pub timeout_secs: Option<u64>,
    pub retry_count: u32,
}

impl CanvasNode {
    pub fn new_trigger(x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Trigger,
            label: "Trigger".into(),
            x,
            y,
            width: 160.0,
            height: 60.0,
            action: None,
            trigger: Some(TriggerType::ManualTrigger),
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_action(label: &str, action: ActionType, x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Action,
            label: label.into(),
            x,
            y,
            width: 180.0,
            height: 70.0,
            action: Some(action),
            trigger: None,
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_condition(label: &str, conditions: Vec<Condition>, x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Condition,
            label: label.into(),
            x,
            y,
            width: 160.0,
            height: 70.0,
            action: None,
            trigger: None,
            conditions,
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }

    pub fn new_output(x: f64, y: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: NodeKind::Output,
            label: "End".into(),
            x,
            y,
            width: 120.0,
            height: 50.0,
            action: None,
            trigger: None,
            conditions: Vec::new(),
            persona: None,
            timeout_secs: None,
            retry_count: 0,
        }
    }
}

/// A port on a node where edges can connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Port {
    Output,
    TrueOutput,
    FalseOutput,
    Input,
}

/// A directed edge between two ports on two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub from_node_id: String,
    pub from_port: Port,
    pub to_node_id: String,
    pub to_port: Port,
    pub label: Option<String>,
}

/// Human-readable label for a `NodeKind`.
fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Trigger => "Trigger",
        NodeKind::Action => "Action",
        NodeKind::Condition => "Condition",
        NodeKind::Output => "Output",
    }
}

/// Human-readable label for an `ActionType` (variant name only).
fn action_type_label(action: &ActionType) -> &'static str {
    match action {
        ActionType::RunCommand { .. } => "Run Command",
        ActionType::SendMessage { .. } => "Send Message",
        ActionType::CallApi { .. } => "Call API",
        ActionType::CreateTask { .. } => "Create Task",
        ActionType::SendNotification { .. } => "Send Notification",
        ActionType::ExecuteSkill { .. } => "Execute Skill",
    }
}

/// Human-readable label for a `PersonaKind`.
fn persona_kind_label(persona: &PersonaKind) -> String {
    match persona {
        PersonaKind::Investigate => "Investigate".into(),
        PersonaKind::Implement => "Implement".into(),
        PersonaKind::Verify => "Verify".into(),
        PersonaKind::Critique => "Critique".into(),
        PersonaKind::Debug => "Debug".into(),
        PersonaKind::CodeReview => "Code Review".into(),
        PersonaKind::Custom(name) => name.clone(),
    }
}

/// Strip path separators and traversal sequences from a workflow ID so it
/// cannot escape its target directory when used in a filename.
fn sanitize_workflow_id(id: &str) -> String {
    id.replace(['/', '\\', ':', '\0'], "_")
        .replace("..", "_")
}

/// Full serialisable state of the workflow canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCanvasState {
    pub workflow_id: String,
    pub name: String,
    pub description: String,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub canvas_offset_x: f64,
    pub canvas_offset_y: f64,
    pub zoom: f64,
}

impl WorkflowCanvasState {
    pub fn empty(name: &str) -> Self {
        Self {
            workflow_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            nodes: vec![CanvasNode::new_trigger(100.0, 200.0)],
            edges: Vec::new(),
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            zoom: 1.0,
        }
    }

    /// Save this canvas state to ~/.hive/workflows/{workflow_id}.canvas.json
    pub fn save_to_disk(&self) -> anyhow::Result<()> {
        let dir = hive_core::config::HiveConfig::base_dir()?.join("workflows");
        std::fs::create_dir_all(&dir)?;
        let safe_id = sanitize_workflow_id(&self.workflow_id);
        let path = dir.join(format!("{safe_id}.canvas.json"));
        let tmp_path = dir.join(format!("{safe_id}.canvas.json.tmp"));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Load a canvas state from disk by workflow_id.
    pub fn load_from_disk(workflow_id: &str) -> anyhow::Result<Self> {
        let dir = hive_core::config::HiveConfig::base_dir()?.join("workflows");
        let safe_id = sanitize_workflow_id(workflow_id);
        let path = dir.join(format!("{safe_id}.canvas.json"));
        let json = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&json)?;
        Ok(state)
    }

    /// List all saved canvas workflow IDs on disk.
    pub fn list_saved() -> Vec<String> {
        let dir = match hive_core::config::HiveConfig::base_dir() {
            Ok(d) => d.join("workflows"),
            Err(_) => return Vec::new(),
        };
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(id) = name.strip_suffix(".canvas.json") {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    /// Build a simple linear canvas from an executable workflow.
    ///
    /// This is used when a workflow exists in automation storage but there is
    /// no saved visual canvas yet, so the builder can still open it.
    pub fn from_workflow(workflow: &Workflow, canvas_workflow_id: impl Into<String>) -> Self {
        let canvas_workflow_id = canvas_workflow_id.into();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut trigger_node = CanvasNode::new_trigger(100.0, 220.0);
        trigger_node.label = match &workflow.trigger {
            TriggerType::Schedule { .. } => "Schedule".into(),
            TriggerType::FileChange { .. } => "File Change".into(),
            TriggerType::WebhookReceived { .. } => "Webhook".into(),
            TriggerType::ManualTrigger => "Manual Trigger".into(),
            TriggerType::OnMessage { .. } => "On Message".into(),
            TriggerType::OnError { .. } => "On Error".into(),
        };
        trigger_node.trigger = Some(workflow.trigger.clone());
        let mut previous_node_id = trigger_node.id.clone();
        nodes.push(trigger_node);

        let mut x = 360.0;
        for step in &workflow.steps {
            let mut node = CanvasNode::new_action(&step.name, step.action.clone(), x, 220.0);
            node.conditions = step.conditions.clone();
            node.timeout_secs = step.timeout_secs;
            node.retry_count = step.retry_count;

            edges.push(CanvasEdge {
                id: uuid::Uuid::new_v4().to_string(),
                from_node_id: previous_node_id.clone(),
                from_port: Port::Output,
                to_node_id: node.id.clone(),
                to_port: Port::Input,
                label: None,
            });

            previous_node_id = node.id.clone();
            nodes.push(node);
            x += 240.0;
        }

        let output_node = CanvasNode::new_output(x, 220.0);
        edges.push(CanvasEdge {
            id: uuid::Uuid::new_v4().to_string(),
            from_node_id: previous_node_id,
            from_port: Port::Output,
            to_node_id: output_node.id.clone(),
            to_port: Port::Input,
            label: None,
        });
        nodes.push(output_node);

        Self {
            workflow_id: canvas_workflow_id,
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            nodes,
            edges,
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            zoom: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when a workflow is saved from the builder.
#[derive(Debug, Clone)]
pub struct WorkflowSaved(pub String);

/// Emitted when the user wants to run the current workflow.
#[derive(Debug, Clone)]
pub struct WorkflowRunRequested(pub String);

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// Workflow list entry for the left sidebar.
#[derive(Debug, Clone)]
pub struct WorkflowListEntry {
    pub id: String,
    pub name: String,
    pub is_builtin: bool,
    pub status: String,
}

struct DragState {
    node_id: String,
    /// Mouse position at start of drag.
    start_x: f64,
    start_y: f64,
    /// Node position at start of drag.
    node_start_x: f64,
    node_start_y: f64,
}

/// State for panning the canvas background.
struct PanState {
    start_mouse_x: f64,
    start_mouse_y: f64,
    start_offset_x: f64,
    start_offset_y: f64,
}

pub struct WorkflowBuilderView {
    theme: HiveTheme,

    // Canvas state
    canvas: WorkflowCanvasState,

    // Interaction
    selected_node_id: Option<String>,
    dragging_node: Option<DragState>,
    connecting_from: Option<(String, Port)>,
    connection_preview_pos: Option<(f64, f64)>,
    panning: Option<PanState>,

    // Viewport
    canvas_offset: (f64, f64),
    zoom: f64,

    // UI panels
    show_node_palette: bool,
    show_properties_panel: bool,

    // Workflow list
    workflow_list: Vec<WorkflowListEntry>,
    active_workflow_id: Option<String>,

    // Dirty flag
    is_dirty: bool,
}

impl EventEmitter<WorkflowSaved> for WorkflowBuilderView {}
impl EventEmitter<WorkflowRunRequested> for WorkflowBuilderView {}

impl WorkflowBuilderView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let theme = if _cx.has_global::<AppTheme>() {
            _cx.global::<AppTheme>().0.clone()
        } else {
            HiveTheme::dark()
        };

        Self {
            theme,
            canvas: WorkflowCanvasState::empty("New Workflow"),
            selected_node_id: None,
            dragging_node: None,
            connecting_from: None,
            connection_preview_pos: None,
            panning: None,
            canvas_offset: (0.0, 0.0),
            zoom: 1.0,
            show_node_palette: true,
            show_properties_panel: false,
            workflow_list: Vec::new(),
            active_workflow_id: None,
            is_dirty: false,
        }
    }

    /// Replace the cached theme and trigger a re-render.
    pub fn set_theme(&mut self, theme: HiveTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Refresh the workflow list from the automation service.
    pub fn refresh_workflow_list(
        &mut self,
        workflows: Vec<WorkflowListEntry>,
        cx: &mut Context<Self>,
    ) {
        self.workflow_list = workflows;
        cx.notify();
    }

    /// Load a workflow canvas state.
    pub fn load_canvas(
        &mut self,
        canvas: WorkflowCanvasState,
        workflow_entry_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.canvas_offset = (canvas.canvas_offset_x, canvas.canvas_offset_y);
        self.zoom = canvas.zoom.clamp(0.3, 3.0);
        self.canvas = canvas;
        self.active_workflow_id = workflow_entry_id;
        self.selected_node_id = None;
        self.connecting_from = None;
        self.connection_preview_pos = None;
        self.dragging_node = None;
        self.panning = None;
        self.is_dirty = false;
        cx.notify();
    }

    /// Add a node to the canvas.
    pub fn add_node(&mut self, node: CanvasNode, cx: &mut Context<Self>) {
        self.canvas.nodes.push(node);
        self.is_dirty = true;
        cx.notify();
    }

    /// Remove a node and its connected edges.
    pub fn delete_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        self.canvas.nodes.retain(|n| n.id != node_id);
        self.canvas
            .edges
            .retain(|e| e.from_node_id != node_id && e.to_node_id != node_id);
        if self.selected_node_id.as_deref() == Some(node_id) {
            self.selected_node_id = None;
        }
        self.is_dirty = true;
        cx.notify();
    }

    /// Connect two nodes via an edge. Silently ignores duplicate connections.
    pub fn connect_nodes(
        &mut self,
        from_id: &str,
        from_port: Port,
        to_id: &str,
        to_port: Port,
        cx: &mut Context<Self>,
    ) {
        let already_exists = self.canvas.edges.iter().any(|e| {
            e.from_node_id == from_id
                && e.from_port == from_port
                && e.to_node_id == to_id
                && e.to_port == to_port
        });
        if already_exists {
            return;
        }
        let edge = CanvasEdge {
            id: uuid::Uuid::new_v4().to_string(),
            from_node_id: from_id.into(),
            from_port,
            to_node_id: to_id.into(),
            to_port,
            label: None,
        };
        self.canvas.edges.push(edge);
        self.is_dirty = true;
        cx.notify();
    }

    // -- Drag/pan/connect interaction handlers --------------------------------

    /// Start dragging a node.
    fn start_drag(&mut self, node_id: &str, mouse_x: f64, mouse_y: f64) {
        if let Some(node) = self.canvas.nodes.iter().find(|n| n.id == node_id) {
            self.dragging_node = Some(DragState {
                node_id: node_id.to_string(),
                start_x: mouse_x,
                start_y: mouse_y,
                node_start_x: node.x,
                node_start_y: node.y,
            });
        }
    }

    /// Update dragged node position based on mouse movement.
    fn update_drag(&mut self, mouse_x: f64, mouse_y: f64, cx: &mut Context<Self>) {
        if let Some(ref drag) = self.dragging_node {
            let zoom = self.zoom.max(0.01);
            let dx = (mouse_x - drag.start_x) / zoom;
            let dy = (mouse_y - drag.start_y) / zoom;
            let new_x = (drag.node_start_x + dx).max(0.0);
            let new_y = (drag.node_start_y + dy).max(0.0);
            let nid = drag.node_id.clone();
            if let Some(node) = self.canvas.nodes.iter_mut().find(|n| n.id == nid) {
                node.x = new_x;
                node.y = new_y;
            }
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Finish dragging a node.
    fn end_drag(&mut self) {
        self.dragging_node = None;
    }

    /// Start panning the canvas.
    fn start_pan(&mut self, mouse_x: f64, mouse_y: f64) {
        self.panning = Some(PanState {
            start_mouse_x: mouse_x,
            start_mouse_y: mouse_y,
            start_offset_x: self.canvas_offset.0,
            start_offset_y: self.canvas_offset.1,
        });
    }

    /// Update pan offset based on mouse movement.
    fn update_pan(&mut self, mouse_x: f64, mouse_y: f64, cx: &mut Context<Self>) {
        if let Some(ref pan) = self.panning {
            let dx = mouse_x - pan.start_mouse_x;
            let dy = mouse_y - pan.start_mouse_y;
            self.canvas_offset.0 = pan.start_offset_x + dx;
            self.canvas_offset.1 = pan.start_offset_y + dy;
            cx.notify();
        }
    }

    /// Finish panning.
    fn end_pan(&mut self) {
        self.panning = None;
    }

    /// Start connecting from a port.
    fn start_connect(&mut self, node_id: &str, port: Port, cx: &mut Context<Self>) {
        self.connecting_from = Some((node_id.to_string(), port));
        self.connection_preview_pos = self
            .canvas
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .map(|node| Self::port_position(node, port))
            .map(|(x, y)| self.canvas_to_display(x, y));
        cx.notify();
    }

    /// Finish connection at a target port.
    fn finish_connect(&mut self, target_node_id: &str, target_port: Port, cx: &mut Context<Self>) {
        if let Some((from_id, from_port)) = self.connecting_from.take() {
            // Don't connect a node to itself
            if from_id != target_node_id {
                self.connect_nodes(&from_id, from_port, target_node_id, target_port, cx);
            }
        }
        self.connection_preview_pos = None;
        cx.notify();
    }

    /// Cancel connection.
    fn cancel_connect(&mut self, cx: &mut Context<Self>) {
        self.connecting_from = None;
        self.connection_preview_pos = None;
        cx.notify();
    }

    /// Persist the current canvas state to disk, clear the dirty flag, and emit
    /// a [`WorkflowSaved`] event.
    pub fn save_workflow(&mut self, cx: &mut Context<Self>) {
        // Sync viewport state into the serialisable canvas model.
        self.canvas.canvas_offset_x = self.canvas_offset.0;
        self.canvas.canvas_offset_y = self.canvas_offset.1;
        self.canvas.zoom = self.zoom;

        match self.canvas.save_to_disk() {
            Ok(()) => {
                self.is_dirty = false;
                info!(
                    workflow_id = %self.canvas.workflow_id,
                    name = %self.canvas.name,
                    "Workflow canvas saved to disk"
                );
                cx.emit(WorkflowSaved(self.canvas.workflow_id.clone()));
            }
            Err(e) => {
                error!(
                    workflow_id = %self.canvas.workflow_id,
                    err = %e,
                    "Failed to save workflow canvas to disk"
                );
            }
        }
        cx.notify();
    }

    pub fn set_active_workflow_id(
        &mut self,
        workflow_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.active_workflow_id = workflow_id;
        cx.notify();
    }

    /// Port position for a node (relative to canvas). Returns (x, y) center of port.
    fn port_position(node: &CanvasNode, port: Port) -> (f64, f64) {
        match port {
            Port::Input => (node.x, node.y + node.height / 2.0),
            Port::Output => (node.x + node.width, node.y + node.height / 2.0),
            Port::TrueOutput => (node.x + node.width, node.y + node.height * 0.33),
            Port::FalseOutput => (node.x + node.width, node.y + node.height * 0.67),
        }
    }

    fn canvas_to_display(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x + self.canvas_offset.0) * self.zoom,
            (y + self.canvas_offset.1) * self.zoom,
        )
    }

    fn edge_color(&self, port: Port) -> Hsla {
        match port {
            Port::TrueOutput => self.theme.accent_green,
            Port::FalseOutput => self.theme.accent_red,
            _ => self.theme.accent_cyan,
        }
    }

    fn render_edge_segments(
        &self,
        elements: &mut Vec<AnyElement>,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        color: Hsla,
    ) {
        let mid_x = (from_x + to_x) / 2.0;

        let h1_x = from_x.min(mid_x);
        let h1_w = (mid_x - from_x).abs().max(1.0);
        elements.push(
            div()
                .absolute()
                .left(px(h1_x))
                .top(px(from_y - 1.0))
                .w(px(h1_w))
                .h(px(2.0))
                .bg(color)
                .into_any_element(),
        );

        let v_top = from_y.min(to_y);
        let v_h = (to_y - from_y).abs().max(1.0);
        elements.push(
            div()
                .absolute()
                .left(px(mid_x - 1.0))
                .top(px(v_top))
                .w(px(2.0))
                .h(px(v_h))
                .bg(color)
                .into_any_element(),
        );

        let h2_x = mid_x.min(to_x);
        let h2_w = (to_x - mid_x).abs().max(1.0);
        elements.push(
            div()
                .absolute()
                .left(px(h2_x))
                .top(px(to_y - 1.0))
                .w(px(h2_w))
                .h(px(2.0))
                .bg(color)
                .into_any_element(),
        );
    }

    fn connecting_status(&self) -> Option<String> {
        let (node_id, port) = self.connecting_from.as_ref()?;
        let node = self.canvas.nodes.iter().find(|node| node.id == *node_id)?;
        let port_label = match port {
            Port::Input => "input",
            Port::Output => "output",
            Port::TrueOutput => "true branch",
            Port::FalseOutput => "false branch",
        };
        Some(format!(
            "Connecting from {} ({port_label}). Click a target node or input port to finish. Click empty canvas to cancel.",
            node.label
        ))
    }

    fn default_action_for_label(label: &str) -> Option<ActionType> {
        match label {
            "Run Command" => Some(ActionType::RunCommand {
                command: "echo workflow step".into(),
            }),
            "Call API" => Some(ActionType::CallApi {
                url: "https://api.github.com".into(),
                method: "GET".into(),
            }),
            "Send Notification" => Some(ActionType::SendNotification {
                title: "Workflow update".into(),
                body: "Step completed".into(),
            }),
            "Execute Skill" => Some(ActionType::ExecuteSkill {
                skill_trigger: "/review".into(),
                input: "Review the current change".into(),
            }),
            _ => None,
        }
    }

    fn palette_item_supported(label: &str, kind: NodeKind) -> bool {
        !matches!(kind, NodeKind::Condition)
            && !matches!(label, "Execute Skill")
    }

    fn palette_item_note(label: &str, kind: NodeKind) -> Option<&'static str> {
        if matches!(kind, NodeKind::Condition) {
            Some("branch logic not wired yet")
        } else if label == "Execute Skill" {
            Some("runtime support still pending")
        } else {
            None
        }
    }

    fn connected_action_ids(&self) -> Vec<String> {
        let Some(trigger_node) = self
            .canvas
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Trigger)
        else {
            return Vec::new();
        };

        let mut ordered_action_ids = Vec::new();
        let mut visited_nodes = HashSet::new();
        let mut queued_nodes = HashSet::new();
        let mut queue = VecDeque::from([trigger_node.id.clone()]);
        queued_nodes.insert(trigger_node.id.clone());

        while let Some(node_id) = queue.pop_front() {
            if !visited_nodes.insert(node_id.clone()) {
                continue;
            }

            if let Some(node) = self.canvas.nodes.iter().find(|candidate| candidate.id == node_id)
                && node.kind == NodeKind::Action
            {
                ordered_action_ids.push(node.id.clone());
            }

            let mut outgoing: Vec<_> = self
                .canvas
                .edges
                .iter()
                .filter(|edge| edge.from_node_id == node_id)
                .collect();
            outgoing.sort_by_key(|edge| match edge.from_port {
                Port::Output => 0,
                Port::TrueOutput => 1,
                Port::FalseOutput => 2,
                Port::Input => 3,
            });

            for edge in outgoing {
                if queued_nodes.insert(edge.to_node_id.clone()) {
                    queue.push_back(edge.to_node_id.clone());
                }
            }
        }

        ordered_action_ids
    }

    fn action_configuration_error(node: &CanvasNode) -> Option<String> {
        let action = node.action.as_ref()?;
        match action {
            ActionType::RunCommand { command } if command.trim().is_empty() => {
                Some(format!("'{}' has no command configured.", node.label))
            }
            ActionType::CallApi { url, method } if url.trim().is_empty() || method.trim().is_empty() => {
                Some(format!("'{}' is missing its API URL or method.", node.label))
            }
            ActionType::SendMessage { channel, content } if channel.trim().is_empty() || content.trim().is_empty() => {
                Some(format!("'{}' is missing its channel or message.", node.label))
            }
            ActionType::SendNotification { title, body }
                if title.trim().is_empty() || body.trim().is_empty() =>
            {
                Some(format!("'{}' is missing its notification title or body.", node.label))
            }
            ActionType::CreateTask { title } if title.trim().is_empty() => {
                Some(format!("'{}' is missing a task title.", node.label))
            }
            ActionType::ExecuteSkill { .. } => Some(format!(
                "'{}' uses Execute Skill, which the workflow runtime does not support yet.",
                node.label
            )),
            _ => None,
        }
    }

    fn validation_issues(
        &self,
        connected_ids: &HashSet<String>,
    ) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let trigger_count = self
            .canvas
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Trigger)
            .count();
        let action_nodes: Vec<_> = self
            .canvas
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Action)
            .collect();

        if trigger_count == 0 {
            errors.push("Add a Trigger node before running the workflow.".into());
        } else if trigger_count > 1 {
            warnings.push("Multiple Trigger nodes are present. Hive only follows the first trigger today.".into());
        }

        if action_nodes.is_empty() {
            errors.push("Add at least one Action node before running the workflow.".into());
        }

        for node in &self.canvas.nodes {
            if node.kind == NodeKind::Condition {
                errors.push(format!(
                    "'{}' is a Condition node, but branching is not executable yet.",
                    node.label
                ));
            }

            if let Some(issue) = Self::action_configuration_error(node) {
                errors.push(issue);
            }
        }

        let has_edges = !self.canvas.edges.is_empty();

        if has_edges {
            let unreachable_actions: Vec<_> = action_nodes
                .iter()
                .filter(|node| !connected_ids.contains(&node.id))
                .map(|node| node.label.clone())
                .collect();
            if !unreachable_actions.is_empty() {
                warnings.push(format!(
                    "Unreachable action nodes will not run: {}.",
                    unreachable_actions.join(", ")
                ));
            }
        } else if action_nodes.len() > 1 {
            warnings.push(
                "No node connections yet. Multiple actions will run in canvas order until you wire them together.".into(),
            );
        } else if action_nodes.len() == 1 {
            warnings.push(
                "This workflow has one action and no connections yet. It can still run, but wiring it from the Trigger is recommended.".into(),
            );
        }

        (errors, warnings)
    }

    fn node_messages(
        &self,
        node: &CanvasNode,
        connected_ids: &HashSet<String>,
    ) -> Vec<(String, Hsla)> {
        let mut messages = Vec::new();

        if node.kind == NodeKind::Condition {
            messages.push((
                "Condition branches are not executable yet.".into(),
                self.theme.accent_red,
            ));
        }

        if let Some(issue) = Self::action_configuration_error(node) {
            messages.push((issue, self.theme.accent_red));
        }

        if node.kind == NodeKind::Action {
            if !self.canvas.edges.is_empty() && !connected_ids.contains(&node.id) {
                messages.push((
                    "This action is not connected to the trigger path, so it will not run.".into(),
                    self.theme.accent_yellow,
                ));
            }

            messages.push((
                "Action editing is still limited here. New nodes use starter values until the node editor lands.".into(),
                self.theme.text_muted,
            ));
        }

        messages
    }

    /// Convert the current canvas to an executable automation `Workflow`.
    pub fn to_executable_workflow(&self) -> Workflow {
        let mut steps: Vec<WorkflowStep> = Vec::new();

        // Find trigger
        let trigger = self
            .canvas
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Trigger)
            .and_then(|n| n.trigger.clone())
            .unwrap_or(TriggerType::ManualTrigger);

        let ordered_action_ids = self.connected_action_ids();
        for action_id in ordered_action_ids {
            if let Some(node) = self
                .canvas
                .nodes
                .iter()
                .find(|candidate| candidate.id == action_id)
                && let Some(ref action) = node.action
            {
                steps.push(WorkflowStep {
                    id: node.id.clone(),
                    name: node.label.clone(),
                    action: action.clone(),
                    conditions: node.conditions.clone(),
                    timeout_secs: node.timeout_secs,
                    retry_count: node.retry_count,
                });
            }
        }

        if steps.is_empty() {
            for node in &self.canvas.nodes {
                if node.kind == NodeKind::Action
                    && let Some(ref action) = node.action
                {
                    steps.push(WorkflowStep {
                        id: node.id.clone(),
                        name: node.label.clone(),
                        action: action.clone(),
                        conditions: node.conditions.clone(),
                        timeout_secs: node.timeout_secs,
                        retry_count: node.retry_count,
                    });
                }
            }
        }

        Workflow {
            id: self.canvas.workflow_id.clone(),
            name: self.canvas.name.clone(),
            description: self.canvas.description.clone(),
            trigger,
            steps,
            status: WorkflowStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_run: None,
            run_count: 0,
        }
    }

    // -- Render helpers -------------------------------------------------------

    fn node_color(&self, kind: NodeKind) -> Hsla {
        match kind {
            NodeKind::Trigger => self.theme.accent_green,
            NodeKind::Action => self.theme.accent_cyan,
            NodeKind::Condition => self.theme.accent_yellow,
            NodeKind::Output => self.theme.accent_pink,
        }
    }

    fn render_node_palette(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let palette_items = [
            ("Trigger", NodeKind::Trigger),
            ("Run Command", NodeKind::Action),
            ("Call API", NodeKind::Action),
            ("Send Notification", NodeKind::Action),
            ("Execute Skill", NodeKind::Action),
            ("Condition", NodeKind::Condition),
            ("End", NodeKind::Output),
        ];

        let mut items: Vec<AnyElement> = Vec::new();
        for (label, kind) in &palette_items {
            let color = self.node_color(*kind);
            let mut bg = color;
            bg.a = 0.15;
            let label_str = label.to_string();
            let kind_copy = *kind;
            let is_supported = Self::palette_item_supported(label, *kind);
            let note = Self::palette_item_note(label, *kind);

            items.push(
                div()
                    .id(ElementId::Name(format!("palette-{label}").into()))
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_md)
                    .bg(bg)
                    .text_size(theme.font_size_xs)
                    .text_color(if is_supported { color } else { theme.text_muted })
                    .when(is_supported, |el| el.cursor_pointer().hover(|s| s.bg(theme.bg_surface)))
                    .when(is_supported, |el| {
                        el.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _e, _w, cx| {
                                let node = match kind_copy {
                                    NodeKind::Trigger => CanvasNode::new_trigger(300.0, 200.0),
                                    NodeKind::Action => CanvasNode::new_action(
                                        &label_str,
                                        Self::default_action_for_label(&label_str).unwrap_or(
                                            ActionType::RunCommand {
                                                command: "echo workflow step".into(),
                                            },
                                        ),
                                        300.0,
                                        200.0,
                                    ),
                                    NodeKind::Condition => {
                                        CanvasNode::new_condition(&label_str, Vec::new(), 300.0, 200.0)
                                    }
                                    NodeKind::Output => CanvasNode::new_output(300.0, 200.0),
                                };
                                this.add_node(node, cx);
                            }),
                        )
                    })
                    .when(!is_supported, |el| el.opacity(0.55))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(theme.space_2)
                            .child(label.to_string())
                            .when_some(note, |el, note| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(theme.text_muted)
                                        .child(note),
                                )
                            }),
                    )
                    .into_any_element(),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(theme.space_1)
            .w(px(200.0))
            .min_w(px(200.0))
            .border_r_1()
            .border_color(theme.border)
            .p(theme.space_3)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::BOLD)
                    .pb(theme.space_2)
                    .child("NODE PALETTE"),
            )
            .children(items)
            .child(
                div()
                    .mt(theme.space_4)
                    .border_t_1()
                    .border_color(theme.border)
                    .pt(theme.space_3)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_weight(FontWeight::BOLD)
                            .pb(theme.space_2)
                            .child("WORKFLOWS"),
                    )
                    .children(self.workflow_list.iter().map(|wf| {
                        let is_active = self.active_workflow_id.as_deref() == Some(&wf.id);
                        let workflow_id = wf.id.clone();
                        let workflow_name = wf.name.clone();
                        div()
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .text_size(theme.font_size_xs)
                            .cursor_pointer()
                            .text_color(if is_active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .when(is_active, |el| el.bg(theme.bg_surface))
                            .hover(|el| el.bg(theme.bg_secondary))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_this, _event, window, cx| {
                                    window.dispatch_action(
                                        Box::new(hive_ui_core::WorkflowBuilderLoadWorkflow {
                                            workflow_id: workflow_id.clone(),
                                        }),
                                        cx,
                                    );
                                }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(theme.space_2)
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_color(if is_active {
                                                theme.text_primary
                                            } else {
                                                theme.text_secondary
                                            })
                                            .child(workflow_name),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(if wf.is_builtin {
                                                theme.accent_yellow
                                            } else {
                                                theme.accent_cyan
                                            })
                                            .child(if wf.is_builtin { "builtin" } else { "saved" }),
                                    ),
                            )
                            .into_any_element()
                    })),
            )
    }

    fn render_canvas_nodes(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut elements: Vec<AnyElement> = Vec::new();
        let offset_x = self.canvas_offset.0 as f32;
        let offset_y = self.canvas_offset.1 as f32;
        let zoom = self.zoom as f32;
        let port_size = 16.0;
        let port_offset = port_size / 2.0;

        for node in &self.canvas.nodes {
            let color = self.node_color(node.kind);
            let mut bg = color;
            bg.a = 0.12;
            let is_selected = self.selected_node_id.as_deref() == Some(&node.id);
            let is_connection_source = self
                .connecting_from
                .as_ref()
                .is_some_and(|(source_id, _)| source_id == &node.id);
            let node_id = node.id.clone();
            let node_id2 = node.id.clone();
            let node_id_input = node.id.clone();

            // Compute display position with canvas offset, scaled by zoom
            let display_x = (node.x as f32 + offset_x) * zoom;
            let display_y = (node.y as f32 + offset_y) * zoom;
            let node_w = node.width as f32 * zoom;
            let node_h = node.height as f32 * zoom;

            // Determine which ports to show based on node kind
            let has_input = node.kind != NodeKind::Trigger;
            let has_output = node.kind == NodeKind::Trigger || node.kind == NodeKind::Action;
            let is_condition = node.kind == NodeKind::Condition;

            // Build port circles
            let mut port_elements: Vec<AnyElement> = Vec::new();

            // Input port (left side)
            if has_input {
                let nid = node_id_input.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-in-{}", node.id).into()))
                        .absolute()
                        .left(px(-port_offset))
                        .top(px(node_h / 2.0 - port_offset))
                        .w(px(port_size))
                        .h(px(port_size))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_aqua)
                        .border_2()
                        .border_color(if self.connecting_from.is_some() {
                            theme.accent_cyan
                        } else {
                            theme.bg_primary
                        })
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                // If connecting from another node, finish the connection here
                                if this.connecting_from.is_some() {
                                    this.finish_connect(&nid, Port::Input, cx);
                                }
                            }),
                        )
                        .into_any_element(),
                );
            }

            // Output port (right side)
            if has_output {
                let nid = node.id.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-out-{}", node.id).into()))
                        .absolute()
                        .right(px(-port_offset))
                        .top(px(node_h / 2.0 - port_offset))
                        .w(px(port_size))
                        .h(px(port_size))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_cyan)
                        .border_2()
                        .border_color(if is_connection_source {
                            theme.accent_cyan
                        } else {
                            theme.bg_primary
                        })
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                this.start_connect(&nid, Port::Output, cx);
                            }),
                        )
                        .into_any_element(),
                );
            }

            // Condition node: True (top-right) and False (bottom-right) output ports
            if is_condition {
                let nid_true = node.id.clone();
                let nid_false = node.id.clone();
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-true-{}", node.id).into()))
                        .absolute()
                        .right(px(-port_offset))
                        .top(px(node_h * 0.25 - port_offset))
                        .w(px(port_size))
                        .h(px(port_size))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_green)
                        .border_2()
                        .border_color(if is_connection_source {
                            theme.accent_green
                        } else {
                            theme.bg_primary
                        })
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                this.start_connect(&nid_true, Port::TrueOutput, cx);
                            }),
                        )
                        .into_any_element(),
                );
                port_elements.push(
                    div()
                        .id(ElementId::Name(format!("port-false-{}", node.id).into()))
                        .absolute()
                        .right(px(-port_offset))
                        .top(px(node_h * 0.75 - port_offset))
                        .w(px(port_size))
                        .h(px(port_size))
                        .rounded(theme.radius_full)
                        .bg(theme.accent_red)
                        .border_2()
                        .border_color(if is_connection_source {
                            theme.accent_red
                        } else {
                            theme.bg_primary
                        })
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                this.start_connect(&nid_false, Port::FalseOutput, cx);
                            }),
                        )
                        .into_any_element(),
                );
            }

            let node_el = div()
                .id(ElementId::Name(format!("node-{}", node.id).into()))
                .absolute()
                .left(px(display_x))
                .top(px(display_y))
                .w(px(node_w))
                .h(px(node_h))
                .rounded(theme.radius_md)
                .bg(bg)
                .border_1()
                .border_color(if is_selected { color } else { theme.border })
                .when(is_selected, |el| el.border_2())
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        // If we're in connect mode and click a node body, finish connect
                        // to its input port
                        if this.connecting_from.is_some() {
                            this.finish_connect(&node_id2, Port::Input, cx);
                            return;
                        }
                        this.selected_node_id = Some(node_id.clone());
                        let pos = event.position;
                        this.start_drag(&node_id, f64::from(pos.x), f64::from(pos.y));
                        cx.notify();
                    }),
                )
                // Port circles
                .children(port_elements)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(color)
                                .font_weight(FontWeight::BOLD)
                                .child(match node.kind {
                                    NodeKind::Trigger => "\u{25B6}",
                                    NodeKind::Action => "\u{2699}",
                                    NodeKind::Condition => "\u{2747}",
                                    NodeKind::Output => "\u{2713}",
                                }),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_primary)
                                .child(node.label.clone()),
                        )
                        .when_some(node.persona.as_ref(), |el, persona| {
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(theme.text_muted)
                                    .child(persona_kind_label(persona)),
                            )
                        }),
                )
                .into_any_element();

            elements.push(node_el);
        }

        // Render edges as simple colored lines using positioned divs
        for edge in &self.canvas.edges {
            let from_node = self.canvas.nodes.iter().find(|n| n.id == edge.from_node_id);
            let to_node = self.canvas.nodes.iter().find(|n| n.id == edge.to_node_id);
            if let (Some(from), Some(to)) = (from_node, to_node) {
                let (fp_x, fp_y) = Self::port_position(from, edge.from_port);
                let (tp_x, tp_y) = Self::port_position(to, edge.to_port);
                let from_x = (fp_x as f32 + offset_x) * zoom;
                let from_y = (fp_y as f32 + offset_y) * zoom;
                let to_x = (tp_x as f32 + offset_x) * zoom;
                let to_y = (tp_y as f32 + offset_y) * zoom;
                self.render_edge_segments(
                    &mut elements,
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                    self.edge_color(edge.from_port),
                );
            }
        }

        if let Some((source_node_id, source_port)) = &self.connecting_from
            && let Some((preview_x, preview_y)) = self.connection_preview_pos
            && let Some(source_node) = self
                .canvas
                .nodes
                .iter()
                .find(|node| node.id == *source_node_id)
        {
            let (source_x, source_y) = Self::port_position(source_node, *source_port);
            let (from_x, from_y) = self.canvas_to_display(source_x, source_y);
            self.render_edge_segments(
                &mut elements,
                from_x as f32,
                from_y as f32,
                preview_x as f32,
                preview_y as f32,
                self.edge_color(*source_port),
            );
        }

        elements
    }

    fn render_properties_panel(
        &self,
        theme: &HiveTheme,
        connected_ids: &HashSet<String>,
    ) -> impl IntoElement {
        let Some(ref node_id) = self.selected_node_id else {
            return div()
                .w(px(280.0))
                .min_w(px(280.0))
                .border_l_1()
                .border_color(theme.border)
                .p(theme.space_3)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("Select a node to view properties"),
                );
        };

        let node = self.canvas.nodes.iter().find(|n| n.id == *node_id);
        let node_messages = node
            .map(|node| self.node_messages(node, connected_ids))
            .unwrap_or_default();

        div()
            .w(px(280.0))
            .min_w(px(280.0))
            .border_l_1()
            .border_color(theme.border)
            .p(theme.space_3)
            .flex()
            .flex_col()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::BOLD)
                    .child("PROPERTIES"),
            )
            .when_some(node, |el, node| {
                el.child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::BOLD)
                        .child(node.label.clone()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("Type: {}", node_kind_label(node.kind))),
                )
                .when_some(node.action.as_ref(), |el, action| {
                    el.child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .child(format!("Action: {}", action_type_label(action))),
                    )
                })
                .when_some(node.persona.as_ref(), |el, persona| {
                    el.child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_aqua)
                            .child(format!("Agent: {}", persona_kind_label(persona))),
                    )
                })
                .children(node_messages.into_iter().map(|(message, color)| {
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(color)
                        .child(message)
                        .into_any_element()
                }))
            })
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for WorkflowBuilderView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let node_count = self.canvas.nodes.len();
        let edge_count = self.canvas.edges.len();
        let connected_ids: HashSet<String> =
            self.connected_action_ids().into_iter().collect();
        let (validation_errors, validation_warnings) =
            self.validation_issues(&connected_ids);
        let can_run = validation_errors.is_empty();

        // Header
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px(theme.space_4)
            .py(theme.space_3)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_3)
                    .child(
                        div()
                            .text_size(theme.font_size_lg)
                            .text_color(theme.text_primary)
                            .font_weight(FontWeight::BOLD)
                            .child("Workflow Builder"),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(format!(
                                "{} \u{2014} {} nodes \u{00B7} {} edges",
                                self.canvas.name, node_count, edge_count
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space_2)
                    // Palette toggle button
                    .child({
                        let palette_bg = if self.show_node_palette {
                            let mut c = theme.accent_cyan;
                            c.a = 0.15;
                            c
                        } else {
                            theme.bg_tertiary
                        };
                        div()
                            .id("toggle-palette-btn")
                            .px(theme.space_2)
                            .py(theme.space_1)
                            .rounded(theme.radius_sm)
                            .bg(palette_bg)
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.show_node_palette = !this.show_node_palette;
                                    cx.notify();
                                }),
                            )
                            .child("Palette")
                    })
                    // Zoom controls
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme.space_1)
                            .child(
                                div()
                                    .id("zoom-out-btn")
                                    .px(theme.space_2)
                                    .py(theme.space_1)
                                    .rounded(theme.radius_sm)
                                    .bg(theme.bg_tertiary)
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom = (this.zoom - 0.1).max(0.3);
                                            cx.notify();
                                        }),
                                    )
                                    .child("\u{2212}"),
                            )
                            .child(
                                div()
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_muted)
                                    .child(format!("{:.0}%", self.zoom * 100.0)),
                            )
                            .child(
                                div()
                                    .id("zoom-in-btn")
                                    .px(theme.space_2)
                                    .py(theme.space_1)
                                    .rounded(theme.radius_sm)
                                    .bg(theme.bg_tertiary)
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_secondary)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom = (this.zoom + 0.1).min(3.0);
                                            cx.notify();
                                        }),
                                    )
                                    .child("+"),
                            ),
                    )
                    // Save button
                    .child(
                        div()
                            .id("wf-save-btn")
                            .px(theme.space_3)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .bg(if self.is_dirty {
                                theme.accent_cyan
                            } else {
                                theme.bg_tertiary
                            })
                            .text_size(theme.font_size_sm)
                            .text_color(if self.is_dirty {
                                theme.bg_primary
                            } else {
                                theme.text_muted
                            })
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _e, _w, cx| {
                                    this.save_workflow(cx);
                                }),
                            )
                            .child("Save"),
                    )
                    // Run button
                    .child(
                        div()
                            .id("wf-run-btn")
                            .px(theme.space_3)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .bg(if can_run {
                                theme.accent_green
                            } else {
                                theme.bg_tertiary
                            })
                            .text_size(theme.font_size_sm)
                            .text_color(if can_run {
                                theme.bg_primary
                            } else {
                                theme.text_muted
                            })
                            .font_weight(FontWeight::BOLD)
                            .when(can_run, |el| {
                                el.cursor_pointer().on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _e, _w, cx| {
                                        let wf_id = this.canvas.workflow_id.clone();
                                        cx.emit(WorkflowRunRequested(wf_id));
                                    }),
                                )
                            })
                            .when(!can_run, |el| el.opacity(0.65))
                            .child("\u{25B6} Run"),
                    ),
            );

        let builder_tip = if self.connecting_from.is_some() {
            None
        } else {
            let mut tip_bg = theme.bg_secondary;
            tip_bg.a = 0.85;
            Some(
                div()
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(tip_bg)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(
                                "Drag nodes from the palette, then click a colored output dot and a target node to connect the flow.",
                            ),
                    )
                    .into_any_element(),
            )
        };

        let connection_banner = self.connecting_status().map(|status| {
            let mut banner_bg = theme.accent_cyan;
            banner_bg.a = 0.12;
            div()
                .px(theme.space_4)
                .py(theme.space_2)
                .border_b_1()
                .border_color(theme.border)
                .bg(banner_bg)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_cyan)
                        .font_weight(FontWeight::BOLD)
                        .child(status),
                )
                .into_any_element()
        });

        let validation_banner = if validation_errors.is_empty() && validation_warnings.is_empty() {
            None
        } else {
            let mut banner_bg = if !validation_errors.is_empty() {
                theme.accent_red
            } else {
                theme.accent_yellow
            };
            banner_bg.a = 0.12;
            let accent = if !validation_errors.is_empty() {
                theme.accent_red
            } else {
                theme.accent_yellow
            };
            let mut messages: Vec<AnyElement> = validation_errors
                .iter()
                .take(3)
                .map(|message| {
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(accent)
                        .child(format!("• {message}"))
                        .into_any_element()
                })
                .collect();
            messages.extend(validation_warnings.iter().take(3).map(|message| {
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(format!("• {message}"))
                    .into_any_element()
            }));

            let hidden_count = validation_errors.len().saturating_sub(3)
                + validation_warnings.len().saturating_sub(3);

            Some(
                div()
                    .px(theme.space_4)
                    .py(theme.space_2)
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(banner_bg)
                    .flex()
                    .flex_col()
                    .gap(theme.space_1)
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(accent)
                            .font_weight(FontWeight::BOLD)
                            .child(if !validation_errors.is_empty() {
                                "Workflow validation blocked run"
                            } else {
                                "Workflow warnings"
                            }),
                    )
                    .children(messages)
                    .when(hidden_count > 0, |el| {
                        el.child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!("+{hidden_count} more validation note(s)")),
                        )
                    })
                    .into_any_element(),
            )
        };

        // Canvas area with nodes + interaction handlers
        let canvas_elements = self.render_canvas_nodes(theme, cx);
        let is_connecting = self.connecting_from.is_some();

        let canvas_area = div()
            .id("wf-canvas")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .relative()
            .overflow_hidden()
            .bg(theme.bg_primary)
            .when(is_connecting, |el| el.cursor(CursorStyle::Crosshair))
            // Mouse down on canvas background → start panning
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                    // If we're in connect mode and click the background, cancel
                    if this.connecting_from.is_some() {
                        this.cancel_connect(cx);
                        return;
                    }
                    // Start panning
                    let pos = event.position;
                    this.start_pan(f64::from(pos.x), f64::from(pos.y));
                    // Deselect node
                    this.selected_node_id = None;
                    cx.notify();
                }),
            )
            // Mouse move → update drag or pan
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _w, cx| {
                let pos = event.position;
                let mx = f64::from(pos.x);
                let my = f64::from(pos.y);
                if this.connecting_from.is_some() {
                    this.connection_preview_pos = Some((mx, my));
                    cx.notify();
                } else if this.dragging_node.is_some() {
                    this.update_drag(mx, my, cx);
                } else if this.panning.is_some() {
                    this.update_pan(mx, my, cx);
                }
            }))
            // Mouse up → end drag or pan
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _w, _cx| {
                    this.end_drag();
                    this.end_pan();
                }),
            )
            .children(canvas_elements);

        // Node palette (left)
        let palette = self.render_node_palette(theme, cx).into_any_element();

        // Properties (right)
        let properties = self.render_properties_panel(theme, &connected_ids).into_any_element();

        let show_palette = self.show_node_palette;

        div()
            .id("workflow-builder-panel")
            .flex()
            .flex_col()
            .size_full()
            .child(header)
            .when_some(validation_banner, |el, banner| el.child(banner))
            .when_some(builder_tip, |el, tip| el.child(tip))
            .when_some(connection_banner, |el, banner| el.child(banner))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .when(show_palette, |el| el.child(palette))
                    .child(canvas_area)
                    .when(
                        self.show_properties_panel || self.selected_node_id.is_some(),
                        |el| el.child(properties),
                    ),
            )
    }
}
