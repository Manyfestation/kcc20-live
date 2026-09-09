//! Helpers used by the offline KCC20 live demo.

use kaspa_consensus_core::{
    hashing::{
        sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash},
        sighash_type::SIG_HASH_ALL,
    },
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint},
};
use secp256k1::{Keypair, Secp256k1, SecretKey};

/// Deterministic demo key for actors that never touch the network (token holders).
pub fn demo_keypair(byte: u8) -> Keypair {
    let secret_key = SecretKey::from_slice(&[byte; 32]).expect("demo secret key is valid");
    Keypair::from_secret_key(&Secp256k1::new(), &secret_key)
}

/// Deterministic keypair and its 32-byte x-only public key, for offline demos.
pub fn demo_keys(byte: u8) -> (Keypair, Vec<u8>) {
    let keypair = demo_keypair(byte);
    let public_key = xonly(&keypair);
    (keypair, public_key)
}

/// Deterministic outpoint for offline demos.
pub fn demo_outpoint(byte: u8, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([byte; 32]),
        index,
    }
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
