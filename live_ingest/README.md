# Live Ingest

A live data ingestion tool for streaming Olympics data to an API endpoint. This tool reads records from `olympics.json` and continuously sends them to a specified API endpoint with different field configurations.

## Features

- Ingests 2 random Olympic records per second
- Supports multiple operational modes with different field configurations
- Adds unique identifiers and timestamps to each record
- Basic authentication support
- Real-time console logging of ingestion status

## Prerequisites

- Rust (latest stable version)
- Access to the target API endpoint (default: `http://localhost:5080/api/default/oly/_json`)
- `olympics.json` file in the parent directory

## Installation

```bash
cd live_ingest
cargo build --release
```

## Running Modes

### Normal Mode (Default)

Includes all base Olympic fields plus the `body` field with descriptive text.

```bash
cargo run
```

or

```bash
cargo r
```

**Fields included:**
- `id`, `name`, `continent`, `flag_url`
- `gold_medals`, `silver_medals`, `bronze_medals`, `total_medals`
- `rank`, `rank_total_medals`
- `body` (text field with Olympic achievement descriptions)
- `unique_id` (UUID)
- `_timestamp` (microsecond timestamp)

### No-FTS Mode

Excludes the `body` field - useful for testing without full-text search fields.

```bash
cargo run -- no-fts
```

or

```bash
cargo r -- no-fts
```

**Fields included:**
- All base Olympic fields (id, name, medals, ranks, etc.)
- `unique_id` (UUID)
- `_timestamp` (microsecond timestamp)
- ❌ `body` field is excluded

### New-Fields Mode

Includes all fields from normal mode plus 3 additional randomly generated fields based on Olympic data.

```bash
cargo run -- new-fields
```

or

```bash
cargo r -- new-fields
```

**Additional fields:**
- `sport_category` (text) - Randomly selected Olympic sport category
  - Examples: "Aquatics", "Athletics", "Gymnastics", "Combat Sports", etc.
- `athlete_highlight` (text) - Randomly selected achievement description
  - Examples: "Record-breaking performance in finals", "Youngest medalist in team history", etc.
- `participation_year` (integer) - Random Olympic year between 2000-2024

## Configuration

The following constants can be modified in `main.rs`:

```rust
const API_URL: &str = "http://localhost:5080/api/default/oly/_json";
const USERNAME: &str = "root@example.com";
const PASSWORD: &str = "Complexpass#123";
const RECORDS_PER_SECOND: usize = 2;
```

## Output Example

```
Reading olympics data from ../olympics.json...
Loaded 53 records from olympics.json
Starting live ingestion: 2 records per second
API: http://localhost:5080/api/default/oly/_json
Mode: new-fields (includes body + sport_category, athlete_highlight, participation_year)
Press Ctrl+C to stop

[2025-12-15 10:30:45] ✓ Ingested 2 records (IDs: a3b2c4d5, e6f7g8h9)
[2025-12-15 10:30:46] ✓ Ingested 2 records (IDs: j1k2l3m4, n5o6p7q8)
```

## Stopping the Ingestion

Press `Ctrl+C` to stop the live ingestion process at any time.

## Error Handling

The tool will log errors if:
- The API endpoint is unavailable
- Authentication fails
- Network issues occur
- The `olympics.json` file is missing or malformed

Errors are printed to stderr with timestamps and will not stop the ingestion process.

## Data Flow

1. Tool reads all records from `../olympics.json`
2. Every second, it selects 2 random records
3. Each record is enriched with:
   - A unique UUID (`unique_id`)
   - Current timestamp in microseconds (`_timestamp`)
   - Optional fields based on the mode
4. Records are sent as a JSON array to the API endpoint via POST request
5. Response status is logged to console

## Development

To run in development mode with cargo watching for file changes:

```bash
cargo install cargo-watch
cargo watch -x run
```

## License

This project is part of the local_db repository.
