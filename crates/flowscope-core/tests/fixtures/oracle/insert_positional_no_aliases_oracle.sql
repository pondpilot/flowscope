-- INSERT positional, no aliases (Oracle-specific)
INSERT INTO archive.user_logs (log_seq, username_upper, event_ts)
SELECT log_id, UPPER(username), event_timestamp
FROM public.users WHERE is_deleted = 1;
