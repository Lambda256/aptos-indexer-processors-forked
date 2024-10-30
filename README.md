# Aptos Indexer Processors (Fork)
This repository is a fork of the [Aptos Indexer Processors](https://github.com/aptos-labs/aptos-indexer-processors) project, aimed at enhancing the existing functionalities and improving performance. The project is designed to efficiently index and process data from the Aptos blockchain.

## Enhanced Features
- **Fungible Asset Balances Performance Improvement**: 
The performance of the `fungible_asset_balances` table has been optimized. This enhancement allows for faster queries and improved overall efficiency, and the updated table is now exposed through Hasura.


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
    - Name of Kafka topic is `aptos.{mainnet/testnet}.{processor name}`.
      - i.e.) `aptos.mainnet.account.transactions.processor`

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