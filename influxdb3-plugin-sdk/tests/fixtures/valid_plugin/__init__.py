def process_writes(influxdb3_local, table_batches, args):
    for batch in table_batches:
        influxdb3_local.info(f"rows={len(batch.rows)} table={batch.table_name}")
