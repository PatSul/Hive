use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::wallet_store::Chain;

/// An EVM-compatible wallet address and its chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmWallet {
    pub address: String,
    pub chain: Chain,
}

/// Parameters for deploying a new ERC-20 token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenDeployParams {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
    pub chain: Chain,
}

/// Result of a successful token deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub tx_hash: String,
    pub contract_address: String,
    pub chain: Chain,
    pub gas_used: u64,
}

/// Infrastructure for an unsigned EVM transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedEvmTransaction {
    pub chain_id: u64,
    pub nonce: u64,
    pub to: Option<String>,
    pub value: String, // hex
    pub data: String, // hex for bytecode/calldata
    pub gas_limit: u64,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
}

/// Simulated signed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEvmTransaction {
    pub raw: String,
    pub hash: String,
}

/// Build an unsigned ERC-20 deploy transaction
pub fn build_erc20_deploy_tx(params: &TokenDeployParams, nonce: u64, gas_price: u128) -> UnsignedEvmTransaction {
    UnsignedEvmTransaction {
        chain_id: params.chain.chain_id(),
        nonce,
        to: None, 
        value: "0x0".to_string(),
        data: crate::erc20_bytecode::get_erc20_contract().bytecode,
        gas_limit: 1_500_000,
        max_fee_per_gas: format!("0x{:x}", gas_price),
        max_priority_fee_per_gas: format!("0x{:x}", gas_price),
    }
}

/// Simulate signing an EVM transaction
pub fn sign_evm_tx_simulated(tx: &UnsignedEvmTransaction, _private_key: &[u8]) -> SignedEvmTransaction {
    let mut hasher = DefaultHasher::new();
    tx.chain_id.hash(&mut hasher);
    tx.nonce.hash(&mut hasher);
    let hash_val = hasher.finish();
    let tx_hash = format!("0x{hash_val:016x}{hash_val:016x}{hash_val:016x}{hash_val:016x}");
    SignedEvmTransaction {
        raw: format!("0xSIMULATED_SIGNED_{:x}", hash_val),
        hash: tx_hash,
    }
}

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

/// Return a public RPC endpoint URL for the given EVM chain.
fn rpc_url(chain: Chain) -> &'static str {
    match chain {
        Chain::Ethereum => "https://eth.llamarpc.com",
        Chain::Base => "https://mainnet.base.org",
        Chain::Solana => unreachable!("Solana is not an EVM chain"),
    }
}

/// Execute a JSON-RPC call against the given URL.
///
/// Builds a `{"jsonrpc":"2.0","method":...,"params":...,"id":1}` request,
/// POSTs it, and returns the `"result"` field from the response.
async fn rpc_call(
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
        .context("JSON-RPC request failed")?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .context("failed to read JSON-RPC response body")?;

    if !status.is_success() {
        anyhow::bail!("JSON-RPC HTTP error {status}: {text}");
    }

    let json: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse JSON-RPC response")?;

    if let Some(err) = json.get("error") {
        anyhow::bail!("JSON-RPC error: {err}");
    }

    json.get("result")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("JSON-RPC response missing 'result' field"))
}

/// Parse a `0x`-prefixed hex string into a `u128`.
fn parse_hex_u128(hex: &str) -> Result<u128> {
    let stripped = hex.strip_prefix("0x").unwrap_or(hex);
    u128::from_str_radix(stripped, 16)
        .with_context(|| format!("failed to parse hex value: {hex}"))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Estimate the cost (in native currency) to deploy an ERC-20 contract.
///
/// Queries the chain's `eth_gasPrice` and multiplies by a typical ERC-20
/// deployment gas estimate (~1,500,000 gas). Falls back to reasonable
/// defaults if the RPC call fails.
pub async fn estimate_deploy_cost(chain: Chain) -> Result<f64> {
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }

    const DEPLOY_GAS: u128 = 1_500_000;
    const WEI_PER_ETH: f64 = 1e18;

    let url = rpc_url(chain);

    match rpc_call(url, "eth_gasPrice", serde_json::json!([])).await {
        Ok(result) => {
            let hex = result
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("eth_gasPrice result is not a string"))?;
            let gas_price = parse_hex_u128(hex)?;
            let cost_wei = gas_price * DEPLOY_GAS;
            let cost = cost_wei as f64 / WEI_PER_ETH;

            info!(
                chain = %chain,
                gas_price_gwei = gas_price as f64 / 1e9,
                estimated_cost = cost,
                "estimated ERC-20 deploy cost"
            );
            Ok(cost)
        }
        Err(e) => {
            warn!(
                chain = %chain,
                error = %e,
                "failed to fetch gas price, using fallback estimate"
            );
            // Fallback to reasonable defaults.
            let cost = match chain {
                Chain::Ethereum => 0.015,
                Chain::Base => 0.0001,
                Chain::Solana => unreachable!(),
            };
            Ok(cost)
        }
    }
}

/// Deploy an ERC-20 token to the specified EVM chain (simulation mode).
///
/// Because a real on-chain deployment requires a private key and signed
/// transaction, this function operates in *simulation mode*: it queries
/// the live gas price, computes a realistic cost estimate, and returns a
/// [`DeployResult`] with a deterministic placeholder transaction hash.
/// Actual on-chain execution requires wallet signing.
pub async fn deploy_token(params: TokenDeployParams, _private_key: &[u8]) -> Result<DeployResult> {
    if !params.chain.is_evm() {
        anyhow::bail!("{} is not an EVM chain", params.chain);
    }

    const DEPLOY_GAS: u64 = 1_500_000;

    // Build a deterministic placeholder tx hash from config fields (for address).
    let mut hasher = DefaultHasher::new();
    params.name.hash(&mut hasher);
    params.symbol.hash(&mut hasher);
    params.chain.label().hash(&mut hasher);
    let hash_val = hasher.finish();
    let contract_address = format!("0x{:040x}", hash_val as u128);

    // Attempt to fetch the real gas price for cost estimation.
    let url = rpc_url(params.chain);
    let gas_price = match rpc_call(url, "eth_gasPrice", serde_json::json!([])).await {
        Ok(result) => {
            if let Some(hex) = result.as_str() {
                parse_hex_u128(hex).unwrap_or(20_000_000_000) // 20 gwei fallback
            } else {
                20_000_000_000
            }
        }
        Err(_) => 20_000_000_000, // 20 gwei fallback
    };

    // Integrate transaction building infrastructure
    let tx = build_erc20_deploy_tx(&params, 0, gas_price);
    let signed_tx = sign_evm_tx_simulated(&tx, _private_key);

    let tx_hash = signed_tx.hash;

    let cost_wei = gas_price * DEPLOY_GAS as u128;
    let cost_eth = cost_wei as f64 / 1e18;

    info!(
        chain = %params.chain,
        name = %params.name,
        symbol = %params.symbol,
        estimated_cost = cost_eth,
        "Token deployment prepared — wallet signing required for on-chain execution"
    );

    Ok(DeployResult {
        tx_hash,
        contract_address,
        chain: params.chain,
        gas_used: DEPLOY_GAS,
    })
}

/// Query the native currency balance for an address on the given chain.
///
/// Issues an `eth_getBalance` JSON-RPC call and converts the result from
/// wei to ETH (or the chain's native token).
pub async fn get_balance(address: &str, chain: Chain) -> Result<f64> {
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }

    let url = rpc_url(chain);
    let result = rpc_call(
        url,
        "eth_getBalance",
        serde_json::json!([address, "latest"]),
    )
    .await
    .context("eth_getBalance RPC call failed")?;

    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_getBalance result is not a string"))?;
    let wei = parse_hex_u128(hex)?;
    let balance = wei as f64 / 1e18;

    info!(chain = %chain, address = %address, balance = balance, "fetched EVM balance");
    Ok(balance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_deploy_params_serializes() {
        let params = TokenDeployParams {
            name: "TestToken".into(),
            symbol: "TT".into(),
            decimals: 18,
            total_supply: "1000000000000000000000000".into(),
            chain: Chain::Ethereum,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: TokenDeployParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "TestToken");
        assert_eq!(parsed.decimals, 18);
    }

    #[test]
    fn deploy_result_serializes() {
        let result = DeployResult {
            tx_hash: "0xabc123".into(),
            contract_address: "0xdef456".into(),
            chain: Chain::Base,
            gas_used: 1_200_000,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0xabc123"));
        let parsed: DeployResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.gas_used, 1_200_000);
    }

    #[test]
    fn evm_wallet_serializes() {
        let wallet = EvmWallet {
            address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
            chain: Chain::Ethereum,
        };
        let json = serde_json::to_string(&wallet).unwrap();
        let parsed: EvmWallet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.address, wallet.address);
    }

    #[tokio::test]
    async fn estimate_deploy_cost_returns_placeholder() {
        let cost = estimate_deploy_cost(Chain::Ethereum).await.unwrap();
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn estimate_deploy_cost_rejects_solana() {
        let result = estimate_deploy_cost(Chain::Solana).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn deploy_token_returns_simulation_result() {
        let params = TokenDeployParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 18,
            total_supply: "1000".into(),
            chain: Chain::Ethereum,
        };
        let result = deploy_token(params, &[0u8; 32]).await;
        // Simulation mode should succeed (not bail!).
        assert!(result.is_ok());
        let deploy = result.unwrap();
        assert!(deploy.tx_hash.starts_with("0x"));
        assert!(deploy.contract_address.starts_with("0x"));
        assert_eq!(deploy.gas_used, 1_500_000);
    }

    #[tokio::test]
    async fn deploy_token_deterministic_hash() {
        let make_params = || TokenDeployParams {
            name: "Alpha".into(),
            symbol: "ALPHA".into(),
            decimals: 18,
            total_supply: "1000000".into(),
            chain: Chain::Base,
        };
        let r1 = deploy_token(make_params(), &[0u8; 32]).await.unwrap();
        let r2 = deploy_token(make_params(), &[0u8; 32]).await.unwrap();
        assert_eq!(r1.tx_hash, r2.tx_hash);
        assert_eq!(r1.contract_address, r2.contract_address);
    }

    // --- New RPC-focused unit tests ---

    #[test]
    fn parse_hex_u128_works() {
        assert_eq!(parse_hex_u128("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u128("0x1").unwrap(), 1);
        assert_eq!(parse_hex_u128("0xa").unwrap(), 10);
        assert_eq!(parse_hex_u128("0xff").unwrap(), 255);
        // Typical gas price ~20 gwei = 20_000_000_000
        assert_eq!(
            parse_hex_u128("0x4a817c800").unwrap(),
            20_000_000_000
        );
    }

    #[test]
    fn parse_hex_balance_conversion() {
        // 1 ETH = 1e18 wei = 0xDE0B6B3A7640000
        let wei = parse_hex_u128("0xDE0B6B3A7640000").unwrap();
        let eth = wei as f64 / 1e18;
        assert!((eth - 1.0).abs() < 1e-10);
    }

    #[test]
    fn parse_hex_zero_balance() {
        let wei = parse_hex_u128("0x0").unwrap();
        let eth = wei as f64 / 1e18;
        assert!((eth - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gas_estimation_math() {
        // Simulate: 30 gwei gas price, 1.5M gas
        let gas_price: u128 = 30_000_000_000; // 30 gwei
        let deploy_gas: u128 = 1_500_000;
        let cost_wei = gas_price * deploy_gas;
        let cost_eth = cost_wei as f64 / 1e18;
        // 30 gwei * 1.5M = 45_000 gwei = 0.000045 ETH
        assert!((cost_eth - 0.045).abs() < 1e-10);
    }

    #[test]
    fn rpc_url_returns_valid_endpoints() {
        assert!(rpc_url(Chain::Ethereum).starts_with("https://"));
        assert!(rpc_url(Chain::Base).starts_with("https://"));
    }

    #[tokio::test]
    async fn get_balance_rejects_solana() {
        let result = get_balance("0x0000", Chain::Solana).await;
        assert!(result.is_err());
    }
}
