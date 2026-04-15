-- INSERT with MINUS set operator (Oracle-specific)
INSERT INTO dq.missing_transactions
SELECT * FROM stg.billing_system
MINUS
SELECT * FROM stg.crm_system;
