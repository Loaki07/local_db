#!/bin/bash

# Configurable settings
DAYS_TO_GENERATE=10        # Number of days to generate data for
INTERVAL_SECONDS=10        # Interval between each record in seconds
INPUT_FILE="olympics.json" # Input JSON file
OUTPUT_FILE="output.json"  # Output JSON file

# Helper function to generate a unique ID (UUID)
generate_uuid() {
  uuidgen
}

# Helper function to generate a microsecond timestamp
generate_timestamp() {
  local seconds=$(date +%s)      # Current time in seconds
  local nanoseconds=$(date +%N) # Current time in nanoseconds
  echo "$((seconds * 1000000 + nanoseconds / 1000))" # Convert to microseconds
}

# Calculate the total number of records to generate
SECONDS_IN_A_DAY=$((24 * 60 * 60))
TOTAL_RECORDS=$((DAYS_TO_GENERATE * SECONDS_IN_A_DAY / INTERVAL_SECONDS))

# Read the input JSON file
if [[ ! -f "$INPUT_FILE" ]]; then
  echo "Error: Input file '$INPUT_FILE' not found!"
  exit 1
fi

# Load the input JSON into a variable
INPUT_JSON=$(cat "$INPUT_FILE")

# Initialize the output JSON array
echo "[" > "$OUTPUT_FILE"

# Get the starting timestamp (current time)
CURRENT_TIMESTAMP=$(generate_timestamp)

# Loop to generate data
for ((i = 0; i < TOTAL_RECORDS; i++)); do
  # Calculate the record index (loop through the input JSON records)
  RECORD_INDEX=$((i % $(echo "$INPUT_JSON" | jq 'length')))

  # Extract the current record from the input JSON
  RECORD=$(echo "$INPUT_JSON" | jq ".[$RECORD_INDEX]")

  # Generate unique_id
  UNIQUE_ID=$(generate_uuid)

  # Calculate the timestamp for this record (going backward in time)
  TIMESTAMP=$((CURRENT_TIMESTAMP - (i * INTERVAL_SECONDS * 1000000)))

  # Add the new fields to the record
  UPDATED_RECORD=$(echo "$RECORD" | jq --arg unique_id "$UNIQUE_ID" --argjson timestamp "$TIMESTAMP" \
    '. + {unique_id: $unique_id, _timestamp: $timestamp}')

  # Append the updated record to the output JSON
  echo "$UPDATED_RECORD" >> "$OUTPUT_FILE"

  # Add a comma between records, except for the last one
  if [[ $i -lt $((TOTAL_RECORDS - 1)) ]]; then
    echo "," >> "$OUTPUT_FILE"
  fi
done

# Close the JSON array
echo "]" >> "$OUTPUT_FILE"

echo "Data generation complete! Output written to '$OUTPUT_FILE'."

