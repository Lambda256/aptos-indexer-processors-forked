// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    db::{
        common::models::event_models::raw_events::parse_events,
        postgres::models::events_models::events::EventPG,
    },
    gap_detectors::ProcessingResult,
    schema,
    utils::{
        counters::PROCESSOR_UNKNOWN_TYPE_COUNT,
        database::{execute_in_chunks, get_config_table_chunk_size, ArcDbPool},
        mq::{CustomProducer, CustomProducerEnum},
        network::Network,
    },
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
use tracing::error;

pub struct EventsProcessor {
    producer: CustomProducerEnum,
    connection_pool: ArcDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
}

impl EventsProcessor {
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

impl Debug for EventsProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "EventsProcessor {{ connections: {:?}  idle_connections: {:?} }}",
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
    events: &[EventPG],
) -> Result<(), String> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Producing to mq",
    );

    let events_topic = format!("aptos.{}.events", network);
    producer.send_to_mq(events_topic.as_str(), events).await
}

async fn insert_to_db(
    conn: ArcDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    events: &[EventPG],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );
    execute_in_chunks(
        conn,
        insert_events_query,
        events,
        get_config_table_chunk_size::<EventPG>("events", per_table_chunk_sizes),
    )
    .await?;
    Ok(())
}

fn insert_events_query(
    items_to_insert: Vec<EventPG>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::events::dsl::*;
    (
        diesel::insert_into(schema::events::table)
            .values(items_to_insert)
            .on_conflict((transaction_version, event_index))
            .do_update()
            .set((
                inserted_at.eq(excluded(inserted_at)),
                indexed_type.eq(excluded(indexed_type)),
            )),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for EventsProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::EventsProcessor.into()
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

        let events = process_transactions(transactions);

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
            &events,
        )
        .await;

        if mq_result.is_err() {
            bail!(
                "Error sending event to mq. Processor {}. Start {}. End {}. Error {:?}",
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
            &events,
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

pub fn process_transactions(transactions: Vec<Transaction>) -> Vec<EventPG> {
    let mut events = vec![];
    for txn in &transactions {
        let txn_events: Vec<EventPG> = parse_events(txn, "EventsProcessor")
            .into_iter()
            .map(|e| e.into())
            .collect();
        events.extend(txn_events);
    }
    events
}
