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
const PATH_NORMAL: u8 = 0x00;

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
    let artifact = build_file("contacts/another_kcc20.ag", "build").unwrap();
    let (alice, alice_pk) = demo_keys(0xaa);
    let (bob, bob_pk) = demo_keys(0xbb);

    let alice_outpoint = demo_outpoint(1, 0);

    let alice_before = token_state(&alice_pk, 100);
    let alice_after = token_state(&alice_pk, 90);
    let bob_after = token_state(&bob_pk, 10);

    let builder = TxBuilder::new(&artifact).unwrap();

    let cov_id = Hash::from_u64_word(0);
    let alice_utxo = builder
        .covenant_utxo("KCC20", alice_before, 1, 1, false, Some(cov_id))
        .unwrap();
    let next_states = ArtifactValue::Array(vec![
        ArtifactValue::Object(bob_after.clone()),
        ArtifactValue::Object(alice_after.clone()),
    ]);

    let transfer = EntryCall::new("transfer").args_with(|tx, input_index| {
        let mut witness = vec![PATH_NORMAL];

        witness.extend(sign_input(tx, input_index, &alice));
        args!(next_states, witness)
    });

    let context = TxContext::new()
        .actor_input(
            "KCC20",
            alice_before,
            transfer,
            alice_outpoint,
            alice_utxo,
            0,
        )
        .actor_output("KCC20", alice_after, CovenantBinding::new(0, cov_id), 1)
        .actor_output("KCC20", bob_after, CovenantBinding::new(0, cov_id), 1);

    let tx = builder.build(&context).unwrap();

    print!("tx: {}", tx.id());
    print!("inputs: {}", tx.inputs.len());
    print!("outputs: {}", tx.outputs.len());

    return Ok(());
}
