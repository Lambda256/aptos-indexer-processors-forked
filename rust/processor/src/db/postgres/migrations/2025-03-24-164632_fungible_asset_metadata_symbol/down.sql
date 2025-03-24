DROP VIEW IF EXISTS legacy_migration_v1.coin_infos;
ALTER TABLE fungible_asset_metadata ALTER COLUMN symbol TYPE VARCHAR(10);

-- replace `coin_infos` with `fungible_asset_metadata`
CREATE OR REPLACE VIEW legacy_migration_v1.coin_infos AS
SELECT encode(sha256(asset_type::bytea), 'hex') as coin_type_hash,
    asset_type as coin_type,
    last_transaction_version as transaction_version_created,
    creator_address,
    name,
    symbol,
    decimals,
    last_transaction_timestamp as transaction_created_timestamp,
    inserted_at,
    supply_aggregator_table_handle_v1 as supply_aggregator_table_handle,
    supply_aggregator_table_key_v1 as supply_aggregator_table_key
FROM public.fungible_asset_metadata
WHERE token_standard = 'v1';
