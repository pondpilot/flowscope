-- BigQuery hyphenated project/dataset identifiers
-- BigQuery requires backtick quoting for hyphenated names

-- Basic hyphenated project and dataset reference
SELECT id, name
FROM `project-a.dataset-b.users`;

-- UDF call with hyphenated project reference
SELECT
    `another-gcp-project`.functions.parse_timestamp(created_at) AS parsed_ts,
    user_id
FROM `data-project.analytics.events`;

-- Three-part hyphenated identifiers in join
SELECT
    u.user_id,
    o.order_total
FROM `my-company.core.users` u
JOIN `my-company.sales.orders` o ON u.user_id = o.user_id;
