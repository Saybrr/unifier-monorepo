#!/bin/bash

# Script to extract distinct $type values from a Wabbajack modlist JSON file
# Equivalent to: SELECT DISTINCT $type FROM json_data

# Default file path
JSON_FILE="Baseline/modlist"

# Allow file path to be passed as command line argument
if [ $# -eq 1 ]; then
    JSON_FILE="$1"
fi

# Check if file exists
if [ ! -f "$JSON_FILE" ]; then
    echo "Error: File '$JSON_FILE' not found."
    exit 1
fi

echo "Extracting distinct \$type values from: $JSON_FILE"
echo "------------------------------------------------------------"

# Extract $type values from Archives array only
# 1. sed: Extract just the Archives array section from the JSON
# 2. grep: Find lines containing "$type" within that section
# 3. sed: Extract just the value between quotes after "$type": "
# 4. sort: Sort the values
# 5. uniq: Remove duplicates
DISTINCT_TYPES=$(sed -n '/"Archives": \[/,/^\s*\]/p' "$JSON_FILE" | \
                 grep '\$type' | \
                 sed 's/.*"\$type": "\([^"]*\)".*/\1/' | \
                 sort | \
                 uniq)

# Count the distinct types
COUNT=$(echo "$DISTINCT_TYPES" | wc -l)

echo "Found $COUNT distinct \$type values:"
echo

# Display results with numbering
echo "$DISTINCT_TYPES" | nl -w2 -s'. '

echo
echo "Total distinct types: $COUNT"

# Alternative one-liner version (commented out):
# echo "One-liner version:"
# echo "grep '\$type' \"$JSON_FILE\" | sed 's/.*\"\$type\": \"\([^\"]*\)\".*/\1/' | sort | uniq -c | sort -nr"
