use gpui::*;
use tracing::info;

use super::{
    AppRpcConfig, AppWallets, HiveWorkspace, NotificationType, TokenLaunchCreateWallet,
    TokenLaunchDeploy, TokenLaunchImportWallet, TokenLaunchResetRpcConfig,
    TokenLaunchSaveRpcConfig, TokenLaunchSelectChain, TokenLaunchSelectWallet,
    TokenLaunchSetStep,
};

pub(super) fn handle_token_launch_set_step(
    workspace: &mut HiveWorkspace,
    action: &TokenLaunchSetStep,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::token_launch::WizardStep;

    info!("TokenLaunch: set step {}", action.step);
    workspace.token_launch_data.current_step = match action.step {
        0 => WizardStep::SelectChain,
        1 => WizardStep::TokenDetails,
        2 => WizardStep::WalletSetup,
        _ => WizardStep::Deploy,
    };
    cx.notify();
}

pub(super) fn handle_token_launch_select_chain(
    workspace: &mut HiveWorkspace,
    action: &TokenLaunchSelectChain,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::token_launch::ChainOption;

    info!("TokenLaunch: select chain {}", action.chain);
    workspace.token_launch_data.selected_chain = match action.chain.as_str() {
        "solana" => Some(ChainOption::Solana),
        "ethereum" => Some(ChainOption::Ethereum),
        "base" => Some(ChainOption::Base),
        _ => None,
    };

    if let Some(chain) = workspace.token_launch_data.selected_chain {
        workspace.token_launch_data.decimals = chain.default_decimals();
        workspace.token_launch_inputs.decimals.update(cx, |state, cx| {
            state.set_value(chain.default_decimals().to_string(), window, cx);
        });
        workspace
            .token_launch_inputs
            .wallet_secret
            .update(cx, |state, cx| {
                state.set_placeholder(
                    HiveWorkspace::token_launch_secret_placeholder(Some(chain)),
                    window,
                    cx,
                );
            });
    } else {
        workspace.token_launch_data.estimated_cost = None;
        workspace
            .token_launch_inputs
            .wallet_secret
            .update(cx, |state, cx| {
                state.set_placeholder(
                    HiveWorkspace::token_launch_secret_placeholder(None),
                    window,
                    cx,
                );
            });
    }

    workspace.sync_token_launch_inputs_to_data(cx);
    workspace.sync_token_launch_rpc_input(workspace.token_launch_data.selected_chain, window, cx);
    workspace.restore_token_launch_wallet_for_chain(window, cx);
    workspace.refresh_token_launch_cost(cx);
    cx.notify();
}

pub(super) fn handle_token_launch_save_rpc_config(
    workspace: &mut HiveWorkspace,
    _action: &TokenLaunchSaveRpcConfig,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let Some(option) = workspace.token_launch_data.selected_chain else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "Select a target chain before saving an RPC endpoint.",
        );
        return;
    };

    let rpc_url = workspace
        .token_launch_inputs
        .rpc_url
        .read(cx)
        .value()
        .trim()
        .to_string();
    if rpc_url.is_empty() {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "RPC endpoint cannot be empty. Use Reset RPC to restore the default.",
        );
        return;
    }

    let chain = HiveWorkspace::token_launch_chain(option);
    let result = if cx.has_global::<AppRpcConfig>() {
        cx.global_mut::<AppRpcConfig>()
            .0
            .set_custom_rpc(chain, rpc_url.clone())
    } else {
        Err(anyhow::anyhow!("RPC config store is not available."))
    };

    match result {
        Ok(()) => {
            if let Err(e) = workspace.persist_token_launch_rpc_config(cx) {
                workspace.push_notification(
                    cx,
                    NotificationType::Warning,
                    "Token Launch",
                    format!("RPC endpoint saved, but persistence failed: {e}"),
                );
            }
            workspace.refresh_token_launch_cost(cx);
            workspace.refresh_token_launch_balance(cx);
            workspace.push_notification(
                cx,
                NotificationType::Success,
                "Token Launch",
                format!("Saved custom RPC for {}.", chain.label()),
            );
            cx.notify();
        }
        Err(e) => {
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                format!("Invalid RPC endpoint: {e}"),
            );
        }
    }
}

pub(super) fn handle_token_launch_reset_rpc_config(
    workspace: &mut HiveWorkspace,
    _action: &TokenLaunchResetRpcConfig,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let Some(option) = workspace.token_launch_data.selected_chain else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "Select a target chain before resetting an RPC endpoint.",
        );
        return;
    };

    let chain = HiveWorkspace::token_launch_chain(option);
    if cx.has_global::<AppRpcConfig>() {
        cx.global_mut::<AppRpcConfig>().0.reset_to_default(chain);
    } else {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Token Launch",
            "RPC config store is not available.",
        );
        return;
    }

    if let Err(e) = workspace.persist_token_launch_rpc_config(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            format!("RPC endpoint reset, but persistence failed: {e}"),
        );
    }

    workspace.sync_token_launch_rpc_input(Some(option), window, cx);
    workspace.refresh_token_launch_cost(cx);
    workspace.refresh_token_launch_balance(cx);
    workspace.push_notification(
        cx,
        NotificationType::Success,
        "Token Launch",
        format!("Restored default RPC for {}.", chain.label()),
    );
    cx.notify();
}

pub(super) fn handle_token_launch_create_wallet(
    workspace: &mut HiveWorkspace,
    _action: &TokenLaunchCreateWallet,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.sync_token_launch_inputs_to_data(cx);

    let Some(option) = workspace.token_launch_data.selected_chain else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "Select a target chain before creating a wallet.",
        );
        return;
    };

    let chain = HiveWorkspace::token_launch_chain(option);
    let wallet_name = workspace
        .token_launch_inputs
        .wallet_name
        .read(cx)
        .value()
        .trim()
        .to_string();
    let wallet_name = if wallet_name.is_empty() {
        format!("{} Wallet", chain.label())
    } else {
        wallet_name
    };

    let (private_key, address) = match hive_blockchain::generate_wallet_material(chain) {
        Ok(material) => material,
        Err(e) => {
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                format!("Wallet creation failed: {e}"),
            );
            return;
        }
    };

    let encrypted_key = match hive_blockchain::encrypt_key(
        &private_key,
        &HiveWorkspace::token_launch_wallet_password(),
    ) {
        Ok(encrypted) => encrypted,
        Err(e) => {
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                format!("Wallet encryption failed: {e}"),
            );
            return;
        }
    };

    let wallet_id = if cx.has_global::<AppWallets>() {
        cx.global_mut::<AppWallets>().0.add_wallet(
            wallet_name.clone(),
            chain,
            address.clone(),
            encrypted_key,
        )
    } else {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Token Launch",
            "Wallet store is not available.",
        );
        return;
    };

    if let Err(e) = workspace.persist_token_launch_wallets(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            format!("Wallet created, but saving failed: {e}"),
        );
    }

    workspace.token_launch_data.wallet_id = Some(wallet_id);
    workspace.token_launch_data.wallet_address = Some(address);
    workspace.token_launch_data.wallet_balance = None;
    workspace.sync_token_launch_saved_wallets(true, window, cx);
    workspace.token_launch_inputs.wallet_name.update(cx, |state, cx| {
        state.set_value(wallet_name, window, cx);
    });
    workspace.clear_token_launch_wallet_secret(window, cx);
    workspace.refresh_token_launch_balance(cx);
    workspace.push_notification(
        cx,
        NotificationType::Success,
        "Token Launch",
        "Wallet created and connected.",
    );
    cx.notify();
}

pub(super) fn handle_token_launch_import_wallet(
    workspace: &mut HiveWorkspace,
    _action: &TokenLaunchImportWallet,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.sync_token_launch_inputs_to_data(cx);

    let Some(option) = workspace.token_launch_data.selected_chain else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "Select a target chain before importing a wallet.",
        );
        return;
    };

    let chain = HiveWorkspace::token_launch_chain(option);
    let wallet_name = workspace
        .token_launch_inputs
        .wallet_name
        .read(cx)
        .value()
        .trim()
        .to_string();
    let wallet_name = if wallet_name.is_empty() {
        format!("Imported {}", chain.label())
    } else {
        wallet_name
    };
    let secret = workspace
        .token_launch_inputs
        .wallet_secret
        .read(cx)
        .value()
        .trim()
        .to_string();

    let (private_key, address) = match hive_blockchain::import_wallet_material(chain, &secret) {
        Ok(material) => material,
        Err(e) => {
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                format!("Wallet import failed: {e}"),
            );
            return;
        }
    };

    let encrypted_key = match hive_blockchain::encrypt_key(
        &private_key,
        &HiveWorkspace::token_launch_wallet_password(),
    ) {
        Ok(encrypted) => encrypted,
        Err(e) => {
            workspace.push_notification(
                cx,
                NotificationType::Error,
                "Token Launch",
                format!("Wallet encryption failed: {e}"),
            );
            return;
        }
    };

    let wallet_id = if cx.has_global::<AppWallets>() {
        cx.global_mut::<AppWallets>().0.add_wallet(
            wallet_name.clone(),
            chain,
            address.clone(),
            encrypted_key,
        )
    } else {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Token Launch",
            "Wallet store is not available.",
        );
        return;
    };

    if let Err(e) = workspace.persist_token_launch_wallets(cx) {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            format!("Wallet imported, but saving failed: {e}"),
        );
    }

    workspace.token_launch_data.wallet_id = Some(wallet_id);
    workspace.token_launch_data.wallet_address = Some(address);
    workspace.token_launch_data.wallet_balance = None;
    workspace.sync_token_launch_saved_wallets(true, window, cx);
    workspace.token_launch_inputs.wallet_name.update(cx, |state, cx| {
        state.set_value(wallet_name, window, cx);
    });
    workspace.clear_token_launch_wallet_secret(window, cx);
    workspace.refresh_token_launch_balance(cx);
    workspace.push_notification(
        cx,
        NotificationType::Success,
        "Token Launch",
        "Wallet imported and connected.",
    );
    cx.notify();
}

pub(super) fn handle_token_launch_select_wallet(
    workspace: &mut HiveWorkspace,
    action: &TokenLaunchSelectWallet,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let Some(option) = workspace.token_launch_data.selected_chain else {
        workspace.push_notification(
            cx,
            NotificationType::Warning,
            "Token Launch",
            "Select a target chain before choosing a wallet.",
        );
        return;
    };

    let chain = HiveWorkspace::token_launch_chain(option);
    let selected_wallet = if cx.has_global::<AppWallets>() {
        cx.global::<AppWallets>()
            .0
            .get_wallet(&action.wallet_id)
            .filter(|wallet| wallet.chain == chain)
            .map(|wallet| {
                (
                    wallet.id.clone(),
                    wallet.name.clone(),
                    wallet.address.clone(),
                )
            })
    } else {
        None
    };

    if let Some((wallet_id, wallet_name, wallet_address)) = selected_wallet {
        workspace.token_launch_data.wallet_id = Some(wallet_id);
        workspace.token_launch_data.wallet_address = Some(wallet_address);
        workspace
            .token_launch_inputs
            .wallet_name
            .update(cx, |state, cx| {
                state.set_value(wallet_name, window, cx);
            });
        workspace.sync_token_launch_saved_wallets(true, window, cx);
        workspace.push_notification(
            cx,
            NotificationType::Success,
            "Token Launch",
            "Connected saved wallet.",
        );
        cx.notify();
    } else {
        workspace.push_notification(
            cx,
            NotificationType::Error,
            "Token Launch",
            "Saved wallet not found for the selected chain.",
        );
    }
}

pub(super) fn handle_token_launch_deploy(
    workspace: &mut HiveWorkspace,
    _action: &TokenLaunchDeploy,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::token_launch::{ChainOption, DeployStatus};

    info!("TokenLaunch: deploy");
    workspace.sync_token_launch_inputs_to_data(cx);

    if workspace.token_launch_data.selected_chain.is_none() {
        workspace.token_launch_data.deploy_status =
            DeployStatus::Failed("Select a target chain before deploying.".to_string());
        cx.notify();
        return;
    }

    if workspace.token_launch_data.token_name.trim().is_empty()
        || workspace.token_launch_data.token_symbol.trim().is_empty()
        || workspace.token_launch_data.total_supply.trim().is_empty()
    {
        workspace.token_launch_data.deploy_status = DeployStatus::Failed(
            "Token name, symbol, and total supply are required.".to_string(),
        );
        cx.notify();
        return;
    }

    if workspace.token_launch_data.wallet_address.is_none()
        || workspace.token_launch_data.wallet_id.is_none()
    {
        workspace.token_launch_data.deploy_status =
            DeployStatus::Failed("Connect a wallet before deploying.".to_string());
        cx.notify();
        return;
    }

    if let (Some(balance), Some(cost)) = (
        workspace.token_launch_data.wallet_balance,
        workspace.token_launch_data.estimated_cost,
    ) && balance < cost
    {
        workspace.token_launch_data.deploy_status = DeployStatus::Failed(
            "Connected wallet does not have enough funds for the estimated deployment cost."
                .to_string(),
        );
        cx.notify();
        return;
    }

    let wallet_id = workspace
        .token_launch_data
        .wallet_id
        .clone()
        .unwrap_or_default();
    let private_key = if cx.has_global::<AppWallets>() {
        match cx
            .global::<AppWallets>()
            .0
            .decrypt_wallet_key(&wallet_id, &HiveWorkspace::token_launch_wallet_password())
        {
            Ok(key) => key,
            Err(e) => {
                workspace.token_launch_data.deploy_status =
                    DeployStatus::Failed(format!("Failed to unlock wallet: {e}"));
                cx.notify();
                return;
            }
        }
    } else {
        workspace.token_launch_data.deploy_status =
            DeployStatus::Failed("Wallet store is not available.".to_string());
        cx.notify();
        return;
    };

    let selected_chain = workspace.token_launch_data.selected_chain.unwrap();
    let token_name = workspace.token_launch_data.token_name.clone();
    let token_symbol = workspace.token_launch_data.token_symbol.clone();
    let total_supply = workspace.token_launch_data.total_supply.clone();
    let decimals = workspace.token_launch_data.decimals;
    let rpc_url = HiveWorkspace::token_launch_current_rpc_url(
        HiveWorkspace::token_launch_chain(selected_chain),
        cx,
    );

    workspace.token_launch_data.deploy_status = DeployStatus::Deploying;
    cx.notify();

    cx.spawn(async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
        let deploy_result = match selected_chain {
            ChainOption::Solana => match total_supply.parse::<u64>() {
                Ok(supply) => hive_blockchain::solana::create_spl_token_with_rpc(
                    hive_blockchain::SplTokenParams {
                        name: token_name,
                        symbol: token_symbol,
                        decimals,
                        supply,
                        metadata_uri: None,
                    },
                    &private_key,
                    Some(rpc_url.as_str()),
                )
                .await
                .map(|result| result.mint_address),
                Err(_) => Err(anyhow::anyhow!(
                    "Total supply must fit into an unsigned 64-bit integer for Solana deployments."
                )),
            },
            ChainOption::Ethereum | ChainOption::Base => hive_blockchain::evm::deploy_token_with_rpc(
                hive_blockchain::TokenDeployParams {
                    name: token_name,
                    symbol: token_symbol,
                    decimals,
                    total_supply,
                    chain: HiveWorkspace::token_launch_chain(selected_chain),
                },
                &private_key,
                Some(rpc_url.as_str()),
            )
            .await
            .map(|result| result.contract_address),
        };

        let _ = this.update(app, |workspace, cx| {
            workspace.token_launch_data.deploy_status = match deploy_result {
                Ok(address) => DeployStatus::Success(address),
                Err(e) => DeployStatus::Failed(e.to_string()),
            };
            cx.notify();
        });
    })
    .detach();
}
