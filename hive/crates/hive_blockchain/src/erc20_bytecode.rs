use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::wallet_store::Chain;

/// Pre-compiled ERC-20 contract ABI and bytecode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc20Contract {
    pub abi: serde_json::Value,
    pub bytecode: String,
}

/// Returns a minimal ERC-20 contract with a standard ABI and compiled bytecode.
///
/// The embedded artifact was compiled locally from a minimal constructor-minted
/// ERC-20 implementation using `solc 0.8.34` with optimization enabled.
pub fn get_erc20_contract() -> Erc20Contract {
    let abi = serde_json::json!([
        {
            "type": "constructor",
            "inputs": [
                { "name": "name_", "type": "string" },
                { "name": "symbol_", "type": "string" },
                { "name": "decimals_", "type": "uint8" },
                { "name": "totalSupply_", "type": "uint256" }
            ]
        },
        {
            "type": "event",
            "name": "Approval",
            "anonymous": false,
            "inputs": [
                { "indexed": true, "name": "owner", "type": "address" },
                { "indexed": true, "name": "spender", "type": "address" },
                { "indexed": false, "name": "value", "type": "uint256" }
            ]
        },
        {
            "type": "event",
            "name": "Transfer",
            "anonymous": false,
            "inputs": [
                { "indexed": true, "name": "from", "type": "address" },
                { "indexed": true, "name": "to", "type": "address" },
                { "indexed": false, "name": "value", "type": "uint256" }
            ]
        },
        {
            "type": "function",
            "name": "allowance",
            "inputs": [
                { "name": "", "type": "address" },
                { "name": "", "type": "address" }
            ],
            "outputs": [{ "name": "", "type": "uint256" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "name",
            "inputs": [],
            "outputs": [{ "name": "", "type": "string" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "symbol",
            "inputs": [],
            "outputs": [{ "name": "", "type": "string" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "decimals",
            "inputs": [],
            "outputs": [{ "name": "", "type": "uint8" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "totalSupply",
            "inputs": [],
            "outputs": [{ "name": "", "type": "uint256" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "balanceOf",
            "inputs": [{ "name": "account", "type": "address" }],
            "outputs": [{ "name": "", "type": "uint256" }],
            "stateMutability": "view"
        },
        {
            "type": "function",
            "name": "approve",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        },
        {
            "type": "function",
            "name": "transfer",
            "inputs": [
                { "name": "to", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        },
        {
            "type": "function",
            "name": "transferFrom",
            "inputs": [
                { "name": "from", "type": "address" },
                { "name": "to", "type": "address" },
                { "name": "amount", "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        }
    ]);

    Erc20Contract {
        abi,
        bytecode: concat!(
            "0x",
            "60a060405234801561000f575f5ffd5b50604051610ac3380380610ac383398101604081905261002e91610162565b5f6100398582610274565b5060016100468482610274565b5060ff821660808190525f9061005d90600a61042b565b610067908361043d565b6002819055335f818152600360205260408082208490555192935090917fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef906100b39085815260200190565b60405180910390a35050505050610454565b634e487b7160e01b5f52604160045260245ffd5b5f82601f8301126100e8575f5ffd5b81516001600160401b03811115610101576101016100c5565b604051601f8201601f19908116603f011681016001600160401b038111828210171561012f5761012f6100c5565b604052818152838201602001851015610146575f5ffd5b8160208501602083015e5f918101602001919091529392505050565b5f5f5f5f60808587031215610175575f5ffd5b84516001600160401b0381111561018a575f5ffd5b610196878288016100d9565b602087015190955090506001600160401b038111156101b3575f5ffd5b6101bf878288016100d9565b935050604085015160ff811681146101d5575f5ffd5b6060959095015193969295505050565b600181811c908216806101f957607f821691505b60208210810361021757634e487b7160e01b5f52602260045260245ffd5b50919050565b601f82111561026f578282111561026f57805f5260205f20601f840160051c602085101561024857505f5b90810190601f840160051c035f5b8181101561026b575f83820155600101610256565b5050505b505050565b81516001600160401b0381111561028d5761028d6100c5565b6102a18161029b84546101e5565b8461021d565b6020601f8211600181146102d3575f83156102bc5750848201515b5f19600385901b1c1916600184901b17845561032b565b5f84815260208120601f198516915b8281101561030257878501518255602094850194600190920191016102e2565b508482101561031f57868401515f19600387901b60f8161c191681555b505060018360011b0184555b5050505050565b634e487b7160e01b5f52601160045260245ffd5b6001815b60018411156103815780850481111561036557610365610332565b600184161561037357908102905b60019390931c92800261034a565b935093915050565b5f8261039757506001610425565b816103a357505f610425565b81600181146103b957600281146103c3576103df565b6001915050610425565b60ff8411156103d4576103d4610332565b50506001821b610425565b5060208310610133831016604e8410600b8410161715610402575081810a610425565b61040e5f198484610346565b805f190482111561042157610421610332565b0290505b92915050565b5f6104368383610389565b9392505050565b808202811582820484141761042557610425610332565b60805161065761046c5f395f61010401526106575ff3fe608060405234801561000f575f5ffd5b5060043610610090575f3560e01c8063313ce56711610063578063313ce567146100ff57806370a082311461013857806395d89b4114610157578063a9059cbb1461015f578063dd62ed3e14610172575f5ffd5b806306fdde0314610094578063095ea7b3146100b257806318160ddd146100d557806323b872dd146100ec575b5f5ffd5b61009c61019c565b6040516100a991906104c7565b60405180910390f35b6100c56100c0366004610517565b610227565b60405190151581526020016100a9565b6100de60025481565b6040519081526020016100a9565b6100c56100fa36600461053f565b610293565b6101267f000000000000000000000000000000000000000000000000000000000000000081565b60405160ff90911681526020016100a9565b6100de610146366004610579565b60036020525f908152604090205481565b61009c610348565b6100c561016d366004610517565b610355565b6100de610180366004610599565b600460209081525f928352604080842090915290825290205481565b5f80546101a8906105ca565b80601f01602080910402602001604051908101604052809291908181526020018280546101d4906105ca565b801561021f5780601f106101f65761010080835404028352916020019161021f565b820191905f5260205f20905b81548152906001019060200180831161020257829003601f168201915b505050505081565b335f8181526004602090815260408083206001600160a01b038716808552925280832085905551919290917f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925906102819086815260200190565b60405180910390a35060015b92915050565b6001600160a01b0383165f9081526004602090815260408083203384529091528120548281101561030b5760405162461bcd60e51b815260206004820152601d60248201527f45524332303a20696e73756666696369656e7420616c6c6f77616e636500000060448201526064015b60405180910390fd5b6001600160a01b0385165f9081526004602090815260408083203384529091529020838203905561033d85858561036a565b506001949350505050565b600180546101a8906105ca565b5f61036133848461036a565b50600192915050565b6001600160a01b0382166103c05760405162461bcd60e51b815260206004820152601f60248201527f45524332303a207472616e7366657220746f207a65726f2061646472657373006044820152606401610302565b6001600160a01b0383165f90815260036020526040902054818110156104375760405162461bcd60e51b815260206004820152602660248201527f45524332303a207472616e7366657220616d6f756e7420657863656564732062604482015265616c616e636560d01b6064820152608401610302565b6001600160a01b038085165f9081526003602052604080822085850390559185168152908120805484929061046d908490610602565b92505081905550826001600160a01b0316846001600160a01b03167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef846040516104b991815260200190565b60405180910390a350505050565b602081525f82518060208401528060208501604085015e5f604082850101526040601f19601f83011684010191505092915050565b80356001600160a01b0381168114610512575f5ffd5b919050565b5f5f60408385031215610528575f5ffd5b610531836104fc565b946020939093013593505050565b5f5f5f60608486031215610551575f5ffd5b61055a846104fc565b9250610568602085016104fc565b929592945050506040919091013590565b5f60208284031215610589575f5ffd5b610592826104fc565b9392505050565b5f5f604083850312156105aa575f5ffd5b6105b3836104fc565b91506105c1602084016104fc565b90509250929050565b600181811c908216806105de57607f821691505b6020821081036105fc57634e487b7160e01b5f52602260045260245ffd5b8082018082111561028d57634e487b7160e01b5f52601160045260245ffdfea26469706673582212208411935329de5c1c4feb185b4019b0dc36a412b68503908336b4179dd35635f464736f6c63430008220033"
        )
        .to_string(),
    }
}

/// Network-specific configuration for a blockchain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub chain_id: u64,
    pub rpc_url: String,
    pub explorer_url: String,
}

/// Returns default chain configurations for all supported networks.
pub fn get_chain_configs() -> HashMap<Chain, ChainConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        Chain::Ethereum,
        ChainConfig {
            name: "Ethereum Mainnet".to_string(),
            chain_id: 1,
            rpc_url: "https://eth.llamarpc.com".to_string(),
            explorer_url: "https://etherscan.io".to_string(),
        },
    );

    configs.insert(
        Chain::Base,
        ChainConfig {
            name: "Base Mainnet".to_string(),
            chain_id: 8453,
            rpc_url: "https://mainnet.base.org".to_string(),
            explorer_url: "https://basescan.org".to_string(),
        },
    );

    configs.insert(
        Chain::Solana,
        ChainConfig {
            name: "Solana Mainnet".to_string(),
            chain_id: 0,
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            explorer_url: "https://explorer.solana.com".to_string(),
        },
    );

    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erc20_contract_has_expected_abi_entries() {
        let contract = get_erc20_contract();
        let abi = contract.abi.as_array().unwrap();
        // constructor + 2 events + 9 functions
        assert_eq!(abi.len(), 12);
    }

    #[test]
    fn erc20_contract_serializes() {
        let contract = get_erc20_contract();
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("transfer"));
        assert!(json.contains("balanceOf"));
        assert!(json.contains("Approval"));
    }

    #[test]
    fn erc20_contract_uses_real_bytecode() {
        let contract = get_erc20_contract();
        assert!(contract.bytecode.starts_with("0x60"));
        assert!(!contract.bytecode.contains("PLACEHOLDER"));
    }

    #[test]
    fn chain_configs_cover_all_chains() {
        let configs = get_chain_configs();
        assert!(configs.contains_key(&Chain::Ethereum));
        assert!(configs.contains_key(&Chain::Base));
        assert!(configs.contains_key(&Chain::Solana));
    }

    #[test]
    fn chain_config_ids_match_chain_enum() {
        let configs = get_chain_configs();
        for (chain, config) in &configs {
            assert_eq!(chain.chain_id(), config.chain_id);
        }
    }

    #[test]
    fn chain_config_rpc_urls_are_https() {
        let configs = get_chain_configs();
        for config in configs.values() {
            assert!(
                config.rpc_url.starts_with("https://"),
                "RPC URL must be HTTPS: {}",
                config.rpc_url
            );
        }
    }
}
