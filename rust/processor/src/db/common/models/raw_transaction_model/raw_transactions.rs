use aptos_protos::{
    transaction::v1::{Transaction as TransactionPB},
    util::timestamp::Timestamp,
};
use bigdecimal::BigDecimal;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};
use crate::db::common::models::default_models::write_set_changes::WriteSetChange;
use crate::db::common::models::events_models::events::Event;
use crate::db::common::models::user_transactions_models::signatures::Signature;
use crate::utils::util::standardize_address;

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct RawTransaction {
    pub version: u64,
    pub hash: String,
    pub state_change_hash: String,
    pub event_root_hash: String,
    pub state_checkpoint_hash: Option<String>,
    pub gas_used: u64,
    pub success: bool,
    pub vm_status: String,
    pub accumulator_root_hash: String,
    pub changes: Vec<WriteSetChange>,
    pub sender: String,
    pub sequence_number: u64,
    pub max_gas_amount: u64,
    pub gas_unit_price: u64,
    pub expiration_timestamp_secs: u64,
    pub payload: Option<serde_json::Value>,
    pub signature: Vec<Signature>,
    pub events: Vec<Event>,
    pub timestamp: i64,
    pub type_: String,
}

impl RawTransaction {
    pub fn from_transaction(txn: &TransactionPB) -> (Self) {
        let info = txn.info.as_ref().unwrap();
        Self {
            version: txn.version,
            hash: standardize_address(
                hex::encode(info.hash.as_slice()).as_str(),
            ),
            state_change_hash: standardize_address(
                hex::encode(info.state_change_hash.as_slice()).as_str(),
            ),
            event_root_hash: standardize_address(
                hex::encode(info.event_root_hash.as_slice()).as_str(),
            ),
            state_checkpoint_hash: info
                .state_checkpoint_hash
                .as_ref()
                .map(|hash| standardize_address(hex::encode(hash).as_str())),
            gas_used: info.gas_used,
            success: info.success,
            vm_status: info.vm_status.clone(),
            accumulator_root_hash: standardize_address(
                hex::encode(info.accumulator_root_hash.as_slice()).as_str(),
            ),
        }
    }
}
pub type RawTransactionModel = RawTransaction;