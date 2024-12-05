#!/usr/bin/env fish

# Define the JSON file and API endpoint
set json_file "olympics.json"
set api_endpoint "http://localhost:5080/api/team1/default/_json"
set auth "root@example.com:Complexpass#123"

# Function to upload a chunk of JSON data
function upload_chunk
    set chunk_file "chunk.json"
    # Write the chunk to a temporary file
    echo $argv > $chunk_file
    # Use curl to upload the chunk
    curl -i -u $auth -H "Content-Type: application/json" -d "@$chunk_file" $api_endpoint
    # Remove the temporary chunk file
    rm $chunk_file
end

# Validate the input JSON file
if not test -f $json_file
    echo "Error: JSON file '$json_file' not found!"
    exit 1
end

# Read the JSON file and split it into chunks of 10 records
set total_records (jq '. | length' $json_file) # Get the total number of records
set chunk_size 10
set start_index 0

# Loop through the JSON file in chunks of 10
while test $start_index -lt $total_records
    # Extract a chunk of 10 records using jq
    set chunk (jq ".[$start_index:$start_index+$chunk_size]" $json_file)

    # Ensure the chunk is valid JSON
    if not echo $chunk | jq empty
        echo "Error: Invalid JSON chunk generated!"
        exit 1
    end

    # Upload the current chunk
    upload_chunk "$chunk"

    # Increment the start index by the chunk size
    set start_index (math "$start_index + $chunk_size")

    # Wait for 10 minutes before the next upload
    echo "Uploaded chunk. Waiting for 1 minutes before the next upload..."
    sleep 60
end

echo "All chunks uploaded successfully!"

