-- Comment containing closing bracket/paren that should not affect parsing
SELECT a
/*
)
*/
FROM b;

-- Comment with multiple brackets and parentheses
SELECT id, name
/*
  This comment has: (parentheses), [brackets], and {braces}
  ) unbalanced ) paren
*/
FROM users;

-- Single-line comment with bracket before FROM
SELECT x -- )
FROM y;
