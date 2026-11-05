// SPDX-License-Identifier: GPL-3.0-or-later

//! The committed JSON schema must match the one generated from the Rust types.
//!
//! To refresh it after changing the PartSpec types, run:
//! `KIRPITCH_UPDATE_SCHEMA=1 cargo test -p kirpitch-core --test schema`

use std::fs;
use std::path::Path;

use kirpitch_core::partspec::json_schema_string;

fn schema_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/partspec.schema.json")
}

#[test]
fn committed_schema_is_up_to_date() {
    let generated = json_schema_string();
    let path = schema_path();
    if std::env::var_os("KIRPITCH_UPDATE_SCHEMA").is_some() {
        fs::write(&path, &generated).expect("write schema");
        return;
    }
    let committed = fs::read_to_string(&path).expect(
        "schema/partspec.schema.json is missing; run with KIRPITCH_UPDATE_SCHEMA=1 to create it",
    );
    assert_eq!(
        committed.replace("\r\n", "\n"),
        generated,
        "schema/partspec.schema.json is stale; run `KIRPITCH_UPDATE_SCHEMA=1 cargo test -p kirpitch-core --test schema`"
    );
}

#[test]
fn schema_describes_the_important_rules() {
    let schema: serde_json::Value = serde_json::from_str(&json_schema_string()).unwrap();
    assert_eq!(schema["type"], "object");
    let required = schema["required"].as_array().unwrap();
    for field in [
        "schema_version",
        "metadata",
        "package",
        "pins",
        "land_pattern",
    ] {
        assert!(required.iter().any(|r| r == field), "{field} is required");
    }
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["$defs"]["Mm"]["type"], "number");
    assert!(schema["$defs"]["Pin"]["required"].is_array());
}
