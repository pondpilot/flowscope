-- Test: Jinja conditional blocks
SELECT
    id,
    name
    {% if include_email %}
    , email
    {% endif %}
FROM users
{% if active_only %}
WHERE active = true
{% endif %}
