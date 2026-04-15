-- CREATE TABLE AS SELECT with CTE and USING join (Oracle-specific)
CREATE TABLE dm.monthly_sales AS
WITH raw_data AS (
    SELECT * FROM sales.transactions t JOIN dims.calendar c USING (date_id)
)
SELECT month_name, SUM(amount) AS total_amount
FROM raw_data GROUP BY month_name;
