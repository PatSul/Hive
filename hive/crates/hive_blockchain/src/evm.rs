use std::borrow::Cow;

use anyhow::{Context, Result};
use ethabi::{Token, ethereum_types::U256};
use k256::ecdsa::{RecoveryId, Signature, SigningKey};
use rlp::RlpStream;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use tokio::time::{Duration, sleep};
use tracing::{info, warn};

use crate::wallet_store::Chain;

const FALLBACK_DEPLOY_GAS: u64 = 1_500_000;
const DEFAULT_GAS_PRICE_WEI: u128 = 20_000_000_000;
const WEI_PER_ETH: f64 = 1e18;
const RECEIPT_POLL_ATTEMPTS: usize = 30;
const RECEIPT_POLL_INTERVAL: Duration = Duration::from_secs(2);

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

/// Infrastructure for an unsigned EVM transaction (EIP-155 legacy format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedEvmTransaction {
    pub chain_id: u64,
    pub nonce: u64,
    pub to: Option<String>,
    pub value: String, // hex
    pub data: String,  // hex for bytecode/calldata
    pub gas_limit: u64,
    pub gas_price: String,
    /// Unused in legacy (EIP-155) encoding; kept for forward-compatibility
    /// with EIP-1559 transactions.
    #[serde(default)]
    pub _priority_fee: String,
}

/// Signed raw transaction data ready for broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEvmTransaction {
    pub raw: String,
    pub hash: String,
}

/// Build an unsigned ERC-20 deploy transaction.
pub fn build_erc20_deploy_tx(
    params: &TokenDeployParams,
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
) -> Result<UnsignedEvmTransaction> {
    Ok(UnsignedEvmTransaction {
        chain_id: params.chain.chain_id(),
        nonce,
        to: None,
        value: "0x0".to_string(),
        data: build_erc20_deploy_data(params)?,
        gas_limit,
        gas_price: format!("0x{:x}", gas_price),
        _priority_fee: String::new(),
    })
}

/// Sign a legacy EIP-155 transaction using the provided 32-byte private key.
pub fn sign_evm_tx(
    tx: &UnsignedEvmTransaction,
    private_key: &[u8],
) -> Result<SignedEvmTransaction> {
    let signing_key = signing_key_from_private_key(private_key)?;
    let signing_payload = encode_unsigned_legacy_tx(tx)?;
    let (signature, recovery_id) = signing_key
        .sign_digest_recoverable(Keccak256::new_with_prefix(signing_payload))
        .context("failed to sign EVM deployment transaction")?;
    let raw_tx = encode_signed_legacy_tx(tx, &signature, recovery_id)?;
    let tx_hash = format!("0x{}", hex_encode(Keccak256::digest(&raw_tx).as_slice()));

    Ok(SignedEvmTransaction {
        raw: format!("0x{}", hex_encode(&raw_tx)),
        hash: tx_hash,
    })
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

fn resolved_rpc_url<'a>(chain: Chain, rpc_url_override: Option<&'a str>) -> Cow<'a, str> {
    match rpc_url_override {
        Some(url) => Cow::Borrowed(url),
        None => Cow::Borrowed(rpc_url(chain)),
    }
}

/// Execute a JSON-RPC call against the given URL.
///
/// Builds a `{"jsonrpc":"2.0","method":...,"params":...,"id":1}` request,
/// POSTs it, and returns the `"result"` field from the response.
async fn rpc_call(url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
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
    u128::from_str_radix(stripped, 16).with_context(|| format!("failed to parse hex value: {hex}"))
}

fn parse_hex_u64(hex: &str) -> Result<u64> {
    let value = parse_hex_u128(hex)?;
    u64::try_from(value).with_context(|| format!("hex value does not fit into u64: {hex}"))
}

fn parse_hex_u256(hex: &str) -> Result<U256> {
    let stripped = hex.strip_prefix("0x").unwrap_or(hex);
    U256::from_str_radix(stripped, 16)
        .with_context(|| format!("failed to parse U256 hex value: {hex}"))
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    if stripped.len() % 2 != 0 {
        anyhow::bail!("hex string must contain an even number of characters");
    }

    let mut bytes = Vec::with_capacity(stripped.len() / 2);
    for chunk in stripped.as_bytes().chunks_exact(2) {
        let hi = hex_nibble(chunk[0] as char)?;
        let lo = hex_nibble(chunk[1] as char)?;
        bytes.push((hi << 4) | lo);
    }
    Ok(bytes)
}

fn hex_nibble(ch: char) -> Result<u8> {
    match ch {
        '0'..='9' => Ok((ch as u8) - b'0'),
        'a'..='f' => Ok((ch as u8) - b'a' + 10),
        'A'..='F' => Ok((ch as u8) - b'A' + 10),
        _ => anyhow::bail!("invalid hex character `{ch}`"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn trim_be_bytes(bytes: &[u8]) -> &[u8] {
    let first_non_zero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    &bytes[first_non_zero..]
}

fn u256_to_trimmed_bytes(value: U256) -> Vec<u8> {
    let mut buffer = [0u8; 32];
    value.to_big_endian(&mut buffer);
    trim_be_bytes(&buffer).to_vec()
}

fn append_u64(stream: &mut RlpStream, value: u64) {
    if value == 0 {
        stream.append_empty_data();
        return;
    }

    let encoded = value.to_be_bytes();
    let bytes = trim_be_bytes(&encoded);
    stream.append(&bytes);
}

fn append_u256(stream: &mut RlpStream, value: U256) {
    let bytes = u256_to_trimmed_bytes(value);
    if bytes.is_empty() {
        stream.append_empty_data();
    } else {
        stream.append(&bytes);
    }
}

fn signing_key_from_private_key(private_key: &[u8]) -> Result<SigningKey> {
    let key_bytes = private_key
        .get(..32)
        .ok_or_else(|| anyhow::anyhow!("EVM private key must be at least 32 bytes"))?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("EVM private key must be exactly 32 bytes"))?;
    SigningKey::from_bytes((&key_array).into())
        .map_err(|e| anyhow::anyhow!("invalid EVM private key: {e}"))
}

fn private_key_to_address(private_key: &[u8]) -> Result<String> {
    let signing_key = signing_key_from_private_key(private_key)?;
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let public_key = encoded.as_bytes();
    let hash = Keccak256::digest(&public_key[1..]);
    Ok(format!("0x{}", hex_encode(&hash[12..])))
}

fn build_erc20_deploy_data(params: &TokenDeployParams) -> Result<String> {
    let contract = crate::erc20_bytecode::get_erc20_contract();
    let mut deployment_data =
        decode_hex(&contract.bytecode).context("failed to decode ERC-20 bytecode")?;
    let total_supply = U256::from_dec_str(params.total_supply.trim())
        .with_context(|| format!("invalid total supply: {}", params.total_supply))?;
    let constructor_args = ethabi::encode(&[
        Token::String(params.name.clone()),
        Token::String(params.symbol.clone()),
        Token::Uint(U256::from(params.decimals)),
        Token::Uint(total_supply),
    ]);
    deployment_data.extend_from_slice(&constructor_args);
    Ok(format!("0x{}", hex_encode(&deployment_data)))
}

fn build_sample_deploy_data() -> Result<String> {
    build_erc20_deploy_data(&TokenDeployParams {
        name: "Hive Token".to_string(),
        symbol: "HIVE".to_string(),
        decimals: 18,
        total_supply: "1000000".to_string(),
        chain: Chain::Ethereum,
    })
}

fn encode_unsigned_legacy_tx(tx: &UnsignedEvmTransaction) -> Result<Vec<u8>> {
    let gas_price = parse_hex_u256(&tx.gas_price)?;
    let value = parse_hex_u256(&tx.value)?;
    let data = decode_hex(&tx.data)?;

    let mut stream = RlpStream::new_list(9);
    append_u64(&mut stream, tx.nonce);
    append_u256(&mut stream, gas_price);
    append_u64(&mut stream, tx.gas_limit);
    stream.append_empty_data();
    append_u256(&mut stream, value);
    stream.append(&data);
    append_u64(&mut stream, tx.chain_id);
    stream.append_empty_data();
    stream.append_empty_data();
    Ok(stream.out().to_vec())
}

fn encode_signed_legacy_tx(
    tx: &UnsignedEvmTransaction,
    signature: &Signature,
    recovery_id: RecoveryId,
) -> Result<Vec<u8>> {
    let gas_price = parse_hex_u256(&tx.gas_price)?;
    let value = parse_hex_u256(&tx.value)?;
    let data = decode_hex(&tx.data)?;
    let signature_bytes = signature.to_bytes();
    let r = U256::from_big_endian(&signature_bytes[..32]);
    let s = U256::from_big_endian(&signature_bytes[32..]);
    let v = tx.chain_id * 2 + 35 + u64::from(recovery_id.is_y_odd());

    let mut stream = RlpStream::new_list(9);
    append_u64(&mut stream, tx.nonce);
    append_u256(&mut stream, gas_price);
    append_u64(&mut stream, tx.gas_limit);
    stream.append_empty_data();
    append_u256(&mut stream, value);
    stream.append(&data);
    append_u64(&mut stream, v);
    append_u256(&mut stream, r);
    append_u256(&mut stream, s);
    Ok(stream.out().to_vec())
}

fn derive_contract_address(sender: &str, nonce: u64) -> Result<String> {
    let sender_bytes = decode_hex(sender).context("failed to decode sender address")?;
    if sender_bytes.len() != 20 {
        anyhow::bail!("EVM sender address must be 20 bytes");
    }

    let mut stream = RlpStream::new_list(2);
    stream.append(&sender_bytes);
    append_u64(&mut stream, nonce);
    let encoded = stream.out();
    let hash = Keccak256::digest(encoded.as_ref());
    Ok(format!("0x{}", hex_encode(&hash[12..])))
}

async fn fetch_chain_id(url: &str) -> Result<u64> {
    let result = rpc_call(url, "eth_chainId", serde_json::json!([])).await?;
    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_chainId result is not a string"))?;
    parse_hex_u64(hex)
}

async fn fetch_nonce(url: &str, address: &str) -> Result<u64> {
    let result = rpc_call(
        url,
        "eth_getTransactionCount",
        serde_json::json!([address, "pending"]),
    )
    .await?;
    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_getTransactionCount result is not a string"))?;
    parse_hex_u64(hex)
}

async fn fetch_gas_price(url: &str) -> Result<u128> {
    let result = rpc_call(url, "eth_gasPrice", serde_json::json!([])).await?;
    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_gasPrice result is not a string"))?;
    parse_hex_u128(hex)
}

async fn estimate_deploy_gas(url: &str, from: &str, data: &str) -> Result<u64> {
    let result = rpc_call(
        url,
        "eth_estimateGas",
        serde_json::json!([{ "from": from, "data": data }]),
    )
    .await?;
    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_estimateGas result is not a string"))?;
    let estimated = parse_hex_u64(hex)?;
    Ok(estimated.saturating_mul(12) / 10)
}

async fn send_raw_transaction(url: &str, raw_tx: &str) -> Result<String> {
    let result = rpc_call(url, "eth_sendRawTransaction", serde_json::json!([raw_tx])).await?;
    result
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("eth_sendRawTransaction result is not a string"))
}

async fn wait_for_receipt(url: &str, tx_hash: &str) -> Result<serde_json::Value> {
    for _ in 0..RECEIPT_POLL_ATTEMPTS {
        let receipt = rpc_call(
            url,
            "eth_getTransactionReceipt",
            serde_json::json!([tx_hash]),
        )
        .await?;
        if !receipt.is_null() {
            return Ok(receipt);
        }
        sleep(RECEIPT_POLL_INTERVAL).await;
    }

    anyhow::bail!(
        "transaction broadcast but not confirmed within {} seconds: {tx_hash}",
        RECEIPT_POLL_ATTEMPTS * RECEIPT_POLL_INTERVAL.as_secs() as usize
    )
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Estimate the cost (in native currency) to deploy an ERC-20 contract.
pub async fn estimate_deploy_cost(chain: Chain) -> Result<f64> {
    estimate_deploy_cost_with_rpc(chain, None).await
}

/// Estimate the cost (in native currency) to deploy an ERC-20 contract using
/// an optional custom RPC endpoint.
pub async fn estimate_deploy_cost_with_rpc(
    chain: Chain,
    rpc_url_override: Option<&str>,
) -> Result<f64> {
    if let Some(url) = rpc_url_override {
        if !crate::rpc_config::validate_url(url) {
            anyhow::bail!("Invalid RPC URL: must be HTTPS and not target private IPs");
        }
    }
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }

    let url = resolved_rpc_url(chain, rpc_url_override);
    let sample_data = build_sample_deploy_data()?;

    match fetch_gas_price(url.as_ref()).await {
        Ok(gas_price) => {
            let gas_limit = estimate_deploy_gas(
                url.as_ref(),
                "0x000000000000000000000000000000000000dEaD",
                &sample_data,
            )
            .await
            .unwrap_or(FALLBACK_DEPLOY_GAS);
            let cost_wei = gas_price * u128::from(gas_limit);
            let cost = cost_wei as f64 / WEI_PER_ETH;

            info!(
                chain = %chain,
                gas_price_gwei = gas_price as f64 / 1e9,
                gas_limit = gas_limit,
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
            let cost = match chain {
                Chain::Ethereum => 0.015,
                Chain::Base => 0.0001,
                Chain::Solana => unreachable!(),
            };
            Ok(cost)
        }
    }
}

/// Deploy an ERC-20 token to the specified EVM chain.
pub async fn deploy_token(params: TokenDeployParams, private_key: &[u8]) -> Result<DeployResult> {
    deploy_token_with_rpc(params, private_key, None).await
}

/// Deploy an ERC-20 token using the configured chain RPC or a caller-provided
/// override.
pub async fn deploy_token_with_rpc(
    params: TokenDeployParams,
    private_key: &[u8],
    rpc_url_override: Option<&str>,
) -> Result<DeployResult> {
    if let Some(url) = rpc_url_override {
        if !crate::rpc_config::validate_url(url) {
            anyhow::bail!("Invalid RPC URL: must be HTTPS and not target private IPs");
        }
    }
    if !params.chain.is_evm() {
        anyhow::bail!("{} is not an EVM chain", params.chain);
    }

    let url = resolved_rpc_url(params.chain, rpc_url_override);
    let rpc_chain_id = fetch_chain_id(url.as_ref())
        .await
        .context("failed to resolve RPC chain id")?;
    if rpc_chain_id != params.chain.chain_id() {
        anyhow::bail!(
            "RPC endpoint chain id mismatch: expected {}, got {}",
            params.chain.chain_id(),
            rpc_chain_id
        );
    }

    let sender = private_key_to_address(private_key)?;
    let nonce = fetch_nonce(url.as_ref(), &sender)
        .await
        .context("failed to fetch deployer nonce")?;
    let gas_price = fetch_gas_price(url.as_ref())
        .await
        .unwrap_or(DEFAULT_GAS_PRICE_WEI);
    let deploy_data = build_erc20_deploy_data(&params)?;
    let gas_limit = estimate_deploy_gas(url.as_ref(), &sender, &deploy_data)
        .await
        .unwrap_or(FALLBACK_DEPLOY_GAS);
    let tx = build_erc20_deploy_tx(&params, nonce, gas_price, gas_limit)?;
    let signed_tx = sign_evm_tx(&tx, private_key)?;
    let tx_hash = send_raw_transaction(url.as_ref(), &signed_tx.raw)
        .await
        .context("failed to broadcast deployment transaction")?;
    let predicted_contract_address = derive_contract_address(&sender, nonce)?;
    let receipt = wait_for_receipt(url.as_ref(), &tx_hash)
        .await
        .context("failed waiting for deployment receipt")?;

    let status = receipt
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("0x1");
    if status == "0x0" {
        anyhow::bail!("deployment transaction reverted: {tx_hash}");
    }

    let contract_address = receipt
        .get("contractAddress")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(predicted_contract_address.as_str())
        .to_string();
    let gas_used = receipt
        .get("gasUsed")
        .and_then(|value| value.as_str())
        .map(parse_hex_u64)
        .transpose()?
        .unwrap_or(gas_limit);
    let cost_eth = (gas_price * u128::from(gas_used)) as f64 / WEI_PER_ETH;

    info!(
        chain = %params.chain,
        name = %params.name,
        symbol = %params.symbol,
        tx_hash = %tx_hash,
        contract_address = %contract_address,
        estimated_cost = cost_eth,
        "ERC-20 token deployed"
    );

    Ok(DeployResult {
        tx_hash,
        contract_address,
        chain: params.chain,
        gas_used,
    })
}

/// Query the native currency balance for an address on the given chain.
///
/// Issues an `eth_getBalance` JSON-RPC call and converts the result from
/// wei to ETH (or the chain's native token).
pub async fn get_balance(address: &str, chain: Chain) -> Result<f64> {
    get_balance_with_rpc(address, chain, None).await
}

/// Query the native currency balance using the configured chain RPC or a
/// caller-provided override.
pub async fn get_balance_with_rpc(
    address: &str,
    chain: Chain,
    rpc_url_override: Option<&str>,
) -> Result<f64> {
    if let Some(url) = rpc_url_override {
        if !crate::rpc_config::validate_url(url) {
            anyhow::bail!("Invalid RPC URL: must be HTTPS and not target private IPs");
        }
    }
    if !chain.is_evm() {
        anyhow::bail!("{chain} is not an EVM chain");
    }

    let url = resolved_rpc_url(chain, rpc_url_override);
    let result = rpc_call(
        url.as_ref(),
        "eth_getBalance",
        serde_json::json!([address, "latest"]),
    )
    .await
    .context("eth_getBalance RPC call failed")?;

    let hex = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("eth_getBalance result is not a string"))?;
    let wei = parse_hex_u128(hex)?;
    let balance = wei as f64 / WEI_PER_ETH;

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

    #[test]
    fn deploy_data_includes_compiled_bytecode_and_constructor_args() {
        let params = TokenDeployParams {
            name: "Test".into(),
            symbol: "T".into(),
            decimals: 18,
            total_supply: "1000".into(),
            chain: Chain::Ethereum,
        };
        let contract = crate::erc20_bytecode::get_erc20_contract();
        let data = build_erc20_deploy_data(&params).unwrap();
        assert!(data.starts_with(contract.bytecode.as_str()));
        assert!(data.len() > contract.bytecode.len());
    }

    #[test]
    fn signing_is_deterministic_for_same_transaction() {
        let tx = UnsignedEvmTransaction {
            chain_id: 1,
            nonce: 0,
            to: None,
            value: "0x0".into(),
            data: build_sample_deploy_data().unwrap(),
            gas_limit: FALLBACK_DEPLOY_GAS,
            gas_price: format!("0x{:x}", DEFAULT_GAS_PRICE_WEI),
            _priority_fee: String::new(),
        };
        let private_key =
            decode_hex("0x4c0883a69102937d6231471b5dbb6204fe5129617082790f8b1a4d7a8b798f8f")
                .unwrap();
        let first = sign_evm_tx(&tx, &private_key).unwrap();
        let second = sign_evm_tx(&tx, &private_key).unwrap();
        assert_eq!(first.raw, second.raw);
        assert_eq!(first.hash, second.hash);
        assert!(first.raw.starts_with("0x"));
        assert!(first.hash.starts_with("0x"));
    }

    #[test]
    fn parse_hex_u128_works() {
        assert_eq!(parse_hex_u128("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u128("0x1").unwrap(), 1);
        assert_eq!(parse_hex_u128("0xa").unwrap(), 10);
        assert_eq!(parse_hex_u128("0xff").unwrap(), 255);
        assert_eq!(parse_hex_u128("0x4a817c800").unwrap(), 20_000_000_000);
    }

    #[test]
    fn parse_hex_balance_conversion() {
        let wei = parse_hex_u128("0xDE0B6B3A7640000").unwrap();
        let eth = wei as f64 / WEI_PER_ETH;
        assert!((eth - 1.0).abs() < 1e-10);
    }

    #[test]
    fn parse_hex_zero_balance() {
        let wei = parse_hex_u128("0x0").unwrap();
        let eth = wei as f64 / WEI_PER_ETH;
        assert!((eth - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gas_estimation_math() {
        let gas_price: u128 = 30_000_000_000;
        let deploy_gas: u128 = 1_500_000;
        let cost_wei = gas_price * deploy_gas;
        let cost_eth = cost_wei as f64 / WEI_PER_ETH;
        assert!((cost_eth - 0.045).abs() < 1e-10);
    }

    #[test]
    fn resolved_rpc_url_prefers_override() {
        let resolved = resolved_rpc_url(Chain::Ethereum, Some("https://rpc.example.com"));
        assert_eq!(resolved.as_ref(), "https://rpc.example.com");
    }

    #[test]
    fn resolved_rpc_url_falls_back_to_default() {
        let resolved = resolved_rpc_url(Chain::Base, None);
        assert_eq!(resolved.as_ref(), "https://mainnet.base.org");
    }

    #[test]
    fn rpc_url_returns_valid_endpoints() {
        assert!(rpc_url(Chain::Ethereum).starts_with("https://"));
        assert!(rpc_url(Chain::Base).starts_with("https://"));
    }

    #[test]
    fn contract_address_derivation_returns_evm_address() {
        let sender = "0x4b7dc0e1c40244f4f4fdf0f7af4ff4a4e9f6d1d1";
        let address = derive_contract_address(sender, 0).unwrap();
        assert!(address.starts_with("0x"));
        assert_eq!(address.len(), 42);
    }

    #[tokio::test]
    async fn get_balance_rejects_solana() {
        let result = get_balance("0x0000", Chain::Solana).await;
        assert!(result.is_err());
    }
}
