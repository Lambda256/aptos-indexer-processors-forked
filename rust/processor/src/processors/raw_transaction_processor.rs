// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    gap_detectors::ProcessingResult,
    utils::{
        database::ArcDbPool,
        mq::{CustomProducer, CustomProducerEnum},
        network::Network,
    },
};
use anyhow::bail;
use aptos_protos::transaction::v1::Transaction;
use async_trait::async_trait;

use crate::db::common::models::raw_transaction_model::raw_transactions::RawTransactionModel;
use std::fmt::Debug;

pub struct RawTransactionProcessor {
    producer: CustomProducerEnum,
    connection_pool: ArcDbPool,
}

impl RawTransactionProcessor {
    pub fn new(producer: CustomProducerEnum, connection_pool: ArcDbPool) -> Self {
        Self {
            producer,
            connection_pool,
        }
    }
}

impl Debug for RawTransactionProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RawTransactionProcessor")
    }
}

async fn produce_to_mq(
    producer: &CustomProducerEnum,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    network: String,
    raw_transactions: &[RawTransactionModel],
) -> Result<(), String> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Producing to mq",
    );

    let ut_topic = format!("aptos.{}.raw.transactions", network);
    producer
        .send_to_mq(ut_topic.as_str(), raw_transactions)
        .await?;

    Ok(())
}

#[async_trait]
impl ProcessorTrait for RawTransactionProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::RawTransactionProcessor.into()
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
        let mq_production_start = std::time::Instant::now();

        let mut raw_transactions = vec![];
        for txn in &transactions {
            let raw_transaction = RawTransactionModel::from_transaction(txn);
            raw_transactions.push(raw_transaction);
        }

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();

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
            &raw_transactions,
        )
        .await;
        let db_insertion_duration_in_secs = mq_production_start.elapsed().as_secs_f64();
        match mq_result {
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
                bail!(e)
            },
        }
    }

    fn connection_pool(&self) -> &ArcDbPool {
        &self.connection_pool
    }
}
