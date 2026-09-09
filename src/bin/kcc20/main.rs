use std::collections::BTreeMap;

use argent::{build_file, builder, ArgentError};
use argent_runtime::{
    args, state, ArgValue,
    ArtifactValue::{self, Array},
    EntryCall, TxBuilder, TxContext,
};
use kaspa_consensus_core::{
    hashing::sighash_type::SIG_HASH_ALL,
    tx::{CovenantBinding, TransactionId, TransactionOutpoint},
    Hash,
};
use kcc20_live_session::{demo_keys, demo_outpoint, print_tx_summary, sign_input};

const OWNER_P2PK_SCHNORR: u8 = 0x00;
const BORROW_DISABLED: u8 = 0x00;
const BORROW_THRESHOLD: u8 = 0x01;
const PATH_NORMAL: u8 = 0x00;
const PATH_BORROW: u8 = 0x01;

fn token_state_borrowable(owner: &[u8; 32], amount: i64) -> BTreeMap<String, ArtifactValue> {
    let mut guard = [0u8; 32];
    guard[0] = 10;

    state! {
        amount: amount,
        owner: owner.to_vec(),
        owner_scheme: OWNER_P2PK_SCHNORR,
        borrow_scheme: BORROW_THRESHOLD,
        borrow_guard: guard,
        extension_commitment: vec![0u8; 32]
    }
}
fn token_state(owner: &[u8; 32], amount: i64) -> BTreeMap<String, ArtifactValue> {
    state! {
        amount: amount,
        owner: owner.to_vec(),
        owner_scheme: OWNER_P2PK_SCHNORR,
        borrow_scheme: BORROW_DISABLED,
        borrow_guard: vec![0u8; 32],
        extension_commitment: vec![0u8; 32]
    }
}

fn main() -> Result<(), ArgentError> {
    let artifact = build_file("contracts/kcc20.ag", "build").unwrap();
    let (alice, alice_pk) = demo_keys(0xaa);
    let (bob, bob_pk) = demo_keys(0xbb);

    let alice_outpoint = demo_outpoint(1, 0);
    let bob_outpoint = demo_outpoint(2, 0);

    let bob_before = token_state_borrowable(&bob_pk, 200);
    let alice_before = token_state(&alice_pk, 100);

    let bob_after = token_state_borrowable(&bob_pk, 211);
    let alice_after = token_state(&alice_pk, 89);

    let builder = TxBuilder::new(&artifact).unwrap();

    let cov_id = Hash::from_u64_word(0);
    let bob_utxo = builder
        .covenant_utxo("KCC20", bob_before.clone(), 1, 1, false, Some(cov_id))
        .unwrap();
    let alice_utxo = builder
        .covenant_utxo("KCC20", alice_before.clone(), 1, 1, false, Some(cov_id))
        .unwrap();
    let next_states = ArtifactValue::Array(vec![
        ArtifactValue::Object(bob_after.clone()),
        ArtifactValue::Object(alice_after.clone()),
    ]);

    let transfer = EntryCall::new("transfer").args_with(|_tx, _input_index| {
        let witness = vec![PATH_BORROW];
        args!(next_states.clone(), witness)
    });
    let transfer_delegate = EntryCall::new("transfer_delegator")
        .args_with(|tx, input_index| args!(sign_input(tx, input_index, &alice)));

    let context = TxContext::new()
        .actor_input("KCC20", bob_before, transfer, bob_outpoint, bob_utxo, 0)
        .actor_input(
            "KCC20",
            alice_before,
            transfer_delegate,
            alice_outpoint,
            alice_utxo,
            0,
        )
        .actor_output(
            "KCC20",
            bob_after.clone(),
            CovenantBinding::new(0, cov_id),
            1,
        )
        .actor_output(
            "KCC20",
            alice_after.clone(),
            CovenantBinding::new(0, cov_id),
            1,
        );

    let tx = builder.build(&context).unwrap();

    println!("tx: {}", tx.id());
    println!("inputs: {}", tx.inputs.len());
    println!("outputs: {}", tx.outputs.len());

    return Ok(());
}
