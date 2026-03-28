//! JSON Schema (draft 2020-12) generator for Gust type declarations and machine states.
//!
//! Produces a JSON Schema document with `$defs` containing:
//! - Object schemas for struct types
//! - Tagged-union (`oneOf`) schemas for enum types
//! - Per-machine state definitions (`{Machine}_{State}`)
//! - Per-machine state union (`{Machine}_State`)

use crate::ast::*;
use serde_json::{json, Map, Value};

/// JSON Schema code generator.
///
/// Consumes a parsed `Program` and emits a JSON Schema string
/// (draft 2020-12) describing all type declarations and machine states.
pub struct SchemaCodegen;

impl SchemaCodegen {
    /// Generate a JSON Schema document covering all types and machines in the program.
    pub fn generate(program: &Program) -> String {
        Self::generate_filtered(program, None)
    }

    /// Generate a JSON Schema document, optionally filtering to a single machine.
    ///
    /// When `machine_filter` is `Some`, only that machine's state definitions are
    /// emitted (type declarations referenced by the machine are still included).
    pub fn generate_filtered(program: &Program, machine_filter: Option<&str>) -> String {
        let mut defs = Map::new();

        // Emit type declarations (structs and enums)
        for type_decl in &program.types {
            match type_decl {
                TypeDecl::Struct { name, fields } => {
                    defs.insert(name.clone(), Self::struct_schema(fields));
                }
                TypeDecl::Enum { name, variants } => {
                    defs.insert(name.clone(), Self::enum_schema(variants));
                }
            }
        }

        // Determine which machines to process
        let machines: Vec<&MachineDecl> = match machine_filter {
            Some(name) => program.machines.iter().filter(|m| m.name == name).collect(),
            None => program.machines.iter().collect(),
        };

        // Emit machine state definitions
        for machine in &machines {
            // Individual state schemas: {Machine}_{State}
            for state in &machine.states {
                let key = format!("{}_{}", machine.name, state.name);
                defs.insert(key, Self::state_schema(&state.fields));
            }

            // State union: {Machine}_State as oneOf
            if !machine.states.is_empty() {
                let key = format!("{}_State", machine.name);
                defs.insert(key, Self::state_union_schema(machine));
            }
        }

        // Determine $id based on machine filter or program scope
        let id = match machine_filter {
            Some(name) => format!("gust://{name}"),
            None if machines.len() == 1 => format!("gust://{}", machines[0].name),
            _ => "gust://schema".to_string(),
        };

        let title = match machine_filter {
            Some(name) => format!("{name} Schema"),
            None if machines.len() == 1 => format!("{} Schema", machines[0].name),
            _ => "Gust Schema".to_string(),
        };

        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": id,
            "title": title,
            "description": "JSON Schema generated from Gust source",
            "$defs": Value::Object(defs),
        });

        serde_json::to_string_pretty(&schema).expect("JSON serialization should not fail")
    }

    /// Build an object schema from a list of named fields (struct or state).
    fn struct_schema(fields: &[Field]) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in fields {
            let (schema, is_optional) = Self::type_expr_schema(&field.ty);
            properties.insert(field.name.clone(), schema);
            if !is_optional {
                required.push(Value::String(field.name.clone()));
            }
        }

        let mut obj = Map::new();
        obj.insert("type".to_string(), json!("object"));
        obj.insert("properties".to_string(), Value::Object(properties));
        if !required.is_empty() {
            obj.insert("required".to_string(), Value::Array(required));
        }
        Value::Object(obj)
    }

    /// Build an object schema from state fields (same structure as struct).
    fn state_schema(fields: &[Field]) -> Value {
        Self::struct_schema(fields)
    }

    /// Build a oneOf schema for an enum type.
    fn enum_schema(variants: &[EnumVariant]) -> Value {
        let one_of: Vec<Value> = variants.iter().map(Self::variant_schema).collect();
        json!({ "oneOf": one_of })
    }

    /// Build the schema for a single enum variant.
    ///
    /// - Unit variant (no payload): `{ "const": "VariantName" }`
    /// - Variant with payload: adjacently tagged object
    ///   `{ "type": "object", "properties": { "VariantName": <payload> }, "required": ["VariantName"] }`
    fn variant_schema(variant: &EnumVariant) -> Value {
        if variant.payload.is_empty() {
            json!({ "const": variant.name })
        } else if variant.payload.len() == 1 {
            // Single payload: wrap in tagged object
            let (inner, _) = Self::type_expr_schema(&variant.payload[0]);
            json!({
                "type": "object",
                "properties": {
                    &variant.name: inner
                },
                "required": [&variant.name]
            })
        } else {
            // Multiple payloads: represent as a tuple (prefixItems)
            let items: Vec<Value> = variant
                .payload
                .iter()
                .map(|t| Self::type_expr_schema(t).0)
                .collect();
            json!({
                "type": "object",
                "properties": {
                    &variant.name: {
                        "type": "array",
                        "prefixItems": items,
                        "items": false
                    }
                },
                "required": [&variant.name]
            })
        }
    }

    /// Build the state union schema for a machine as a oneOf of tagged state objects.
    fn state_union_schema(machine: &MachineDecl) -> Value {
        let one_of: Vec<Value> = machine
            .states
            .iter()
            .map(|state| {
                let ref_path = format!("#/$defs/{}_{}", machine.name, state.name);
                json!({
                    "type": "object",
                    "properties": {
                        &state.name: { "$ref": ref_path }
                    },
                    "required": [&state.name]
                })
            })
            .collect();
        json!({ "oneOf": one_of })
    }

    /// Convert a Gust `TypeExpr` to a JSON Schema value.
    ///
    /// Returns `(schema, is_optional)` where `is_optional` is true for `Option<T>`,
    /// meaning the field should be omitted from the `required` array.
    fn type_expr_schema(ty: &TypeExpr) -> (Value, bool) {
        match ty {
            TypeExpr::Unit => (json!({ "type": "null" }), false),

            TypeExpr::Simple(name) => (Self::simple_type_schema(name), false),

            TypeExpr::Generic(name, args) => Self::generic_type_schema(name, args),

            TypeExpr::Tuple(types) => {
                let items: Vec<Value> = types.iter().map(|t| Self::type_expr_schema(t).0).collect();
                (
                    json!({
                        "type": "array",
                        "prefixItems": items,
                        "items": false
                    }),
                    false,
                )
            }
        }
    }

    /// Map a simple (non-generic) type name to a JSON Schema.
    fn simple_type_schema(name: &str) -> Value {
        match name {
            "String" => json!({ "type": "string" }),
            "i32" | "i64" | "u32" | "u64" => json!({ "type": "integer" }),
            "f32" | "f64" => json!({ "type": "number" }),
            "bool" => json!({ "type": "boolean" }),
            // User-defined type: emit $ref
            _ => json!({ "$ref": format!("#/$defs/{name}") }),
        }
    }

    /// Map a generic type to a JSON Schema, returning `(schema, is_optional)`.
    fn generic_type_schema(name: &str, args: &[TypeExpr]) -> (Value, bool) {
        match name {
            "Option" if args.len() == 1 => {
                let (inner, _) = Self::type_expr_schema(&args[0]);
                (inner, true)
            }
            "Vec" | "List" if args.len() == 1 => {
                let (items_schema, _) = Self::type_expr_schema(&args[0]);
                (
                    json!({
                        "type": "array",
                        "items": items_schema
                    }),
                    false,
                )
            }
            "Result" if args.len() == 2 => {
                let (ok_schema, _) = Self::type_expr_schema(&args[0]);
                let (err_schema, _) = Self::type_expr_schema(&args[1]);
                (json!({ "oneOf": [ok_schema, err_schema] }), false)
            }
            // Unknown generic: emit empty schema with description
            _ => (
                json!({
                    "description": format!("Unresolved generic type: {name}")
                }),
                false,
            ),
        }
    }
}
