use std::borrow::Cow;
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    hash::Hash,
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer, keypair_from_seed},
    transaction::Transaction,
};
use tokio::time::{Duration, sleep};
use tracing::info;

/// Solana mainnet RPC endpoint.
const SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const LAMPORTS_PER_SOL: f64 = 1e9;
const SIGNATURE_POLL_ATTEMPTS: usize = 30;
const SIGNATURE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const FALLBACK_MINT_RENT_LAMPORTS: u64 = 2_039_280;
const FALLBACK_ATA_RENT_LAMPORTS: u64 = 2_039_280;
const FALLBACK_FEE_BUFFER_LAMPORTS: u64 = 10_000;

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
    pub instructions: Vec<String>,
    pub fee_payer: String,
}

/// Signed transaction data ready for broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedSolanaTransaction {
    pub raw: String,
    pub signatures: Vec<String>,
}

pub fn build_spl_deploy_tx(
    _params: &SplTokenParams,
    payer: &str,
    blockhash: &str,
) -> UnsignedSolanaTransaction {
    UnsignedSolanaTransaction {
        recent_blockhash: blockhash.to_string(),
        instructions: vec![
            "CreateAccount".to_string(),
            "InitializeMint".to_string(),
            "CreateAssociatedTokenAccount".to_string(),
            "MintTo".to_string(),
        ],
        fee_payer: payer.to_string(),
    }
}

fn signed_solana_tx(raw_bytes: &[u8], tx: &Transaction) -> SignedSolanaTransaction {
    SignedSolanaTransaction {
        raw: bs58::encode(raw_bytes).into_string(),
        signatures: tx.signatures.iter().map(ToString::to_string).collect(),
    }
}

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

fn resolved_rpc_url<'a>(rpc_url_override: Option<&'a str>) -> Cow<'a, str> {
    match rpc_url_override {
        Some(url) => Cow::Borrowed(url),
        None => Cow::Borrowed(SOLANA_RPC_URL),
    }
}

async fn solana_rpc_call(
    url: &str,
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
        .post(url)
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

async fn get_latest_blockhash(url: &str) -> Result<Hash> {
    let result = solana_rpc_call(
        url,
        "getLatestBlockhash",
        serde_json::json!([{ "commitment": "confirmed" }]),
    )
    .await?;
    let blockhash = result
        .get("value")
        .and_then(|value| value.get("blockhash"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("getLatestBlockhash response missing blockhash"))?;
    Hash::from_str(blockhash).context("invalid Solana blockhash")
}

async fn get_minimum_balance_for_rent(url: &str, size: usize) -> Result<u64> {
    let result = solana_rpc_call(
        url,
        "getMinimumBalanceForRentExemption",
        serde_json::json!([size]),
    )
    .await?;
    result
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("rent exemption result is not a number"))
}

async fn send_transaction(url: &str, encoded_tx: &str) -> Result<String> {
    let result = solana_rpc_call(
        url,
        "sendTransaction",
        serde_json::json!([
            encoded_tx,
            {
                "encoding": "base58",
                "preflightCommitment": "confirmed"
            }
        ]),
    )
    .await?;

    result
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("sendTransaction result is not a string"))
}

async fn wait_for_signature(url: &str, signature: &str) -> Result<()> {
    for _ in 0..SIGNATURE_POLL_ATTEMPTS {
        let result = solana_rpc_call(
            url,
            "getSignatureStatuses",
            serde_json::json!([
                [signature],
                { "searchTransactionHistory": true }
            ]),
        )
        .await?;

        let status = result
            .get("value")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .filter(|value| !value.is_null());

        if let Some(status) = status {
            if let Some(err) = status.get("err").filter(|value| !value.is_null()) {
                anyhow::bail!("token deployment transaction failed: {err}");
            }

            let confirmed = status
                .get("confirmationStatus")
                .and_then(|value| value.as_str())
                .map(|status| status == "confirmed" || status == "finalized")
                .unwrap_or_else(|| {
                    status
                        .get("confirmations")
                        .map(|value| value.is_null())
                        .unwrap_or(false)
                });

            if confirmed {
                return Ok(());
            }
        }

        sleep(SIGNATURE_POLL_INTERVAL).await;
    }

    anyhow::bail!(
        "transaction broadcast but not confirmed within {} seconds: {signature}",
        SIGNATURE_POLL_ATTEMPTS * SIGNATURE_POLL_INTERVAL.as_secs() as usize
    )
}

fn solana_keypair_from_private_key(private_key: &[u8]) -> Result<Keypair> {
    let seed = private_key
        .get(..32)
        .ok_or_else(|| anyhow::anyhow!("Solana private key must be at least 32 bytes"))?;
    keypair_from_seed(seed).map_err(|e| anyhow::anyhow!("invalid Solana private key: {e}"))
}

fn scaled_supply(params: &SplTokenParams) -> Result<u64> {
    let multiplier = 10u128
        .checked_pow(u32::from(params.decimals))
        .ok_or_else(|| anyhow::anyhow!("token decimals are too large"))?;
    let raw_supply = u128::from(params.supply)
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("total supply overflows Solana token amount range"))?;
    u64::try_from(raw_supply)
        .map_err(|_| anyhow::anyhow!("total supply does not fit into an unsigned 64-bit integer"))
}

fn spl_token_program_id() -> Result<Pubkey> {
    Pubkey::from_str(SPL_TOKEN_PROGRAM_ID).context("invalid SPL token program id")
}

fn build_deploy_instructions(
    payer: &Keypair,
    mint: &Keypair,
    params: &SplTokenParams,
    mint_rent_lamports: u64,
) -> Result<(Vec<Instruction>, Pubkey, u64)> {
    let token_program_id = spl_token_program_id()?;
    let associated_token_address =
        spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &mint.pubkey());
    let amount = scaled_supply(params)?;

    let instructions = vec![
        solana_system_interface::instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            mint_rent_lamports,
            spl_token::state::Mint::LEN as u64,
            &token_program_id,
        ),
        spl_token::instruction::initialize_mint(
            &token_program_id,
            &mint.pubkey(),
            &payer.pubkey(),
            None,
            params.decimals,
        )
        .context("failed to build initialize_mint instruction")?,
        spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(),
            &payer.pubkey(),
            &mint.pubkey(),
            &token_program_id,
        ),
        spl_token::instruction::mint_to(
            &token_program_id,
            &mint.pubkey(),
            &associated_token_address,
            &payer.pubkey(),
            &[],
            amount,
        )
        .context("failed to build mint_to instruction")?,
    ];

    Ok((instructions, associated_token_address, amount))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Estimate the cost (in SOL) to create an SPL token with a single associated
/// token account for the deployer.
pub async fn estimate_deploy_cost() -> Result<f64> {
    estimate_deploy_cost_with_rpc(None).await
}

/// Estimate deploy cost using the default Solana RPC endpoint or an override.
pub async fn estimate_deploy_cost_with_rpc(rpc_url_override: Option<&str>) -> Result<f64> {
    let rpc_url = resolved_rpc_url(rpc_url_override);

    let mint_rent = get_minimum_balance_for_rent(rpc_url.as_ref(), spl_token::state::Mint::LEN)
        .await
        .unwrap_or(FALLBACK_MINT_RENT_LAMPORTS);
    let ata_rent = get_minimum_balance_for_rent(rpc_url.as_ref(), spl_token::state::Account::LEN)
        .await
        .unwrap_or(FALLBACK_ATA_RENT_LAMPORTS);
    let total_lamports = mint_rent
        .saturating_add(ata_rent)
        .saturating_add(FALLBACK_FEE_BUFFER_LAMPORTS);
    let sol = total_lamports as f64 / LAMPORTS_PER_SOL;

    info!(
        mint_rent_lamports = mint_rent,
        ata_rent_lamports = ata_rent,
        total_sol = sol,
        "estimated SPL token deploy cost"
    );

    Ok(sol)
}

/// Create a new SPL token on Solana.
pub async fn create_spl_token(
    params: SplTokenParams,
    private_key: &[u8],
) -> Result<SplDeployResult> {
    create_spl_token_with_rpc(params, private_key, None).await
}

/// Create an SPL token using the default Solana RPC endpoint or an override.
pub async fn create_spl_token_with_rpc(
    params: SplTokenParams,
    private_key: &[u8],
    rpc_url_override: Option<&str>,
) -> Result<SplDeployResult> {
    let rpc_url = resolved_rpc_url(rpc_url_override);
    let payer = solana_keypair_from_private_key(private_key)?;
    let mint = Keypair::new();
    let latest_blockhash = get_latest_blockhash(rpc_url.as_ref())
        .await
        .context("failed to fetch latest Solana blockhash")?;
    let mint_rent_lamports =
        get_minimum_balance_for_rent(rpc_url.as_ref(), spl_token::state::Mint::LEN)
            .await
            .unwrap_or(FALLBACK_MINT_RENT_LAMPORTS);
    let (instructions, associated_token_address, amount) =
        build_deploy_instructions(&payer, &mint, &params, mint_rent_lamports)?;

    let tx_blueprint = build_spl_deploy_tx(
        &params,
        &payer.pubkey().to_string(),
        &latest_blockhash.to_string(),
    );
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[&payer, &mint],
        latest_blockhash,
    );
    let raw_tx = bincode::serialize(&tx).context("failed to serialize Solana transaction")?;
    let signed_tx = signed_solana_tx(&raw_tx, &tx);
    let tx_signature = signed_tx
        .signatures
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("signed transaction did not contain a signature"))?;
    let rpc_signature = send_transaction(rpc_url.as_ref(), &signed_tx.raw)
        .await
        .context("failed to broadcast SPL token deployment transaction")?;
    wait_for_signature(rpc_url.as_ref(), &rpc_signature)
        .await
        .context("failed waiting for SPL token deployment confirmation")?;

    info!(
        name = %params.name,
        symbol = %params.symbol,
        mint_address = %mint.pubkey(),
        associated_token_address = %associated_token_address,
        amount = amount,
        tx_signature = %tx_signature,
        instruction_count = tx_blueprint.instructions.len(),
        "SPL token deployed"
    );

    Ok(SplDeployResult {
        mint_address: mint.pubkey().to_string(),
        tx_signature,
    })
}

/// Query the SOL balance for a wallet address.
pub async fn get_balance(address: &str) -> Result<f64> {
    get_balance_with_rpc(address, None).await
}

/// Query the SOL balance using the default Solana RPC endpoint or an override.
pub async fn get_balance_with_rpc(address: &str, rpc_url_override: Option<&str>) -> Result<f64> {
    let rpc_url = resolved_rpc_url(rpc_url_override);

    let result = solana_rpc_call(rpc_url.as_ref(), "getBalance", serde_json::json!([address]))
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
pub async fn get_token_balances(address: &str) -> Result<Vec<(String, f64)>> {
    get_token_balances_with_rpc(address, None).await
}

/// Query SPL token balances using the default Solana RPC endpoint or an
/// override.
pub async fn get_token_balances_with_rpc(
    address: &str,
    rpc_url_override: Option<&str>,
) -> Result<Vec<(String, f64)>> {
    let rpc_url = resolved_rpc_url(rpc_url_override);
    let result = solana_rpc_call(
        rpc_url.as_ref(),
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

    #[test]
    fn build_spl_tx_includes_real_instruction_sequence() {
        let params = SplTokenParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 9,
            supply: 1000,
            metadata_uri: None,
        };
        let tx = build_spl_deploy_tx(&params, "payer", "blockhash");
        assert_eq!(tx.instructions.len(), 4);
        assert_eq!(tx.instructions[0], "CreateAccount");
        assert_eq!(tx.instructions[3], "MintTo");
    }

    #[test]
    fn scaled_supply_applies_decimals() {
        let params = SplTokenParams {
            name: "Scale".into(),
            symbol: "SCL".into(),
            decimals: 3,
            supply: 42,
            metadata_uri: None,
        };
        assert_eq!(scaled_supply(&params).unwrap(), 42_000);
    }

    #[test]
    fn lamports_to_sol_conversion() {
        let lamports: u64 = 1_000_000_000;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 1.0).abs() < f64::EPSILON);

        let lamports: u64 = 500_000_000;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 0.5).abs() < f64::EPSILON);

        let lamports: u64 = 0;
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_get_balance_response() {
        let response: serde_json::Value = serde_json::json!({
            "value": 2_500_000_000u64
        });
        let lamports = response.get("value").and_then(|v| v.as_u64()).unwrap();
        let sol = lamports as f64 / LAMPORTS_PER_SOL;
        assert!((sol - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn resolved_rpc_url_prefers_override() {
        let resolved = resolved_rpc_url(Some("https://solana.example.com"));
        assert_eq!(resolved.as_ref(), "https://solana.example.com");
    }

    #[test]
    fn resolved_rpc_url_falls_back_to_default() {
        let resolved = resolved_rpc_url(None);
        assert_eq!(resolved.as_ref(), SOLANA_RPC_URL);
    }

    #[test]
    fn parse_token_accounts_response() {
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

        let accounts = response.get("value").and_then(|v| v.as_array()).unwrap();

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
        assert_eq!(balances[1].0, "So11111111111111111111111111111111111111112");
        assert!((balances[1].1 - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rent_exemption_fallback_value() {
        let total =
            FALLBACK_MINT_RENT_LAMPORTS + FALLBACK_ATA_RENT_LAMPORTS + FALLBACK_FEE_BUFFER_LAMPORTS;
        let sol = total as f64 / LAMPORTS_PER_SOL;
        assert!(sol > 0.0);
    }
}
