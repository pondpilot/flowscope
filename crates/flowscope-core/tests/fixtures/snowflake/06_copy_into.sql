-- COPY INTO table from stage (loading data)
COPY INTO analytics.orders
FROM @my_stage/orders/
FILE_FORMAT = (TYPE = 'CSV');

-- COPY INTO location from table (unloading data)
COPY INTO @my_stage/export/
FROM analytics.orders
FILE_FORMAT = (TYPE = 'PARQUET');
