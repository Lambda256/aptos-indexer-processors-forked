// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    db::common::models::default_models::{
        block_metadata_transactions::{BlockMetadataTransaction, BlockMetadataTransactionModel},
        move_tables::{CurrentTableItem, TableItem, TableMetadata},
        transactions::TransactionModel,
        write_set_changes::WriteSetChangeDetail,
    },
    gap_detectors::ProcessingResult,
    schema,
    utils::{
        database::{execute_in_chunks, get_config_table_chunk_size, ArcDbPool},
        mq::{CustomProducer, CustomProducerEnum},
        network::Network,
    },
    worker::TableFlags,
};
use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::Transaction;
use async_trait::async_trait;
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    ExpressionMethods,
};
use std::fmt::Debug;
use tokio::join;
use tracing::error;

pub struct DefaultProcessor {
    producer: CustomProducerEnum,
    connection_pool: ArcDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
    deprecated_tables: TableFlags,
}

impl DefaultProcessor {
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

impl Debug for DefaultProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "DefaultTransactionProcessor {{ connections: {:?}  idle_connections: {:?} }}",
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
    block_metadata_transactions: &[BlockMetadataTransactionModel],
    (table_items, current_table_items, table_metadata): (
        &[TableItem],
        &[CurrentTableItem],
        &[TableMetadata],
    ),
) -> Result<(), String> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Producing to mq",
    );

    let bmt_topic = format!("aptos.{}.block.metadata.transactions", network);
    let ti_topic = format!("aptos.{}.table.items", network);
    let cti_topic = format!("aptos.{}.current.table.items", network);
    let tm_topic = format!("aptos.{}.table.metadatas", network);
    let (bmt_res, ti_res, cti_res, tm_res) = tokio::join!(
        producer.send_to_mq(bmt_topic.as_str(), block_metadata_transactions),
        producer.send_to_mq(ti_topic.as_str(), table_items),
        producer.send_to_mq(cti_topic.as_str(), current_table_items),
        producer.send_to_mq(tm_topic.as_str(), table_metadata),
    );

    for res in [bmt_res, ti_res, cti_res, tm_res] {
        res?;
    }

    Ok(())
}

async fn insert_to_db(
    conn: ArcDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    block_metadata_transactions: &[BlockMetadataTransactionModel],
    (table_items, current_table_items, table_metadata): (
        &[TableItem],
        &[CurrentTableItem],
        &[TableMetadata],
    ),
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );

    let bmt_res = execute_in_chunks(
        conn.clone(),
        insert_block_metadata_transactions_query,
        block_metadata_transactions,
        get_config_table_chunk_size::<BlockMetadataTransactionModel>(
            "block_metadata_transactions",
            per_table_chunk_sizes,
        ),
    );

    let ti_res = execute_in_chunks(
        conn.clone(),
        insert_table_items_query,
        table_items,
        get_config_table_chunk_size::<TableItem>("table_items", per_table_chunk_sizes),
    );

    let cti_res = execute_in_chunks(
        conn.clone(),
        insert_current_table_items_query,
        current_table_items,
        get_config_table_chunk_size::<CurrentTableItem>(
            "current_table_items",
            per_table_chunk_sizes,
        ),
    );

    let tm_res = execute_in_chunks(
        conn.clone(),
        insert_table_metadata_query,
        table_metadata,
        get_config_table_chunk_size::<TableMetadata>("table_metadatas", per_table_chunk_sizes),
    );

    let (bmt_res, ti_res, cti_res, tm_res) = join!(bmt_res, ti_res, cti_res, tm_res);

    for res in [bmt_res, ti_res, cti_res, tm_res] {
        res?;
    }

    Ok(())
}

fn insert_block_metadata_transactions_query(
    items_to_insert: Vec<BlockMetadataTransactionModel>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::block_metadata_transactions::dsl::*;

    (
        diesel::insert_into(schema::block_metadata_transactions::table)
            .values(items_to_insert)
            .on_conflict(version)
            .do_nothing(),
        None,
    )
}

fn insert_table_items_query(
    items_to_insert: Vec<TableItem>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::table_items::dsl::*;

    (
        diesel::insert_into(schema::table_items::table)
            .values(items_to_insert)
            .on_conflict((transaction_version, write_set_change_index))
            .do_nothing(),
        None,
    )
}

fn insert_current_table_items_query(
    items_to_insert: Vec<CurrentTableItem>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::current_table_items::dsl::*;

    (
        diesel::insert_into(schema::current_table_items::table)
            .values(items_to_insert)
            .on_conflict((table_handle, key_hash))
            .do_update()
            .set((
                key.eq(excluded(key)),
                decoded_key.eq(excluded(decoded_key)),
                decoded_value.eq(excluded(decoded_value)),
                is_deleted.eq(excluded(is_deleted)),
                last_transaction_version.eq(excluded(last_transaction_version)),
                inserted_at.eq(excluded(inserted_at)),
            )),
        Some(" WHERE current_table_items.last_transaction_version <= excluded.last_transaction_version "),
    )
}

fn insert_table_metadata_query(
    items_to_insert: Vec<TableMetadata>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::table_metadatas::dsl::*;

    (
        diesel::insert_into(schema::table_metadatas::table)
            .values(items_to_insert)
            .on_conflict(handle)
            .do_nothing(),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for DefaultProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::DefaultProcessor.into()
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
        let flags = self.deprecated_tables;
        let (block_metadata_transactions, (table_items, current_table_items, table_metadata)) =
            tokio::task::spawn_blocking(move || process_transactions(transactions, flags))
                .await
                .expect("Failed to spawn_blocking for TransactionModel::from_transactions");
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
            &block_metadata_transactions,
            (&table_items, &current_table_items, &table_metadata),
        )
        .await;

        if mq_result.is_err() {
            bail!(
                "Error sending default transactions to mq. Processor {}. Start {}. End {}. Error {:?}",
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
            &block_metadata_transactions,
            (&table_items, &current_table_items, &table_metadata),
            &self.per_table_chunk_sizes,
        )
        .await;

        // These vectors could be super large and take a lot of time to drop, move to background to
        // make it faster.
        tokio::task::spawn(async move {
            drop(block_metadata_transactions);
            drop(table_items);
            drop(current_table_items);
            drop(table_metadata);
        });

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

fn process_transactions(
    transactions: Vec<Transaction>,
    flags: TableFlags,
) -> (
    Vec<BlockMetadataTransaction>,
    (Vec<TableItem>, Vec<CurrentTableItem>, Vec<TableMetadata>),
) {
    let (block_metadata_txns, wsc_details) = TransactionModel::from_transactions(&transactions);
    let mut block_metadata_transactions = vec![];
    for block_metadata_txn in block_metadata_txns {
        block_metadata_transactions.push(block_metadata_txn);
    }
    let mut table_items = vec![];
    let mut current_table_items = AHashMap::new();
    let mut table_metadata = AHashMap::new();
    for detail in wsc_details {
        if let WriteSetChangeDetail::Table(item, current_item, metadata) = detail {
            table_items.push(item);
            current_table_items.insert(
                (
                    current_item.table_handle.clone(),
                    current_item.key_hash.clone(),
                ),
                current_item,
            );
            if let Some(meta) = metadata {
                table_metadata.insert(meta.handle.clone(), meta);
            }
        }
    }

    // Getting list of values and sorting by pk in order to avoid postgres deadlock since we're doing multi threaded db writes
    let mut current_table_items = current_table_items
        .into_values()
        .collect::<Vec<CurrentTableItem>>();
    let mut table_metadata = table_metadata.into_values().collect::<Vec<TableMetadata>>();
    // Sort by PK
    current_table_items
        .sort_by(|a, b| (&a.table_handle, &a.key_hash).cmp(&(&b.table_handle, &b.key_hash)));
    table_metadata.sort_by(|a, b| a.handle.cmp(&b.handle));

    if flags.contains(TableFlags::TABLE_ITEMS) {
        table_items.clear();
    }
    if flags.contains(TableFlags::TABLE_METADATAS) {
        table_metadata.clear();
    }

    (
        block_metadata_transactions,
        (table_items, current_table_items, table_metadata),
    )
}
