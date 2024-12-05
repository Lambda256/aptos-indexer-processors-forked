use crate::db::common::models::default_models::write_set_changes::WriteSetChangeDetail;
use crate::db::common::models::events_models::events::{Event, EventModel};
use crate::db::common::models::user_transactions_models::signatures::Signature;
use crate::db::common::models::user_transactions_models::user_transactions::UserTransactionModel;
use crate::utils::util::{
    get_clean_payload, get_payload_type, parse_timestamp, standardize_address,
};
use aptos_protos::transaction::v1::transaction::{TransactionType, TxnData};
use aptos_protos::transaction::v1::Transaction as TransactionPB;
use bigdecimal::ToPrimitive;
use serde::{Deserialize, Serialize};

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
    pub changes: Vec<serde_json::Value>,
    pub sender: String,
    pub sequence_number: u64,
    pub max_gas_amount: u64,
    pub gas_unit_price: u64,
    pub expiration_timestamp_secs: i64,
    pub payload_type: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub signature: Vec<Signature>,
    pub events: Vec<Event>,
    pub timestamp: i64,
    pub type_: String,

    pub block_height: i64,
}

impl RawTransaction {
    pub fn from_transaction(txn: &TransactionPB) -> (Self) {
        let info = txn.info.as_ref().unwrap();
        let block_height = txn.block_height as i64;
        let txn_version = txn.version as i64;
        let txn_timestamp = parse_timestamp(txn.timestamp.as_ref().unwrap(), txn_version);
        let wsc_details = WriteSetChangeDetail::from_write_set_changes(
            &info.changes,
            txn.version.to_i64().unwrap(),
            block_height,
        );
        let mut sender: String = "".to_string();
        let mut sequence_number: u64 = 0;
        let mut max_gas_amount: u64 = 0;
        let mut gas_unit_price: u64 = 0;
        let mut expiration_timestamp_secs: i64 = 0;
        let mut payload_type: Option<String> = None;
        let mut payload: Option<serde_json::Value> = None;
        let mut signature: Vec<Signature> = vec![];
        let mut events = vec![];
        match txn.txn_data.as_ref() {
            Some(txn_data) => {
                let default = vec![];
                let raw_events = match txn_data {
                    TxnData::BlockMetadata(tx_inner) => &tx_inner.events,
                    TxnData::Genesis(tx_inner) => &tx_inner.events,
                    TxnData::User(tx_inner) => &tx_inner.events,
                    TxnData::Validator(tx_inner) => &tx_inner.events,
                    _ => &default,
                };
                let txn_events = EventModel::from_events(raw_events, txn_version, block_height);
                events.extend(txn_events);
                if let TxnData::User(inner) = txn_data {
                    let (user_transaction, sigs) = UserTransactionModel::from_transaction(
                        inner,
                        txn.timestamp.as_ref().unwrap(),
                        block_height,
                        txn.epoch as i64,
                        txn_version,
                    );
                    let req = inner.request.as_ref().unwrap();
                    let (pload, ptype) = match req.payload.as_ref() {
                        Some(payload) => {
                            let payload_cleaned = get_clean_payload(payload, txn_version);
                            (payload_cleaned, Some(get_payload_type(payload)))
                        },
                        None => (None, None),
                    };
                    payload_type = ptype;
                    payload = pload;
                    sender = user_transaction.sender;
                    sequence_number = user_transaction.sequence_number.to_u64().unwrap();
                    max_gas_amount = user_transaction.max_gas_amount.to_u64().unwrap();
                    gas_unit_price = user_transaction.gas_unit_price.to_u64().unwrap();
                    expiration_timestamp_secs = user_transaction
                        .expiration_timestamp_secs
                        .and_utc()
                        .timestamp();
                    signature = sigs;
                } else {
                }
            },
            None => {},
        };

        let mut changes: Vec<serde_json::Value> = vec![];
        for wsc in wsc_details {
            changes.push(serde_json::to_value(&wsc).unwrap());
        }

        Self {
            version: txn.version,
            hash: standardize_address(hex::encode(info.hash.as_slice()).as_str()),
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
            changes,
            sender,
            sequence_number,
            max_gas_amount,
            gas_unit_price,
            expiration_timestamp_secs,
            payload_type,
            payload,
            signature,
            events,
            timestamp: txn_timestamp.and_utc().timestamp_micros(),
            type_: TransactionType::try_from(txn.r#type)
                .unwrap()
                .as_str_name()
                .to_string(),
            block_height,
        }
    }
}
pub type RawTransactionModel = RawTransaction;
