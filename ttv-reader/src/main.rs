// TTV File Reader - Inspects Puffin format (.ttv) files
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

// cargo r -- ~/Downloads/740263305421835878456d4.ttv
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_ttv_file>", args[0]);
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    println!("\n📄 File: {}", file_path.display());

    let data = fs::read(&file_path)?;
    println!(
        "📊 Size: {} bytes ({:.2} KB)\n",
        data.len(),
        data.len() as f64 / 1024.0
    );

    // Check Puffin magic header
    if data.len() > 4 {
        let header = &data[0..4];
        if header == b"PFA1" {
            println!("✓ Valid Puffin Archive Format v1\n");
        } else {
            println!(
                "⚠ Header: {:02X} {:02X} {:02X} {:02X} (expected PFA1)\n",
                header[0], header[1], header[2], header[3]
            );
        }
    }

    // Parse and extract embedded files
    if let Some(metadata) = parse_metadata(&data)? {
        // Analyze the index content
        analyze_index(&data, &metadata)?;

        println!("\n📦 Embedded Files:\n");

        if let Some(blobs) = metadata["blobs"].as_array() {
            for (i, blob) in blobs.iter().enumerate() {
                let blob_tag = blob["properties"]["blob_tag"].as_str().unwrap_or("unknown");
                let offset = blob["offset"].as_u64().unwrap_or(0) as usize;
                let length = blob["length"].as_u64().unwrap_or(0) as usize;
                let blob_type = blob["type"].as_str().unwrap_or("unknown");

                println!(
                    "{}. {} ({} bytes at offset {})",
                    i + 1,
                    blob_tag,
                    length,
                    offset
                );
                println!("   Type: {}", blob_type);

                // Extract and analyze the embedded file
                if offset + length <= data.len() {
                    let file_data = &data[offset..offset + length];

                    // Try to parse as JSON if it's meta.json
                    if blob_tag.contains("meta.json") {
                        if let Ok(json_str) = std::str::from_utf8(file_data) {
                            if let Ok(parsed) = serde_json::from_str::<Value>(json_str) {
                                println!("   Content: {}", serde_json::to_string_pretty(&parsed)?);
                            }
                        }
                    } else if blob_tag.contains(".term") {
                        // Decode term dictionary
                        print!("   Preview: ");
                        for (i, &b) in file_data.iter().take(32).enumerate() {
                            if i > 0 && i % 16 == 0 {
                                print!("\n            ");
                            }
                            print!("{:02x} ", b);
                        }
                        println!();

                        // Try to extract readable terms
                        if let Some(terms) = extract_terms(file_data) {
                            println!("   Terms found: {}", terms.join(", "));
                        }
                    } else {
                        // Show hex preview for binary files
                        print!("   Preview: ");
                        for (i, &b) in file_data.iter().take(32).enumerate() {
                            if i > 0 && i % 16 == 0 {
                                print!("\n            ");
                            }
                            print!("{:02x} ", b);
                        }
                        if length > 32 {
                            print!("... ({} more bytes)", length - 32);
                        }
                        println!();
                    }
                } else {
                    println!("   ⚠ Invalid offset/length!");
                }
                println!();
            }
        }

        println!(
            "\n📋 Full Metadata:\n{}",
            serde_json::to_string_pretty(&metadata)?
        );
    }

    // Hex dump of header
    println!("\n🔢 Hex (first 128 bytes):");
    for (i, chunk) in data[..std::cmp::min(128, data.len())]
        .chunks(16)
        .enumerate()
    {
        print!("{:04x}: ", i * 16);
        for b in chunk {
            print!("{:02x} ", b);
        }
        println!();
    }

    Ok(())
}

fn analyze_index(data: &[u8], metadata: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // Find and parse meta.json
    if let Some(blobs) = metadata["blobs"].as_array() {
        for blob in blobs {
            let blob_tag = blob["properties"]["blob_tag"].as_str().unwrap_or("");
            if blob_tag.contains("meta.json") {
                let offset = blob["offset"].as_u64().unwrap_or(0) as usize;
                let length = blob["length"].as_u64().unwrap_or(0) as usize;

                if offset + length <= data.len() {
                    let file_data = &data[offset..offset + length];
                    if let Ok(json_str) = std::str::from_utf8(file_data) {
                        if let Ok(meta) = serde_json::from_str::<Value>(json_str) {
                            println!("🔍 Index Analysis:\n");

                            // Check schema
                            if let Some(schema) = meta["schema"].as_array() {
                                println!("Fields defined:");
                                for field in schema {
                                    let name = field["name"].as_str().unwrap_or("unknown");
                                    let field_type = field["type"].as_str().unwrap_or("unknown");
                                    let indexed = field["options"]["indexing"].is_object();
                                    let stored =
                                        field["options"]["stored"].as_bool().unwrap_or(false);

                                    println!(
                                        "  - {} (type: {}, indexed: {}, stored: {})",
                                        name, field_type, indexed, stored
                                    );
                                }
                            }

                            // Check segments
                            if let Some(segments) = meta["segments"].as_array() {
                                println!("\nSegments:");
                                let mut total_docs = 0;
                                for segment in segments {
                                    let max_doc = segment["max_doc"].as_u64().unwrap_or(0);
                                    let segment_id =
                                        segment["segment_id"].as_str().unwrap_or("unknown");
                                    total_docs += max_doc;
                                    println!("  - {} ({} documents)", segment_id, max_doc);
                                }

                                println!("\n📈 Total documents: {}", total_docs);

                                if total_docs == 0 {
                                    println!("⚠️  Index is EMPTY - no documents indexed");
                                } else {
                                    println!("✓ Index contains data");
                                }
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    Ok(())
}

fn extract_terms(data: &[u8]) -> Option<Vec<String>> {
    let mut terms = Vec::new();
    let mut current = Vec::new();

    // Simple heuristic: look for printable ASCII sequences
    for &b in data {
        if b >= 32 && b <= 126 {
            current.push(b);
        } else if !current.is_empty() && current.len() >= 3 {
            if let Ok(s) = String::from_utf8(current.clone()) {
                terms.push(s);
            }
            current.clear();
        } else {
            current.clear();
        }
    }

    if !terms.is_empty() {
        Some(terms)
    } else {
        None
    }
}

fn parse_metadata(data: &[u8]) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    if let Some(start) = find_json(data) {
        if let Some(end) = find_json_end(data, start) {
            if let Ok(json_str) = std::str::from_utf8(&data[start..end]) {
                if let Ok(parsed) = serde_json::from_str::<Value>(json_str) {
                    return Ok(Some(parsed));
                }
            }
        }
    }
    Ok(None)
}

fn find_json(data: &[u8]) -> Option<usize> {
    let start = if data.len() > 10000 {
        data.len() - 10000
    } else {
        0
    };
    data[start..]
        .windows(8)
        .position(|w| w == b"{\"blobs\"")
        .map(|p| p + start)
}

fn find_json_end(data: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, &b) in data[start..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}
