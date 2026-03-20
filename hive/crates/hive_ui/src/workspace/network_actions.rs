use gpui::*;

use super::{
    format_network_relative_time, network_peer_status_label, network_peer_status_rank,
    AppNetwork, HiveWorkspace, NetworkRefresh, PeerDisplayInfo,
};

pub(super) fn refresh_network_peer_data(
    workspace: &mut HiveWorkspace,
    cx: &App,
) {
    if !cx.has_global::<AppNetwork>() {
        return;
    }

    let node = &cx.global::<AppNetwork>().0;
    workspace.network_peer_data.our_peer_id = node.peer_id().to_string();
    let mut peers: Vec<PeerDisplayInfo> = node
        .peers_snapshot()
        .into_iter()
        .map(|peer| PeerDisplayInfo {
            name: peer.identity.name,
            status: network_peer_status_label(&peer.state),
            address: peer.addr.to_string(),
            latency_ms: peer.latency_ms,
            last_seen: format_network_relative_time(peer.last_seen),
        })
        .collect();

    peers.sort_by(|a, b| {
        network_peer_status_rank(&a.status)
            .cmp(&network_peer_status_rank(&b.status))
            .then_with(|| a.name.cmp(&b.name))
    });

    workspace.network_peer_data.peers = peers;
}

pub(super) fn handle_network_refresh(
    workspace: &mut HiveWorkspace,
    _action: &NetworkRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    tracing::info!("Network: refresh");
    refresh_network_peer_data(workspace, cx);
    cx.notify();
}
