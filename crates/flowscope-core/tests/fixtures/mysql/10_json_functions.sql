-- MySQL JSON functions
SELECT
    id,
    name,
    JSON_EXTRACT(metadata, '$.category') AS category,
    JSON_UNQUOTE(JSON_EXTRACT(metadata, '$.tags[0]')) AS first_tag,
    JSON_LENGTH(JSON_EXTRACT(metadata, '$.tags')) AS tag_count
FROM products
WHERE JSON_CONTAINS(metadata, '"featured"', '$.tags');
