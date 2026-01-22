-- COPY from S3
COPY analytics.events
FROM 's3://data-lake/events/2024/'
IAM_ROLE 'arn:aws:iam::123456789:role/RedshiftCopyRole'
FORMAT AS PARQUET;
