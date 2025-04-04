// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    db::postgres::models::user_transactions_models::{
        signatures::Signature, user_transactions::UserTransactionModel,
    },
    gap_detectors::ProcessingResult,
    schema,
    utils::{
        counters::PROCESSOR_UNKNOWN_TYPE_COUNT,
        database::{execute_in_chunks, get_config_table_chunk_size, ArcDbPool},
        mq::{CustomProducer, CustomProducerEnum},
        network::Network,
        table_flags::TableFlags,
    },
};
use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::{transaction::TxnData, Transaction};
use async_trait::async_trait;
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    ExpressionMethods,
};
use std::fmt::Debug;
use tracing::error;

pub struct UserTransactionProcessor {
    producer: CustomProducerEnum,
    connection_pool: ArcDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
    deprecated_tables: TableFlags,
}

impl UserTransactionProcessor {
    pub fn new(
        producer: CustomProducerEnum,
        connection_pool: ArcDbPool,
        per_table_chunk_sizes: AHashMap<String, usize>,
        deprecated_tables: TableFlags,
    ) -> Self {
        Self {
            producer,
            connection_pool,
            per_table_chunk_sizes,
            deprecated_tables,
        }
    }
}

impl Debug for UserTransactionProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "UserTransactionProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

async fn produce_to_mq(
    producer: &CustomProducerEnum,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    network: String,
    user_transactions: &[UserTransactionModel],
    signatures: &[Signature],
) -> Result<(), String> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Producing to mq",
    );

    let ut_topic = format!("aptos.{}.user.transactions", network);
    let is_topic = format!("aptos.{}.signatures", network);
    let (ut_res, is_res) = tokio::join!(
        producer.send_to_mq(ut_topic.as_str(), user_transactions),
        producer.send_to_mq(is_topic.as_str(), signatures),
    );

    for res in [ut_res, is_res] {
        res?;
    }

    Ok(())
}

async fn insert_to_db(
    conn: ArcDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    user_transactions: &[UserTransactionModel],
    signatures: &[Signature],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );

    let ut = execute_in_chunks(
        conn.clone(),
        insert_user_transactions_query,
        user_transactions,
        get_config_table_chunk_size::<UserTransactionModel>(
            "user_transactions",
            per_table_chunk_sizes,
        ),
    );
    let is = execute_in_chunks(
        conn,
        insert_signatures_query,
        signatures,
        get_config_table_chunk_size::<Signature>("signatures", per_table_chunk_sizes),
    );

    let (ut_res, is_res) = futures::join!(ut, is);
    for res in [ut_res, is_res] {
        res?;
    }
    Ok(())
}

pub fn insert_user_transactions_query(
    items_to_insert: Vec<UserTransactionModel>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::user_transactions::dsl::*;
    (
        diesel::insert_into(schema::user_transactions::table)
            .values(items_to_insert)
            .on_conflict(version)
            .do_update()
            .set((
                entry_function_contract_address.eq(excluded(entry_function_contract_address)),
                entry_function_module_name.eq(excluded(entry_function_module_name)),
                entry_function_function_name.eq(excluded(entry_function_function_name)),
                inserted_at.eq(excluded(inserted_at)),
            )),
        None,
    )
}

pub fn insert_signatures_query(
    items_to_insert: Vec<Signature>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::signatures::dsl::*;
    (
        diesel::insert_into(schema::signatures::table)
            .values(items_to_insert)
            .on_conflict((
                transaction_version,
                multi_agent_index,
                multi_sig_index,
                is_sender_primary,
            ))
            .do_nothing(),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for UserTransactionProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::UserTransactionProcessor.into()
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        start_version: u64,
        end_version: u64,
        _db_chain_id: Option<u64>,
    ) -> anyhow::Result<ProcessingResult> {
        let processing_start = std::time::Instant::now();
        let last_transaction_timestamp = transactions.last().unwrap().timestamp;

        let (user_transactions, signatures) =
            user_transaction_parse(transactions, self.deprecated_tables);

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_start = std::time::Instant::now();

        let network = Network::from_chain_id(_db_chain_id.unwrap_or(0));
        if network.is_none() {
            bail!(
                "Error getting network from chain id. Processor {}.",
                self.name()
            )
        }

        let mq_result = produce_to_mq(
            &self.producer,
            self.name(),
            start_version,
            end_version,
            network.unwrap().to_string(),
            &user_transactions,
            &signatures,
        )
        .await;

        if mq_result.is_err() {
            bail!(
                "Error sending user transaction to mq. Processor {}. Start {}. End {}. Error {:?}",
                self.name(),
                start_version,
                end_version,
                mq_result.err()
            )
        }

        let tx_result = insert_to_db(
            self.get_pool(),
            self.name(),
            start_version,
            end_version,
            &user_transactions,
            &signatures,
            &self.per_table_chunk_sizes,
        )
        .await;
        let db_insertion_duration_in_secs = db_insertion_start.elapsed().as_secs_f64();
        match tx_result {
            Ok(_) => Ok(ProcessingResult::DefaultProcessingResult(
                DefaultProcessingResult {
                    start_version,
                    end_version,
                    processing_duration_in_secs,
                    db_insertion_duration_in_secs,
                    last_transaction_timestamp,
                },
            )),
            Err(e) => {
                error!(
                    start_version = start_version,
                    end_version = end_version,
                    processor_name = self.name(),
                    error = ?e,
                    "[Parser] Error inserting transactions to db",
                );
                bail!(e)
            },
        }
    }

    fn connection_pool(&self) -> &ArcDbPool {
        &self.connection_pool
    }
}

/// Helper function to parse user transactions and signatures from the transaction data.
pub fn user_transaction_parse(
    transactions: Vec<Transaction>,
    deprecated_tables: TableFlags,
) -> (Vec<UserTransactionModel>, Vec<Signature>) {
    let mut signatures = vec![];
    let mut user_transactions = vec![];
    for txn in transactions {
        let txn_version = txn.version as i64;
        let block_height = txn.block_height as i64;
        let txn_data = match txn.txn_data.as_ref() {
            Some(txn_data) => txn_data,
            None => {
                PROCESSOR_UNKNOWN_TYPE_COUNT
                    .with_label_values(&["UserTransactionProcessor"])
                    .inc();
                tracing::warn!(
                    transaction_version = txn_version,
                    "Transaction data doesn't exist"
                );
                continue;
            },
        };
        if let TxnData::User(inner) = txn_data {
            let (user_transaction, sigs) = UserTransactionModel::from_transaction(
                inner,
                txn.timestamp.as_ref().unwrap(),
                block_height,
                txn.epoch as i64,
                txn_version,
            );
            signatures.extend(sigs);
            user_transactions.push(user_transaction);
        }
    }

    if deprecated_tables.contains(TableFlags::SIGNATURES) {
        signatures.clear();
    }

    (user_transactions, signatures)
}
