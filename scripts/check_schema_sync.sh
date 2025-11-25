#!/bin/bash
# Check that the API schema is in sync with the Rust definitions

set -e

# Regenerate schema
node ./scripts/update_api_schema.cjs

# Check for changes
if ! git diff --exit-code docs/api_schema.json > /dev/null 2>&1; then
    echo "ERROR: API schema is out of sync with Rust definitions."
    echo "Run 'just update-schema' and commit the changes."
    exit 1
fi

echo "API schema is in sync."
