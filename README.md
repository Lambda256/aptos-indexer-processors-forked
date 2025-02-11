# Aptos Indexer Processors (Fork)
This repository is a fork of the [Aptos Indexer Processors](https://github.com/aptos-labs/aptos-indexer-processors) project, aimed at enhancing the existing functionalities and improving performance. The project is designed to efficiently index and process data from the Aptos blockchain.

To set up and run the project, please refer to the following link for detailed instructions: [Getting Started with Aptos Indexer Processors](https://github.com/aptos-labs/aptos-indexer-processors/tree/main/rust/processor).

## Enhanced Features
- **Postgres Performance Improvement**:
  Numerous optimizations has been made to existing postgres tables to increase GraphQL query performance, and as a result, `fungible_asset_balances` table is now exposed in GraphQL schema.


- **Kafka Data Storage**: Added functionality to store data in Kafka when the broker is specified in the configuration. This feature supports the implementation of the Aptos event stream service.  

## Configuration

- `config.yaml`
  - `chain_id`: ID of the chain used for validation purposes.
  - `grpc_data_stream_endpoint`: Replace with the grpc data stream endpoints for mainnet, devnet, testnet, or previewnet.
    - mainnet: https://grpc.mainnet.aptoslabs.com:443
    - testnet: https://grpc.testnet.aptoslabs.com:443
  - `grpc_data_stream_api_key`: Replace `YOUR_TOKEN` with your auth token.
  - `db_connection_uri`: The DB connection used to write the processed data
  - (optional) `starting-version`
    - If `starting-version` is set, the processor will begin indexing from transaction version = `starting_version`.
    - To auto restart the client in case of an error, you can cache the latest processed transaction version. In the example, the processor restarts from cached transaction version that is stored in a table, and if neither `starting_version` nor cached version are set, the processor defaults starting version to 0.
  - (optional) `brokers`
    - endpoint of the Kafka brokers
    - If brokers are set, the processor will publish the processed data to the Kafka topic.
    - Name of Kafka topic is `aptos.{mainnet/testnet}.{model name}`.
      - e.g. `aptos.mainnet.account.transactions.processor`

  Your `config.yaml` should look like this: 
  ```yaml
  # config.yaml
  health_check_port: 8080
  server_config:
    processor_config:
      type: account_transactions_processor
    transaction_filter:
      focus_user_transactions: false
    brokers: localhost-1:9092,localhost-2:9092,localhost-3:9092   # optional, replace with your Kafka brokers
    postgres_connection_string: postgres://XXXXX   # replace with your db connection string
    indexer_grpc_data_service_address: https://grpc.mainnet.aptoslabs.com:443
    indexer_grpc_http2_ping_interval_in_secs: 60
    indexer_grpc_http2_ping_timeout_in_secs: 10
    number_concurrent_processing_tasks: 3
    auth_token: aptoslabs_XXXXXX   # replace with your auth token
  ```
      

## Response

- The response is a stream of `RawDatastreamResponse` objects.
- To learn more about the protos and the code generated from those protos see [protos/](https://github.com/aptos-labs/aptos-core/tree/main/protos) in aptos-core.


# Aptos Core Processors
These are the core processors that index data for the [Indexer API](https://aptos.dev/en/build/indexer/aptos-hosted). 
The core processors live in the `sdk-processor` crate and are written in Rust using the [Indexer SDK](https://aptos.dev/en/build/indexer/indexer-sdk).
Read more about indexing on Aptos [here](https://aptos.dev/en/build/indexer).
If you want to create a custom processor to index your contract, start with our [Quickstart Guide](https://aptos.dev/en/build/indexer/indexer-sdk/quickstart).

> [!WARNING]  
> For production-grade indexers, we recommend using the [Indexer SDK](https://aptos.dev/en/build/indexer/indexer-sdk) to write your custom processor.
> The Python implementation is known to have a grpc deserialization recursion limit. The issue is with the GRPC library and we haven't had a chance to look into this. Please proceed with caution.
> The typescript implementation is known to get stuck when there are lots of data to process. The issue is with the GRPC client and we haven't had a chance to optimize. Please proceed with caution.
