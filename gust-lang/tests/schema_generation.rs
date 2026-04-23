use gust_lang::{SchemaCodegen, parse_program_with_errors};
use serde_json::Value;

/// Parse Gust source and generate JSON Schema, returning parsed JSON.
fn schema_for(source: &str) -> Value {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let json_str = SchemaCodegen::generate(&program);
    serde_json::from_str(&json_str).expect("output should be valid JSON")
}

/// Parse Gust source and generate JSON Schema filtered to a single machine.
fn schema_for_machine(source: &str, machine: &str) -> Value {
    let program = parse_program_with_errors(source, "test.gu").expect("source should parse");
    let json_str = SchemaCodegen::generate_filtered(&program, Some(machine));
    serde_json::from_str(&json_str).expect("output should be valid JSON")
}

// ---------------------------------------------------------------------------
// Schema envelope
// ---------------------------------------------------------------------------

#[test]
fn schema_has_draft_2020_12_meta() {
    let schema = schema_for("machine M { state S }");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[test]
fn schema_has_id_and_title() {
    let schema = schema_for("machine OrderProcessor { state S }");
    assert_eq!(schema["$id"], "gust://OrderProcessor");
    assert_eq!(schema["title"], "OrderProcessor Schema");
    assert_eq!(
        schema["description"],
        "JSON Schema generated from Gust source"
    );
}

// ---------------------------------------------------------------------------
// Struct types
// ---------------------------------------------------------------------------

#[test]
fn struct_type_generates_object_schema() {
    let schema = schema_for(
        r#"
type Order {
    id: String,
    customer: String,
    total: f64,
}
machine M { state S }
"#,
    );

    let order = &schema["$defs"]["Order"];
    assert_eq!(order["type"], "object");
    assert_eq!(order["properties"]["id"]["type"], "string");
    assert_eq!(order["properties"]["customer"]["type"], "string");
    assert_eq!(order["properties"]["total"]["type"], "number");

    let required = order["required"]
        .as_array()
        .expect("required should be array");
    assert_eq!(required.len(), 3);
    assert!(required.contains(&Value::String("id".to_string())));
    assert!(required.contains(&Value::String("customer".to_string())));
    assert!(required.contains(&Value::String("total".to_string())));
}

#[test]
fn struct_with_integer_and_bool_fields() {
    let schema = schema_for(
        r#"
type Config {
    count: i32,
    limit: u64,
    enabled: bool,
}
machine M { state S }
"#,
    );

    let config = &schema["$defs"]["Config"];
    assert_eq!(config["properties"]["count"]["type"], "integer");
    assert_eq!(config["properties"]["limit"]["type"], "integer");
    assert_eq!(config["properties"]["enabled"]["type"], "boolean");
}

// ---------------------------------------------------------------------------
// Enum types
// ---------------------------------------------------------------------------

#[test]
fn enum_type_generates_one_of_schema() {
    let schema = schema_for(
        r#"
enum Status {
    Pending,
    Active(String),
    Cancelled,
}
machine M { state S }
"#,
    );

    let status = &schema["$defs"]["Status"];
    let one_of = status["oneOf"].as_array().expect("oneOf should be array");
    assert_eq!(one_of.len(), 3);

    // Unit variant
    assert_eq!(one_of[0]["const"], "Pending");

    // Variant with payload
    assert_eq!(one_of[1]["type"], "object");
    assert_eq!(one_of[1]["properties"]["Active"]["type"], "string");

    // Another unit variant
    assert_eq!(one_of[2]["const"], "Cancelled");
}

#[test]
fn enum_variant_with_multiple_payloads_uses_tuple() {
    let schema = schema_for(
        r#"
enum Result {
    Ok(String, i64),
    Err(String),
}
machine M { state S }
"#,
    );

    let result = &schema["$defs"]["Result"];
    let one_of = result["oneOf"].as_array().expect("oneOf should be array");

    // Ok variant with 2 payloads becomes a tuple
    let ok_variant = &one_of[0];
    let ok_inner = &ok_variant["properties"]["Ok"];
    assert_eq!(ok_inner["type"], "array");
    let prefix = ok_inner["prefixItems"]
        .as_array()
        .expect("prefixItems should be array");
    assert_eq!(prefix.len(), 2);
    assert_eq!(prefix[0]["type"], "string");
    assert_eq!(prefix[1]["type"], "integer");
    assert_eq!(ok_inner["items"], false);
}

// ---------------------------------------------------------------------------
// Machine states
// ---------------------------------------------------------------------------

#[test]
fn machine_states_generate_prefixed_definitions() {
    let schema = schema_for(
        r#"
machine OrderProcessor {
    state Pending(order: String)
    state Validated(order: String, total: f64)
    state Failed(reason: String)
    transition validate: Pending -> Validated | Failed
    on validate() {
        goto Failed("err");
    }
}
"#,
    );

    let pending = &schema["$defs"]["OrderProcessor_Pending"];
    assert_eq!(pending["type"], "object");
    assert_eq!(pending["properties"]["order"]["type"], "string");

    let validated = &schema["$defs"]["OrderProcessor_Validated"];
    assert_eq!(validated["type"], "object");
    assert_eq!(validated["properties"]["order"]["type"], "string");
    assert_eq!(validated["properties"]["total"]["type"], "number");

    let failed = &schema["$defs"]["OrderProcessor_Failed"];
    assert_eq!(failed["type"], "object");
    assert_eq!(failed["properties"]["reason"]["type"], "string");
}

#[test]
fn machine_state_union_generated_as_one_of() {
    let schema = schema_for(
        r#"
machine OrderProcessor {
    state Pending(order: String)
    state Validated(total: f64)
    transition validate: Pending -> Validated
    on validate() {
        goto Validated(0.0);
    }
}
"#,
    );

    let state_union = &schema["$defs"]["OrderProcessor_State"];
    let one_of = state_union["oneOf"]
        .as_array()
        .expect("oneOf should be array");
    assert_eq!(one_of.len(), 2);

    // Each entry is a tagged object with $ref
    assert_eq!(
        one_of[0]["properties"]["Pending"]["$ref"],
        "#/$defs/OrderProcessor_Pending"
    );
    assert_eq!(one_of[0]["required"][0], "Pending");

    assert_eq!(
        one_of[1]["properties"]["Validated"]["$ref"],
        "#/$defs/OrderProcessor_Validated"
    );
    assert_eq!(one_of[1]["required"][0], "Validated");
}

// ---------------------------------------------------------------------------
// Option fields
// ---------------------------------------------------------------------------

#[test]
fn option_fields_are_not_in_required_array() {
    let schema = schema_for(
        r#"
type Config {
    name: String,
    description: Option<String>,
    count: i32,
}
machine M { state S }
"#,
    );

    let config = &schema["$defs"]["Config"];
    let required = config["required"]
        .as_array()
        .expect("required should be array");

    // name and count should be required, description should not
    assert!(required.contains(&Value::String("name".to_string())));
    assert!(required.contains(&Value::String("count".to_string())));
    assert!(!required.contains(&Value::String("description".to_string())));
    assert_eq!(required.len(), 2);

    // The schema for the optional field should still be present
    assert_eq!(config["properties"]["description"]["type"], "string");
}

// ---------------------------------------------------------------------------
// Vec / List types
// ---------------------------------------------------------------------------

#[test]
fn vec_type_generates_array_schema() {
    let schema = schema_for(
        r#"
type Container {
    items: Vec<String>,
    numbers: Vec<i64>,
}
machine M { state S }
"#,
    );

    let container = &schema["$defs"]["Container"];
    assert_eq!(container["properties"]["items"]["type"], "array");
    assert_eq!(container["properties"]["items"]["items"]["type"], "string");
    assert_eq!(container["properties"]["numbers"]["type"], "array");
    assert_eq!(
        container["properties"]["numbers"]["items"]["type"],
        "integer"
    );
}

// ---------------------------------------------------------------------------
// Nested type references
// ---------------------------------------------------------------------------

#[test]
fn nested_type_references_use_ref() {
    let schema = schema_for(
        r#"
type Address {
    street: String,
}
type Customer {
    name: String,
    address: Address,
}
machine M { state S }
"#,
    );

    let customer = &schema["$defs"]["Customer"];
    assert_eq!(customer["properties"]["address"]["$ref"], "#/$defs/Address");
}

#[test]
fn machine_state_with_custom_type_field_uses_ref() {
    let schema = schema_for(
        r#"
type Order {
    id: String,
}
machine Proc {
    state Active(order: Order)
}
"#,
    );

    let active = &schema["$defs"]["Proc_Active"];
    assert_eq!(active["properties"]["order"]["$ref"], "#/$defs/Order");
}

// ---------------------------------------------------------------------------
// Unknown / unresolved types
// ---------------------------------------------------------------------------

#[test]
fn unknown_types_generate_empty_schema_with_description() {
    let schema = schema_for(
        r#"
type Wrapper {
    data: UnknownType,
}
machine M { state S }
"#,
    );

    let wrapper = &schema["$defs"]["Wrapper"];
    // Unknown types get a $ref (they might be defined elsewhere)
    assert_eq!(wrapper["properties"]["data"]["$ref"], "#/$defs/UnknownType");
}

#[test]
fn unknown_generic_types_generate_description() {
    let schema = schema_for(
        r#"
type Wrapper {
    data: HashMap<String, i64>,
}
machine M { state S }
"#,
    );

    let wrapper = &schema["$defs"]["Wrapper"];
    let data = &wrapper["properties"]["data"];
    assert!(
        data["description"]
            .as_str()
            .expect("should have description")
            .contains("HashMap")
    );
}

// ---------------------------------------------------------------------------
// Multiple machines
// ---------------------------------------------------------------------------

#[test]
fn multiple_machines_each_get_own_state_schemas() {
    let schema = schema_for(
        r#"
machine Alpha {
    state Running(count: i32)
    state Done
}
machine Beta {
    state Idle(name: String)
    state Active
}
"#,
    );

    // Alpha states
    assert!(schema["$defs"]["Alpha_Running"].is_object());
    assert!(schema["$defs"]["Alpha_State"].is_object());

    // Beta states
    assert!(schema["$defs"]["Beta_Idle"].is_object());
    assert!(schema["$defs"]["Beta_State"].is_object());

    // Should not overlap
    assert!(schema["$defs"]["Alpha_Idle"].is_null());
    assert!(schema["$defs"]["Beta_Running"].is_null());
}

#[test]
fn multiple_machines_schema_uses_generic_id() {
    let schema = schema_for(
        r#"
machine A { state S }
machine B { state T }
"#,
    );
    assert_eq!(schema["$id"], "gust://schema");
    assert_eq!(schema["title"], "Gust Schema");
}

// ---------------------------------------------------------------------------
// Machine filter
// ---------------------------------------------------------------------------

#[test]
fn machine_filter_only_includes_target_machine_states() {
    let schema = schema_for_machine(
        r#"
machine Alpha {
    state Running(count: i32)
}
machine Beta {
    state Idle(name: String)
}
"#,
        "Alpha",
    );

    assert!(schema["$defs"]["Alpha_Running"].is_object());
    assert!(schema["$defs"]["Alpha_State"].is_object());
    assert!(schema["$defs"]["Beta_Idle"].is_null());
    assert!(schema["$defs"]["Beta_State"].is_null());
    assert_eq!(schema["$id"], "gust://Alpha");
}

// ---------------------------------------------------------------------------
// Empty machine
// ---------------------------------------------------------------------------

#[test]
fn empty_machine_still_generates_valid_schema() {
    let schema = schema_for("machine Empty { }");

    // Should be valid JSON Schema with no state definitions
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    // No state union since there are no states
    assert!(schema["$defs"]["Empty_State"].is_null());
}

// ---------------------------------------------------------------------------
// Tuple types
// ---------------------------------------------------------------------------

#[test]
fn tuple_type_generates_prefix_items_schema() {
    let schema = schema_for(
        r#"
type Pair {
    value: (String, i64),
}
machine M { state S }
"#,
    );

    let pair = &schema["$defs"]["Pair"];
    let value = &pair["properties"]["value"];
    assert_eq!(value["type"], "array");
    let prefix = value["prefixItems"]
        .as_array()
        .expect("prefixItems should be array");
    assert_eq!(prefix.len(), 2);
    assert_eq!(prefix[0]["type"], "string");
    assert_eq!(prefix[1]["type"], "integer");
    assert_eq!(value["items"], false);
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[test]
fn result_type_generates_one_of_schema() {
    let schema = schema_for(
        r#"
type Response {
    outcome: Result<String, i32>,
}
machine M { state S }
"#,
    );

    let response = &schema["$defs"]["Response"];
    let outcome = &response["properties"]["outcome"];
    let one_of = outcome["oneOf"].as_array().expect("oneOf should be array");
    assert_eq!(one_of.len(), 2);
    assert_eq!(one_of[0]["type"], "string");
    assert_eq!(one_of[1]["type"], "integer");
}

// ---------------------------------------------------------------------------
// Unit type
// ---------------------------------------------------------------------------

#[test]
fn unit_type_generates_null_schema() {
    let schema = schema_for(
        r#"
type Wrapper {
    empty: (),
}
machine M { state S }
"#,
    );

    let wrapper = &schema["$defs"]["Wrapper"];
    assert_eq!(wrapper["properties"]["empty"]["type"], "null");
}

// ---------------------------------------------------------------------------
// State with no fields
// ---------------------------------------------------------------------------

#[test]
fn state_with_no_fields_generates_empty_object() {
    let schema = schema_for(
        r#"
machine Simple {
    state Idle
    state Running
}
"#,
    );

    let idle = &schema["$defs"]["Simple_Idle"];
    assert_eq!(idle["type"], "object");
    // No required array needed for empty object
    assert!(idle.get("required").is_none() || idle["required"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Output is valid JSON
// ---------------------------------------------------------------------------

#[test]
fn output_is_valid_pretty_json() {
    let program = parse_program_with_errors(
        r#"
type Order { id: String }
machine M { state S(order: Order) }
"#,
        "test.gu",
    )
    .expect("source should parse");

    let json_str = SchemaCodegen::generate(&program);

    // Should be pretty-printed (contains newlines and indentation)
    assert!(json_str.contains('\n'));
    assert!(json_str.contains("  "));

    // Should parse as valid JSON
    let _: Value = serde_json::from_str(&json_str).expect("should be valid JSON");
}

// ---------------------------------------------------------------------------
// Option field in machine state
// ---------------------------------------------------------------------------

#[test]
fn option_field_in_state_not_required() {
    let schema = schema_for(
        r#"
machine Proc {
    state Active(name: String, note: Option<String>)
}
"#,
    );

    let active = &schema["$defs"]["Proc_Active"];
    let required = active["required"]
        .as_array()
        .expect("required should be array");
    assert!(required.contains(&Value::String("name".to_string())));
    assert!(!required.contains(&Value::String("note".to_string())));
}
