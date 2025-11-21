-- UNION query
SELECT id, name, 'customer' as type FROM customers
UNION ALL
SELECT id, name, 'vendor' as type FROM vendors
UNION ALL
SELECT id, name, 'partner' as type FROM partners;
