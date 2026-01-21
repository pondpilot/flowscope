-- Basic COPY from file to table
COPY users FROM 's3://bucket/users.csv';

-- COPY to file from table
COPY users TO '/tmp/users_backup.csv';

-- COPY with query (exporting to file)
COPY (SELECT id, name FROM users WHERE active = true) TO '/tmp/active_users.csv';
