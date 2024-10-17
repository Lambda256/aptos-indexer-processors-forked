// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    db::common::models::account_transaction_models::account_transactions::AccountTransaction,
    gap_detectors::ProcessingResult,
    schema,
    utils::{
        database::{execute_in_chunks, get_config_table_chunk_size, ArcDbPool},
        mq::{CustomProducer, CustomProducerEnum},
        network::Network,
    },
};
use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::Transaction;
use async_trait::async_trait;
use diesel::{pg::Pg, query_builder::QueryFragment};
use rayon::prelude::*;
use std::fmt::Debug;
use tracing::{info, error};

pub struct AccountTransactionsProcessor {
    producer: CustomProducerEnum,
    connection_pool: ArcDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
}

impl AccountTransactionsProcessor {
    pub fn new(
        producer: CustomProducerEnum,
        connection_pool: ArcDbPool,
        per_table_chunk_sizes: AHashMap<String, usize>,
    ) -> Self {
        Self {
            producer,
            connection_pool,
            per_table_chunk_sizes,
        }
    }
}

impl Debug for AccountTransactionsProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "AccountTransactionsProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

async fn insert_to_db(
    conn: ArcDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    account_transactions: &[AccountTransaction],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );
    execute_in_chunks(
        conn.clone(),
        insert_account_transactions_query,
        account_transactions,
        get_config_table_chunk_size::<AccountTransaction>(
            "account_transactions",
            per_table_chunk_sizes,
        ),
    )
    .await?;
    Ok(())
}

fn insert_account_transactions_query(
    item_to_insert: Vec<AccountTransaction>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::account_transactions::dsl::*;

    (
        diesel::insert_into(schema::account_transactions::table)
            .values(item_to_insert)
            .on_conflict((transaction_version, account_address))
            .do_nothing(),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for AccountTransactionsProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::AccountTransactionsProcessor.into()
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        start_version: u64,
        end_version: u64,
        _db_chain_id: Option<u64>,
    ) -> anyhow::Result<ProcessingResult> {
        let processing_start = std::time::Instant::now();
        let last_transaction_timestamp = transactions.last().unwrap().timestamp.clone();

        let account_transactions: Vec<_> = transactions
            .into_par_iter()
            .map(|txn| {
                let transaction_version = txn.version as i64;
                let accounts = AccountTransaction::get_accounts(&txn);
                accounts
                    .into_iter()
                    .map(|account_address| AccountTransaction {
                        transaction_version,
                        account_address,
                    })
                    .collect()
            })
            .collect::<Vec<Vec<_>>>()
            .into_iter()
            .flatten()
            .collect();

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_start = std::time::Instant::now();

        let network = Network::from_chain_id(_db_chain_id.unwrap_or(0));
        if network.is_none() {
            bail!("Error getting network from chain id. Processor {}.", self.name())
        }
        let topic_string = format!("aptos.{}.account.transactions", network.unwrap());
        info!(
            processor_name = self.name(),
            topic_string = &topic_string,
            "Configured topic with correct network"
        );
        let topic: &str = &topic_string;
        let mq_result = self
            .producer
            .send_into_mq(topic, &account_transactions)
            .await;

        // return error if sending to mq fails
        if mq_result.is_err() {
            bail!("Error sending account transactions to mq. Processor {}. Start {}. End {}. Error {:?}", self.name(), start_version, end_version, mq_result.err())
        }

        let tx_result = insert_to_db(
            self.get_pool(),
            self.name(),
            start_version,
            end_version,
            &account_transactions,
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
            Err(err) => {
                error!(
                    start_version = start_version,
                    end_version = end_version,
                    processor_name = self.name(),
                    "[Parser] Error inserting transactions to db: {:?}",
                    err
                );
                bail!(format!("Error inserting transactions to db. Processor {}. Start {}. End {}. Error {:?}", self.name(), start_version, end_version, err))
            },
        }
    }

    fn connection_pool(&self) -> &ArcDbPool {
        &self.connection_pool
    }
}
