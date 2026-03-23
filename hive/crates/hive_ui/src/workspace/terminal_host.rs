use gpui::*;

use super::{
    HiveWorkspace, InteractiveShell, ShellOutput, TerminalClear, TerminalCmd, TerminalKill,
    TerminalRestart, TerminalSubmitCommand,
};

pub(super) fn ensure_terminal_shell(
    workspace: &mut HiveWorkspace,
    cx: &mut Context<HiveWorkspace>,
) {
    if workspace.terminal_cmd_tx.is_some() {
        return;
    }

    let cwd = std::path::PathBuf::from(&workspace.terminal_data.cwd);
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<TerminalCmd>();
    workspace.terminal_cmd_tx = Some(cmd_tx);
    workspace.terminal_data.is_running = true;
    workspace.terminal_data.push_system("Shell starting...");

    let task = cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
        let mut shell = match InteractiveShell::new(Some(&cwd)) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Failed to start shell: {e}");
                let _ = this.update(app, |ws, cx| {
                    ws.terminal_data.push_system(&msg);
                    ws.terminal_data.is_running = false;
                    ws.terminal_cmd_tx = None;
                    cx.notify();
                });
                return;
            }
        };

        let _ = this.update(app, |ws, cx| {
            ws.terminal_data.push_system("Shell ready.");
            cx.notify();
        });

        loop {
            tokio::select! {
                output = shell.read_async() => {
                    match output {
                        Some(ShellOutput::Stdout(line)) => {
                            let _ = this.update(app, |ws, cx| {
                                ws.terminal_data.push_line(
                                    hive_ui_panels::panels::terminal::TerminalLineKind::Stdout,
                                    line,
                                );
                                cx.notify();
                            });
                        }
                        Some(ShellOutput::Stderr(line)) => {
                            let _ = this.update(app, |ws, cx| {
                                ws.terminal_data.push_line(
                                    hive_ui_panels::panels::terminal::TerminalLineKind::Stderr,
                                    line,
                                );
                                cx.notify();
                            });
                        }
                        Some(ShellOutput::Exit(code)) => {
                            let msg = format!("Shell exited with code {code}");
                            let _ = this.update(app, |ws, cx| {
                                ws.terminal_data.push_system(&msg);
                                ws.terminal_data.is_running = false;
                                ws.terminal_cmd_tx = None;
                                cx.notify();
                            });
                            return;
                        }
                        None => {
                            let _ = this.update(app, |ws, cx| {
                                ws.terminal_data.push_system("Shell disconnected.");
                                ws.terminal_data.is_running = false;
                                ws.terminal_cmd_tx = None;
                                cx.notify();
                            });
                            return;
                        }
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(TerminalCmd::Write(text)) => {
                            if let Err(e) = shell.write(&format!("{text}\n")).await {
                                let msg = format!("Write error: {e}");
                                let _ = this.update(app, |ws, cx| {
                                    ws.terminal_data.push_system(&msg);
                                    cx.notify();
                                });
                            }
                        }
                        Some(TerminalCmd::Kill) => {
                            let _ = shell.kill().await;
                            let _ = this.update(app, |ws, cx| {
                                ws.terminal_data.push_system("Shell killed.");
                                ws.terminal_data.is_running = false;
                                ws.terminal_cmd_tx = None;
                                cx.notify();
                            });
                            return;
                        }
                        None => {
                            let _ = shell.kill().await;
                            return;
                        }
                    }
                }
            }
        }
    });
    workspace._terminal_task = Some(task);
    cx.notify();
}

pub(super) fn handle_terminal_clear(
    workspace: &mut HiveWorkspace,
    _action: &TerminalClear,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.terminal_data.lines.clear();
    cx.notify();
}

pub(super) fn handle_terminal_submit(
    workspace: &mut HiveWorkspace,
    _action: &TerminalSubmitCommand,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let cmd = workspace.terminal_input.read(cx).text().to_string();
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return;
    }

    workspace.terminal_input.update(cx, |input, cx| {
        input.set_value("", window, cx);
    });
    workspace.terminal_data.push_line(
        hive_ui_panels::panels::terminal::TerminalLineKind::Stdin,
        cmd.clone(),
    );

    if let Some(tx) = &workspace.terminal_cmd_tx {
        let _ = tx.send(TerminalCmd::Write(cmd));
    } else {
        workspace
            .terminal_data
            .push_system("No shell running. Restarting...");
        ensure_terminal_shell(workspace, cx);
    }
    cx.notify();
}

pub(super) fn handle_terminal_kill(
    workspace: &mut HiveWorkspace,
    _action: &TerminalKill,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if let Some(tx) = workspace.terminal_cmd_tx.take() {
        let _ = tx.send(TerminalCmd::Kill);
    }
    workspace._terminal_task = None;
    workspace.terminal_data.is_running = false;
    cx.notify();
}

pub(super) fn handle_terminal_restart(
    workspace: &mut HiveWorkspace,
    _action: &TerminalRestart,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if let Some(tx) = workspace.terminal_cmd_tx.take() {
        let _ = tx.send(TerminalCmd::Kill);
    }
    workspace._terminal_task = None;
    workspace.terminal_data.is_running = false;
    workspace.terminal_data.push_system("Restarting shell...");
    ensure_terminal_shell(workspace, cx);
}
