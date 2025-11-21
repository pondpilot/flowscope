-- INSERT INTO SELECT
INSERT INTO public.user_archive (id, name, archived_at)
SELECT id, name, NOW()
FROM public.users
WHERE last_login < CURRENT_DATE - INTERVAL '1 year';
