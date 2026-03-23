use std::path::PathBuf;

use gpui::*;

use super::{AppRpcConfig, AppWallets, HiveConfig, HiveWorkspace};

impl HiveWorkspace {
    pub(super) fn sync_token_launch_inputs_to_data(&mut self, cx: &App) {
        self.token_launch_data.token_name = self
            .token_launch_inputs
            .token_name
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.token_launch_data.token_symbol = self
            .token_launch_inputs
            .token_symbol
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.token_launch_data.total_supply = self
            .token_launch_inputs
            .total_supply
            .read(cx)
            .value()
            .trim()
            .to_string();

        let default_decimals = self
            .token_launch_data
            .selected_chain
            .map(|chain| chain.default_decimals())
            .unwrap_or(9);
        self.token_launch_data.decimals = self
            .token_launch_inputs
            .decimals
            .read(cx)
            .value()
            .trim()
            .parse::<u8>()
            .unwrap_or(default_decimals);

        if !matches!(
            self.token_launch_data.deploy_status,
            hive_ui_panels::panels::token_launch::DeployStatus::Deploying
        ) {
            self.token_launch_data.deploy_status =
                hive_ui_panels::panels::token_launch::DeployStatus::NotStarted;
        }
    }

    pub(super) fn token_launch_wallet_password() -> String {
        use hive_core::SecureStorage;

        const FALLBACK: &str = "hive-wallet-default";

        let password_path = match HiveConfig::base_dir() {
            Ok(dir) => dir.join("wallet_password.enc"),
            Err(_) => return FALLBACK.to_string(),
        };

        let storage = match SecureStorage::new() {
            Ok(s) => s,
            Err(_) => return FALLBACK.to_string(),
        };

        if let Ok(hex_ct) = std::fs::read_to_string(&password_path) {
            let hex_ct = hex_ct.trim();
            if !hex_ct.is_empty() {
                if let Ok(password) = storage.decrypt(hex_ct) {
                    return password;
                }
            }
        }

        let password = Self::generate_random_password(32);

        if let Ok(encrypted) = storage.encrypt(&password) {
            let _ = std::fs::write(&password_path, &encrypted);

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &password_path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
        }

        password
    }

    pub(super) fn generate_random_password(len: usize) -> String {
        use rand::Rng;

        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        (0..len)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    pub(super) fn token_launch_wallet_path() -> PathBuf {
        HiveConfig::base_dir()
            .map(|dir| dir.join("wallets.enc"))
            .unwrap_or_else(|_| PathBuf::from("wallets.enc"))
    }

    pub(super) fn token_launch_rpc_config_path() -> PathBuf {
        HiveConfig::base_dir()
            .map(|dir| dir.join("rpc_config.json"))
            .unwrap_or_else(|_| PathBuf::from("rpc_config.json"))
    }

    pub(super) fn token_launch_chain(
        option: hive_ui_panels::panels::token_launch::ChainOption,
    ) -> hive_blockchain::Chain {
        match option {
            hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                hive_blockchain::Chain::Solana
            }
            hive_ui_panels::panels::token_launch::ChainOption::Ethereum => {
                hive_blockchain::Chain::Ethereum
            }
            hive_ui_panels::panels::token_launch::ChainOption::Base => hive_blockchain::Chain::Base,
        }
    }

    pub(super) fn token_launch_secret_placeholder(
        option: Option<hive_ui_panels::panels::token_launch::ChainOption>,
    ) -> &'static str {
        match option {
            Some(hive_ui_panels::panels::token_launch::ChainOption::Solana) => {
                "Solana private key (hex or base58)"
            }
            Some(
                hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                | hive_ui_panels::panels::token_launch::ChainOption::Base,
            ) => "EVM private key (hex)",
            None => "Select a chain to configure wallet import",
        }
    }

    pub(super) fn token_launch_current_rpc_url(chain: hive_blockchain::Chain, cx: &App) -> String {
        if cx.has_global::<AppRpcConfig>() {
            return cx
                .global::<AppRpcConfig>()
                .0
                .get_rpc(chain)
                .map(|config| config.url.clone())
                .unwrap_or_default();
        }

        hive_blockchain::RpcConfigStore::with_defaults()
            .get_rpc(chain)
            .map(|config| config.url.clone())
            .unwrap_or_default()
    }

    pub(super) fn sync_token_launch_rpc_input(
        &self,
        option: Option<hive_ui_panels::panels::token_launch::ChainOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (value, placeholder) = match option {
            Some(chain) => {
                let chain = Self::token_launch_chain(chain);
                (
                    Self::token_launch_current_rpc_url(chain, cx),
                    "https://rpc.example.com",
                )
            }
            None => (String::new(), "Select a chain to configure RPC"),
        };

        self.token_launch_inputs.rpc_url.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
            state.set_value(value, window, cx);
        });
    }

    pub(super) fn persist_token_launch_rpc_config(
        &self,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if !cx.has_global::<AppRpcConfig>() {
            return Ok(());
        }

        let rpc_path = Self::token_launch_rpc_config_path();
        if let Some(parent) = rpc_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        cx.global::<AppRpcConfig>().0.save_to_file(&rpc_path)?;
        Ok(())
    }

    pub(super) fn persist_token_launch_wallets(
        &self,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if !cx.has_global::<AppWallets>() {
            return Ok(());
        }

        let wallet_path = Self::token_launch_wallet_path();
        if let Some(parent) = wallet_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        cx.global::<AppWallets>().0.save_to_file(&wallet_path)?;
        Ok(())
    }

    pub(super) fn clear_token_launch_wallet_secret(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.token_launch_inputs
            .wallet_secret
            .update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
    }

    pub(super) fn sync_token_launch_saved_wallets(
        &mut self,
        preserve_current: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.token_launch_data.available_wallets.clear();
            self.token_launch_data.wallet_id = None;
            self.token_launch_data.wallet_address = None;
            self.token_launch_data.wallet_balance = None;
            self.token_launch_inputs
                .wallet_name
                .update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            return;
        };

        let chain = Self::token_launch_chain(option);
        let current_wallet_id = if preserve_current {
            self.token_launch_data.wallet_id.clone()
        } else {
            None
        };

        let available_wallets = if cx.has_global::<AppWallets>() {
            let mut wallets = cx
                .global::<AppWallets>()
                .0
                .list_wallets()
                .into_iter()
                .filter(|wallet| wallet.chain == chain)
                .collect::<Vec<_>>();
            wallets.sort_by_key(|wallet| std::cmp::Reverse(wallet.created_at));
            wallets
                .into_iter()
                .map(
                    |wallet| hive_ui_panels::panels::token_launch::SavedWalletOption {
                        id: wallet.id.clone(),
                        name: wallet.name.clone(),
                        address: wallet.address.clone(),
                    },
                )
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        self.token_launch_data.available_wallets = available_wallets;
        let selected_wallet = current_wallet_id
            .as_deref()
            .and_then(|id| {
                self.token_launch_data
                    .available_wallets
                    .iter()
                    .find(|wallet| wallet.id == id)
            })
            .or_else(|| self.token_launch_data.available_wallets.first())
            .cloned();

        if let Some(wallet) = selected_wallet {
            self.token_launch_data.wallet_id = Some(wallet.id.clone());
            self.token_launch_data.wallet_address = Some(wallet.address.clone());
            self.token_launch_inputs
                .wallet_name
                .update(cx, |state, cx| {
                    state.set_value(wallet.name.clone(), window, cx);
                });
            self.refresh_token_launch_balance(cx);
        } else {
            self.token_launch_data.wallet_id = None;
            self.token_launch_data.wallet_address = None;
            self.token_launch_data.wallet_balance = None;
            self.token_launch_inputs
                .wallet_name
                .update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
        }

        self.clear_token_launch_wallet_secret(window, cx);
    }

    pub(super) fn restore_token_launch_wallet_for_chain(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_token_launch_saved_wallets(false, window, cx);
    }

    pub(super) fn refresh_token_launch_cost(&mut self, cx: &mut Context<Self>) {
        let Some(option) = self.token_launch_data.selected_chain else {
            self.token_launch_data.estimated_cost = None;
            cx.notify();
            return;
        };
        let chain = Self::token_launch_chain(option);
        let rpc_url = Self::token_launch_current_rpc_url(chain, cx);

        cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let estimated_cost = match option {
                    hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                        hive_blockchain::solana::estimate_deploy_cost_with_rpc(Some(
                            rpc_url.as_str(),
                        ))
                        .await
                        .ok()
                    }
                    hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                    | hive_ui_panels::panels::token_launch::ChainOption::Base => {
                        hive_blockchain::evm::estimate_deploy_cost_with_rpc(
                            Self::token_launch_chain(option),
                            Some(rpc_url.as_str()),
                        )
                        .await
                        .ok()
                    }
                };

                let _ = this.update(app, |this, cx| {
                    this.token_launch_data.estimated_cost = estimated_cost;
                    cx.notify();
                });
            },
        )
        .detach();
    }

    pub(super) fn refresh_token_launch_balance(&mut self, cx: &mut Context<Self>) {
        let (Some(option), Some(address)) = (
            self.token_launch_data.selected_chain,
            self.token_launch_data.wallet_address.clone(),
        ) else {
            self.token_launch_data.wallet_balance = None;
            cx.notify();
            return;
        };
        let chain = Self::token_launch_chain(option);
        let rpc_url = Self::token_launch_current_rpc_url(chain, cx);

        cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let balance = match option {
                    hive_ui_panels::panels::token_launch::ChainOption::Solana => {
                        hive_blockchain::solana::get_balance_with_rpc(
                            &address,
                            Some(rpc_url.as_str()),
                        )
                        .await
                        .ok()
                    }
                    hive_ui_panels::panels::token_launch::ChainOption::Ethereum
                    | hive_ui_panels::panels::token_launch::ChainOption::Base => {
                        hive_blockchain::evm::get_balance_with_rpc(
                            &address,
                            Self::token_launch_chain(option),
                            Some(rpc_url.as_str()),
                        )
                        .await
                        .ok()
                    }
                };

                let _ = this.update(app, |this, cx| {
                    this.token_launch_data.wallet_balance = balance;
                    cx.notify();
                });
            },
        )
        .detach();
    }
}
