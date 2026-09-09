//! Plumbing for the KCC20 live session: wallet key, signing, mass/fee
//! reporting, and a thin TN10 wRPC client. Nothing in here is KCC20-specific;
//! the contract and transaction logic live in `contracts/` and `src/bin/demo.rs`.

use std::{fs, path::Path, time::Duration};

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    config::params::TESTNET_PARAMS,
    constants::SOMPI_PER_KASPA,
    hashing::{
        sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash},
        sighash_type::SIG_HASH_ALL,
    },
    mass::MassCalculator,
    tx::{
        MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId,
        TransactionOutpoint, UtxoEntry,
    },
};
use kaspa_rpc_core::RpcTransaction;
use kaspa_txscript::{pay_to_address_script, script_builder::ScriptBuilder};
use kaspa_wrpc_client::prelude::*;
use secp256k1::{Keypair, Secp256k1, SecretKey};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub type DemoResult<T> = Result<T>;

pub const SOMPI: u64 = SOMPI_PER_KASPA;
pub const WALLET_KEY_PATH: &str = ".wallet/tn10.key";
pub const EXPLORER: &str = "https://explorer-tn10.kaspa.org/txs";
/// Post-Toccata mempool minimum relay fee rate, in sompi per gram of mass.
pub const MIN_RELAY_FEE_PER_GRAM: u64 = 100;

// ---------------------------------------------------------------------------
// Keys and addresses
// ---------------------------------------------------------------------------

/// Load the wallet key from `.wallet/tn10.key` (hex secret), creating a fresh
/// random one on first use. The file is gitignored.
pub fn load_or_create_wallet() -> Result<Keypair> {
    let path = Path::new(WALLET_KEY_PATH);
    if path.exists() {
        let hex = fs::read_to_string(path)?;
        return keypair_from_hex(hex.trim());
    }
    let keypair = Keypair::new(&Secp256k1::new(), &mut rand::thread_rng());
    fs::create_dir_all(path.parent().expect("key path has a parent"))?;
    fs::write(
        path,
        faster_hex::hex_string(&keypair.secret_key().secret_bytes()),
    )?;
    Ok(keypair)
}

pub fn keypair_from_hex(hex: &str) -> Result<Keypair> {
    let mut secret = [0u8; 32];
    faster_hex::hex_decode(hex.as_bytes(), &mut secret)?;
    Ok(Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&secret)?,
    ))
}

/// Deterministic demo key for actors that never touch the network (token holders).
pub fn demo_keypair(byte: u8) -> Keypair {
    let secret_key = SecretKey::from_slice(&[byte; 32]).expect("demo secret key is valid");
    Keypair::from_secret_key(&Secp256k1::new(), &secret_key)
}

/// Deterministic keypair and its 32-byte x-only public key, for offline demos.
pub fn demo_keys(byte: u8) -> (Keypair, [u8;32]) {
    let keypair = demo_keypair(byte);
    let public_key = xonly(&keypair).try_into().expect("x_only_public_key must be 32bytes");
    (keypair, public_key)
}

/// Deterministic outpoint for offline demos.
pub fn demo_outpoint(byte: u8, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([byte; 32]),
        index,
    }
}

/// Anyone-can-spend funding UTXO for offline demos.
pub fn demo_funding_utxo(value: u64) -> UtxoEntry {
    UtxoEntry::new(
        value,
        ScriptPublicKey::from_vec(0, vec![0x51]),
        0,
        false,
        None,
    )
}

/// Compact transaction summary for offline demos.
pub fn print_tx_summary(tx: &Transaction) {
    println!("transaction: {}", tx.id());
    println!("inputs: {}", tx.inputs.len());
    println!("outputs: {}", tx.outputs.len());
}

/// 32-byte x-only public key: what `pubkey` fields and P2PK owners hold.
pub fn xonly(keypair: &Keypair) -> Vec<u8> {
    keypair.x_only_public_key().0.serialize().to_vec()
}

pub fn tn10_address(keypair: &Keypair) -> Address {
    Address::new(Prefix::Testnet, Version::PubKey, &xonly(keypair))
}

pub fn p2pk_script(keypair: &Keypair) -> ScriptPublicKey {
    pay_to_address_script(&tn10_address(keypair))
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/// Schnorr signature (64 bytes + sighash type) over one input of the unsigned
/// transaction. This is what Silverscript `checkSig` verifies and what a plain
/// P2PK input pushes as its signature script.
pub fn sign_input<T: AsRef<Transaction>>(
    tx: &MutableTransaction<T>,
    input_idx: usize,
    keypair: &Keypair,
) -> Vec<u8> {
    let reused_values = SigHashReusedValuesUnsync::new();
    let sig_hash =
        calc_schnorr_signature_hash(&tx.as_verifiable(), input_idx, SIG_HASH_ALL, &reused_values);
    let message = secp256k1::Message::from_digest(sig_hash.as_bytes());
    let mut signature = keypair.sign_schnorr(message).as_ref().to_vec();
    signature.push(SIG_HASH_ALL.to_u8());
    signature
}

/// Signature script for an ordinary P2PK wallet input.
pub fn p2pk_sig_script<T: AsRef<Transaction>>(
    tx: &MutableTransaction<T>,
    input_idx: usize,
    keypair: &Keypair,
) -> Vec<u8> {
    ScriptBuilder::new()
        .add_data(&sign_input(tx, input_idx, keypair))
        .expect("65-byte push fits")
        .drain()
}

// ---------------------------------------------------------------------------
// Mass / fee reporting
// ---------------------------------------------------------------------------

pub struct TxReport {
    pub id: TransactionId,
    pub inputs: usize,
    pub outputs: usize,
    pub fee: u64,
    /// Smallest fee the mempool relays for this transaction's mass.
    pub min_fee: u64,
    pub compute_mass: u64,
    pub transient_mass: u64,
    pub storage_mass: u64,
}

impl TxReport {
    pub fn new(tx: &Transaction, entries: &[UtxoEntry]) -> Self {
        let calculator = MassCalculator::new_with_consensus_params(&TESTNET_PARAMS);
        let non_contextual = calculator.calc_non_contextual_masses(tx);
        let populated = PopulatedTransaction::new(tx, entries.to_vec());
        let storage_mass = calculator
            .calc_contextual_masses(&populated)
            .map(|m| m.storage_mass)
            .unwrap_or(u64::MAX);
        let fee = entries.iter().map(|e| e.amount).sum::<u64>()
            - tx.outputs.iter().map(|o| o.value).sum::<u64>();
        let min_fee = MIN_RELAY_FEE_PER_GRAM
            * non_contextual
                .compute_mass
                .max(non_contextual.transient_mass)
                .max(storage_mass);
        Self {
            id: tx.id(),
            inputs: tx.inputs.len(),
            outputs: tx.outputs.len(),
            fee,
            min_fee,
            compute_mass: non_contextual.compute_mass,
            transient_mass: non_contextual.transient_mass,
            storage_mass,
        }
    }

    pub fn print(&self, label: &str) {
        println!("{label}");
        println!("  tx        {}", self.id);
        println!("  shape     {} in -> {} out", self.inputs, self.outputs);
        println!(
            "  fee       {} sompi (relay minimum {})",
            self.fee, self.min_fee
        );
        println!(
            "  mass      compute {}  transient {}  storage {}",
            self.compute_mass, self.transient_mass, self.storage_mass
        );
    }
}

pub fn kas(sompi: u64) -> String {
    format!("{}.{:08} KAS", sompi / SOMPI, sompi % SOMPI)
}

// ---------------------------------------------------------------------------
// TN10 node
// ---------------------------------------------------------------------------

pub struct Node {
    client: KaspaRpcClient,
}

impl Node {
    /// Connect to testnet-10. `KASPA_TN10_URL` overrides the public resolver.
    pub async fn connect() -> Result<Self> {
        let network = NetworkId::with_suffix(NetworkType::Testnet, 10);
        let url = std::env::var("KASPA_TN10_URL").ok();
        let resolver = url.is_none().then(Resolver::default);
        let client = KaspaRpcClient::new(
            WrpcEncoding::Borsh,
            url.as_deref(),
            resolver,
            Some(network),
            None,
        )?;
        client
            .connect(Some(ConnectOptions {
                block_async_connect: true,
                strategy: ConnectStrategy::Fallback,
                url: None,
                connect_timeout: Some(Duration::from_secs(20)),
                retry_interval: None,
            }))
            .await?;
        let info = client.get_server_info().await?;
        if info.network_id.to_string() != network.to_string() {
            return Err(format!("connected to {} instead of {network}", info.network_id).into());
        }
        println!(
            "node      {} v{} synced={} daa={}",
            client.url().unwrap_or_default(),
            info.server_version,
            info.is_synced,
            info.virtual_daa_score
        );
        Ok(Self { client })
    }

    /// Spendable wallet UTXOs, largest first.
    pub async fn utxos(&self, address: &Address) -> Result<Vec<(TransactionOutpoint, UtxoEntry)>> {
        let mut entries: Vec<_> = self
            .client
            .get_utxos_by_addresses(vec![address.clone()])
            .await?
            .into_iter()
            .map(|e| {
                let outpoint =
                    TransactionOutpoint::new(e.outpoint.transaction_id, e.outpoint.index);
                let u = e.utxo_entry;
                (
                    outpoint,
                    UtxoEntry::new(
                        u.amount,
                        u.script_public_key,
                        u.block_daa_score,
                        u.is_coinbase,
                        u.covenant_id,
                    ),
                )
            })
            .collect();
        entries.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
        Ok(entries)
    }

    pub async fn balance(&self, address: &Address) -> Result<u64> {
        Ok(self
            .utxos(address)
            .await?
            .iter()
            .map(|(_, u)| u.amount)
            .sum())
    }

    /// Submit a finalized transaction and return its id.
    pub async fn submit(&self, tx: &Transaction) -> Result<TransactionId> {
        Ok(self
            .client
            .submit_transaction(RpcTransaction::from(tx), false)
            .await?)
    }

    /// Wait until the transaction has left the mempool (accepted into a block)
    /// or the timeout passes. Chained spends of mempool outputs are allowed on
    /// Kaspa, so this is a courtesy pause, not a correctness requirement.
    pub async fn wait_accepted(&self, id: TransactionId, timeout: Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self
                .client
                .get_mempool_entry(id, false, false)
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect().await?;
        Ok(())
    }
}
