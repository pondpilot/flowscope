-- Redshift UNLOAD
UNLOAD ('SELECT * FROM analytics.orders WHERE order_date > ''2024-01-01''')
TO 's3://bucket/exports/orders_'
IAM_ROLE 'arn:aws:iam::123456789:role/RedshiftCopyRole';
