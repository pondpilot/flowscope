-- Expression recursion stress test with 40+ OR conditions
-- Tests parser stack depth handling for deeply nested boolean expressions

SELECT
    id,
    name,
    status
FROM items
WHERE
    status = 'a' OR status = 'b' OR status = 'c' OR status = 'd' OR status = 'e'
    OR status = 'f' OR status = 'g' OR status = 'h' OR status = 'i' OR status = 'j'
    OR status = 'k' OR status = 'l' OR status = 'm' OR status = 'n' OR status = 'o'
    OR status = 'p' OR status = 'q' OR status = 'r' OR status = 's' OR status = 't'
    OR status = 'u' OR status = 'v' OR status = 'w' OR status = 'x' OR status = 'y'
    OR status = 'z' OR status = 'aa' OR status = 'ab' OR status = 'ac' OR status = 'ad'
    OR status = 'ae' OR status = 'af' OR status = 'ag' OR status = 'ah' OR status = 'ai'
    OR status = 'aj' OR status = 'ak' OR status = 'al' OR status = 'am' OR status = 'an'
    OR status = 'ao' OR status = 'ap';
