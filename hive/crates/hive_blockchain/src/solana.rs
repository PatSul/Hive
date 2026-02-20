use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Solana mainnet RPC endpoint.
const SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com";

/// SPL Token program ID (mainnet).
const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

/// A Solana wallet identified by its base58-encoded public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaWallet {
    pub address: String,
}

/// Parameters for creating a new SPL token on Solana.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplTokenParams {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub supply: u64,
    pub metadata_uri: Option<String>,
}

/// Result of a successful SPL token deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplDeployResult {
    pub mint_address: String,
    pub tx_signature: String,
}

/// Infrastructure for an unsigned Solana transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedSolanaTransaction {
    pub recent_blockhash: String,
    pub instructions: Vec<String>, // Placeholder for instruction data
    pub fee_payer: String,
}

/// Simulated signed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedSolanaTransaction {
    pub raw: String,
    pub signatures: Vec<String>,
}

pub fn build_spl_deploy_tx(_params: &SplTokenParams, payer: &str, blockhash: &str) -> UnsignedSolanaTransaction {
    UnsignedSolanaTransaction {
        recent_blockhash: blockhash.to_string(),
        instructions: vec!["CreateAccount".to_string(), "InitializeMint".to_string()],
        fee_payer: payer.to_string(),
    }
}

pub fn sign_solana_tx_simulated(tx: &UnsignedSolanaTransaction, _private_key: &[u8]) -> SignedSolanaTransaction {
    let mut hasher = DefaultHasher::new();
    tx.recent_blockhash.hash(&mut hasher);
    tx.fee_payer.hash(&mut hasher);
    let hash_val = hasher.finish();
    let sig = format!("SimTx{hash_val:016x}{hash_val:016x}");
    SignedSolanaTransaction {
        raw: format!("SIMULATED_SIGNED_{:x}", hash_val),
        signatures: vec![sig],
    }
}

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

/// Execute a JSON-RPC call against the Solana mainnet RPC endpoint.
///
/// Builds a `{"jsonrpc":"2.0","method":...,"params":...,"id":1}` request,
/// POSTs it, and returns the `"result"` field from the response.
async fn solana_rpc_call(
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(SOLANA_RPC_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Solana JSON-RPC request failed")?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .context("failed to read Solana JSON-RPC response body")?;

    if !status.is_success() {
        anyhow::bail!("Solana JSON-RPC HTTP error {status}: {text}");
    }

    let json: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse Solana JSON-RPC response")?;

    if let Some(err) = json.get("error") {
        anyhow::bail!("Solana JSON-RPC error: {err}");
    }

    json.get("result")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Solana JSON-RPC response missing 'result' field"))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Estimate the cost (in SOL) to create an SPL token with metadata.
///
/// Queries the Solana RPC for the minimum balance required for rent
/// exemption of an SPL token mint account (~165 bytes). Falls back to
/// 0.05 SOL if the RPC call fails.
pub async fn estimate_deploy_cost() -> Result<f64> {
    const LAMPORTS_PER_SOL: f64 = 1e9;
    // SPL token mint account size.
    const MINT_ACCOUNT_SIZE: u64 = 165;

    match solana_rpc_call(
        "getMinimumBalanceForRentExemption",
        serde_json::json!([MINT_ACCOUNT_SIZE]),
    )
    .await
    {
        Ok(result) => {
            let lamports = result
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("rent exemption result is not a number"))?;
            let sol = lamports as f64 / LAMPORTS_PER_SOL;

            info!(
                lamports = lamports,
                sol = sol,
                "estimated SPL token deploy cost (rent exemption)"
            );
            Ok(sol)
        }
        Err(e) => {
            warn!(
                error = %e,
                "failed to fetch rent exemption cost, using fallback estimate"
            );
            Ok(0.05)
        }
    }
}

/// Create a new SPL token on Solana (simulation mode).
///
/// Because a real on-chain deployment requires a signed transaction with
/// a funded keypair, this function operates in *simulation mode*: it
/// queries the live rent exemption cost, computes a realistic estimate,
/// and returns an [`SplDeployResult`] with a deterministic placeholder
/// signature. Actual on-chain execution requires wallet signing.
pub async fn create_spl_token(
    params: SplTokenParams,
    _private_key: &[u8],
) -> Result<SplDeployResult> {
    const LAMPORTS_PER_SOL: f64 = 1e9;
    const MINT_ACCOUNT_SIZE: u64 = 165;

    // Fetch real rent exemption cost.
    let rent_lamports = match solana_rpc_call(
        "getMinimumBalanceForRentExemption",
        serde_json::json!([MINT_ACCOUNT_SIZE]),
    )
    .await
    {
        Ok(result) => result.as_u64().unwrap_or(2_039_280),
        Err(_) => 2_039_280, // ~0.00204 SOL fallback
    };

    let cost_sol = rent_lamports as f64 / LAMPORTS_PER_SOL;

    // Build deterministic placeholder identifiers from config fields.
    let mut hasher = DefaultHasher::new();
    params.name.hash(&mut hasher);
    params.symbol.hash(&mut hasher);
    params.decimals.hash(&mut hasher);
    let hash_val = hasher.finish();

    // Base58-like placeholder (not real base58 encoding, but deterministic
    // and visually recognisable as a simulation artifact).
    let mint_address = format!("SimMint{hash_val:016x}");

    // Integrate transaction building infrastructure
    let tx = build_spl_deploy_tx(&params, "SimPayer", "SimBlockhash");
    let signed_tx = sign_solana_tx_simulated(&tx, _private_key);

    let tx_signature = signed_tx.signatures.first().cloned().unwrap_or_else(|| "SimTx".to_string());

    info!(
        name = %params.name,
        symbol = %params.symbol,
        estimated_cost_sol = cost_sol,
        "Token deployment prepared — wallet signing required for on-chain execution"
    );

    Ok(SplDeployResult {
        mint_address,
        tx_signature,
    })
}

/// Query the SOL balance for a wallet address.
///
/// Calls the Solana `getBalance` RPC method and converts lamports to SOL.
pub async fn get_balance(address: &str) -> Result<f64> {
    const LAMPORTS_PER_SOL: f64 = 1e9;

    let result = solana_rpc_call("getBalance", serde_json::json!([address]))
        .await
        .context("getBalance RPC call failed")?;

    let lamports = result
        .get("value")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("getBalance response missing 'value' field"))?;

    let balance = lamports as f64 / LAMPORTS_PER_SOL;

    info!(address = %address, balance = balance, "fetched Solana balance");
    Ok(balance)
}

/// Query SPL token balances for a wallet address.
///
/// Calls `getTokenAccountsByOwner` with the SPL Token program ID and
/// `jsonParsed` encoding, then extracts the mint address and token amount
/// for each account.
pub async fn get_token_balances(address: &str) -> Result<Vec<(String, f64)>> {
    let result = solana_rpc_call(
        "getTokenAccountsByOwner",
        serde_json::json!([
            address,
            { "programId": SPL_TOKEN_PROGRAM_ID },
            { "encoding": "jsonParsed" }
        ]),
    )
    .await
    .context("getTokenAccountsByOwner RPC call failed")?;

    let accounts = result
        .get("value")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("token accounts response missing 'value' array"))?;

    let mut balances = Vec::new();

    for account in accounts {
        // Navigate: account -> account -> data -> parsed -> info
        let info = account
            .get("account")
            .and_then(|a| a.get("data"))
            .and_then(|d| d.get("parsed"))
            .and_then(|p| p.get("info"));

        if let Some(info) = info {
            let mint = info
                .get("mint")
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();

            let amount = info
                .get("tokenAmount")
                .and_then(|ta| ta.get("uiAmount"))
                .and_then(|ui| ui.as_f64())
                .unwrap_or(0.0);

            if !mint.is_empty() {
                balances.push((mint, amount));
            }
        }
    }

    info!(
        address = %address,
        token_count = balances.len(),
        "fetched Solana token balances"
    );
    Ok(balances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solana_wallet_serializes() {
        let wallet = SolanaWallet {
            address: "7EcDhSYGxXyscszYEp35KHN8vvw3svAuLKTzXwCFLtV".into(),
        };
        let json = serde_json::to_string(&wallet).unwrap();
        let parsed: SolanaWallet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.address, wallet.address);
    }

    #[test]
    fn spl_token_params_serializes() {
        let params = SplTokenParams {
            name: "HiveToken".into(),
            symbol: "HIVE".into(),
            decimals: 9,
            supply: 1_000_000_000,
            metadata_uri: Some("https://example.com/meta.json".into()),
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: SplTokenParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, "HIVE");
        assert_eq!(parsed.decimals, 9);
        assert!(parsed.metadata_uri.is_some());
    }

    #[test]
    fn spl_token_params_optional_metadata() {
        let params = SplTokenParams {
            name: "Bare".into(),
            symbol: "BARE".into(),
            decimals: 6,
            supply: 1000,
            metadata_uri: None,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: SplTokenParams = serde_json::from_str(&json).unwrap();
        assert!(parsed.metadata_uri.is_none());
    }

    #[test]
    fn spl_deploy_result_serializes() {
        let result = SplDeployResult {
            mint_address: "TokenMintAddress123".into(),
            tx_signature: "5KtP8...sig".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SplDeployResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mint_address, "TokenMintAddress123");
    }

    #[tokio::test]
    async fn estimate_deploy_cost_returns_positive() {
        let cost = estimate_deploy_cost().await.unwrap();
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn create_spl_token_returns_simulation_result() {
        let params = SplTokenParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 9,
            supply: 1000,
            metadata_uri: None,
        };
        let result = create_spl_token(params, &[0u8; 64]).await;
        // Simulation mode should succeed (not bail!).
        assert!(result.is_ok());
        let deploy = result.unwrap();
        assert!(deploy.mint_address.starts_with("SimMint"));
        assert!(deploy.tx_signature.starts_with("SimTx"));
    }

    #[tokio::test]
    async fn create_spl_token_deterministic() {
        let make_params = || SplTokenParams {
            name: "Alpha".into(),
            symbol: "ALPHA".into(),
            decimals: 9,
            supply: 1_000_000,
            metadata_uri: None,
        };
        let r1 = create_spl_token(make_params(), &[0u8; 64]).await.unwrap();
        let r2 = create_spl_token(make_params(), &[0u8; 64]).await.unwrap();
        assert_eq!(r1.mint_address, r2.mint_address);
        assert_eq!(r1.tx_signature, r2.tx_signature);
    }

    // --- New RPC-focused unit tests ---

    #[test]
    fn lamports_to_sol_conversion() {
        const LAMPORTS_PER_SOL: f64 = 1e9;
        // 1 SOL
        let lamports: u64 = 1_000_000_000;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 1.0).abs() < f64::EPSILON);

        // 0.5 SOL
        let lamports: u64 = 500_000_000;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 0.5).abs() < f64::EPSILON);

        // 0 SOL
        let lamports: u64 = 0;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_get_balance_response() {
        // Simulate the JSON structure returned by getBalance.
        let response: serde_json::Value = serde_json::json!({
            "value": 2_500_000_000u64
        });
        let lamports = response
            .get("value")
            .and_then(|v| v.as_u64())
            .unwrap();
        let sol = lamports as f64 / 1e9;
        assert!((sol - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_token_accounts_response() {
        // Simulate the JSON structure returned by getTokenAccountsByOwner.
        let response: serde_json::Value = serde_json::json!({
            "value": [
                {
                    "account": {
                        "data": {
                            "parsed": {
                                "info": {
                                    "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                                    "tokenAmount": {
                                        "uiAmount": 150.25,
                                        "decimals": 6,
                                        "amount": "150250000"
                                    }
                                }
                            }
                        }
                    }
                },
                {
                    "account": {
                        "data": {
                            "parsed": {
                                "info": {
                                    "mint": "So11111111111111111111111111111111111111112",
                                    "tokenAmount": {
                                        "uiAmount": 3.0,
                                        "decimals": 9,
                                        "amount": "3000000000"
                                    }
                                }
                            }
                        }
                    }
                }
            ]
        });

        let accounts = response
            .get("value")
            .and_then(|v| v.as_array())
            .unwrap();

        let mut balances = Vec::new();
        for account in accounts {
            let info = account
                .get("account")
                .and_then(|a| a.get("data"))
                .and_then(|d| d.get("parsed"))
                .and_then(|p| p.get("info"));

            if let Some(info) = info {
                let mint = info
                    .get("mint")
                    .and_then(|m| m.as_str())
                    .unwrap_or_default()
                    .to_string();
                let amount = info
                    .get("tokenAmount")
                    .and_then(|ta| ta.get("uiAmount"))
                    .and_then(|ui| ui.as_f64())
                    .unwrap_or(0.0);
                if !mint.is_empty() {
                    balances.push((mint, amount));
                }
            }
        }

        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances[0].0,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
        assert!((balances[0].1 - 150.25).abs() < f64::EPSILON);
        assert_eq!(
            balances[1].0,
            "So11111111111111111111111111111111111111112"
        );
        assert!((balances[1].1 - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rent_exemption_fallback_value() {
        // The fallback value 2_039_280 lamports should be ~0.00204 SOL.
        let fallback: u64 = 2_039_280;
        let sol = fallback as f64 / 1e9;
        assert!((sol - 0.00203928).abs() < 1e-8);
    }
}
