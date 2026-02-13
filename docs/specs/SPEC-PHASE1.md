# Phase 1 Spec: Make It Real

> Status: COMPLETE
> Completed on: 2026-02-13
> Implementation commit: `35e41c5`

## Prerequisites

Before starting Phase 1 implementation:

1. **v0.1 POC is complete** - All Phase 1 work builds on the existing parser, AST, and codegen infrastructure.
2. **Rust toolchain** - Requires Rust 1.70+ for stable async trait support.
3. **Test environment** - A sample Rust project that will consume generated `.gu` code.
4. **Understanding of current architecture** - Read `docs/ARCHITECTURE.md` and inspect existing codegen output.

## Current State

### File Structure
```
D:\Projects\gust\
├── gust-lang\
│   ├── src\
│   │   ├── grammar.pest      # PEG grammar (86 lines)
│   │   ├── ast.rs            # AST types (148 lines)
│   │   ├── parser.rs         # Parser implementation (434 lines)
│   │   ├── codegen.rs        # Rust codegen (555 lines)
│   │   ├── codegen_go.rs     # Go codegen (669 lines)
│   │   └── lib.rs            # Public API (9 lines)
│   └── Cargo.toml
├── gust-runtime\
│   ├── src\
│   │   └── lib.rs            # Runtime traits (77 lines)
│   └── Cargo.toml
├── gust-cli\
│   ├── src\
│   │   └── main.rs           # CLI (156 lines)
│   └── Cargo.toml
├── examples\
│   ├── order_processor.gu    # Example input (79 lines)
│   ├── order_processor.g.rs  # Rust output (209 lines)
│   └── order_processor.g.go  # Go output (329 lines)
└── Cargo.toml                # Workspace config
```

### Current AST Types (ast.rs)

```rust
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub machines: Vec<MachineDecl>,
}

pub struct UsePath {
    pub segments: Vec<String>,  // e.g., ["crate", "models", "Order"]
}

pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<Field>,
}

pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
}

pub enum TypeExpr {
    Simple(String),                    // e.g., "String", "i64"
    Generic(String, Vec<TypeExpr>),    // e.g., "Vec<String>", "Option<Money>"
}

pub struct MachineDecl {
    pub name: String,
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

pub struct StateDecl {
    pub name: String,
    pub fields: Vec<Field>,
}

pub struct TransitionDecl {
    pub name: String,
    pub from: String,
    pub targets: Vec<String>,
}

pub struct EffectDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
    // Note: is_async field will be added in Feature 4
}

pub struct OnHandler {
    pub transition_name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    // Note: is_async field will be added in Feature 4
}

pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

pub struct Block {
    pub statements: Vec<Statement>,
}

pub enum Statement {
    Let { name: String, ty: Option<TypeExpr>, value: Expr },
    Return(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    Goto { state: String, args: Vec<Expr> },
    Perform { effect: String, args: Vec<Expr> },
    Expr(Expr),
}

pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),
    Ident(String),
    FieldAccess(Box<Expr>, String),
    FnCall(String, Vec<Expr>),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Perform(String, Vec<Expr>),  // effect name, arguments
}
```

### Current Grammar Rules (grammar.pest)

Key rules relevant to Phase 1 (verified against actual grammar.pest):

```pest
program = { SOI ~ (use_decl | type_decl | machine_decl)* ~ EOI }

use_decl = { "use" ~ path ~ ";" }
path     = { ident ~ ("::" ~ ident)* }

type_decl   = { "type" ~ ident ~ "{" ~ field_list ~ "}" }
field_list  = { (field ~ ("," ~ field)* ~ ","?)? }
field       = { ident ~ ":" ~ type_expr }

type_expr    = { generic_type | simple_type }
generic_type = { ident ~ "<" ~ type_expr ~ ("," ~ type_expr)* ~ ">" }
simple_type  = { ident }

machine_decl = { "machine" ~ ident ~ ("{" ~ machine_body ~ "}") }
machine_body = { machine_item* }
machine_item = { state_decl | transition_decl | on_handler | effect_decl }

state_decl      = { "state" ~ ident ~ ("(" ~ field_list ~ ")")? }
transition_decl = { "transition" ~ ident ~ ":" ~ ident ~ "->" ~ target_states }
target_states   = { ident ~ ("|" ~ ident)* }
effect_decl     = { "effect" ~ ident ~ "(" ~ field_list ~ ")" ~ "->" ~ type_expr }
on_handler      = { "on" ~ ident ~ "(" ~ param_list ~ ")" ~ ("->" ~ type_expr)? ~ block }
param_list      = { (param ~ ("," ~ param)* ~ ","?)? }
param           = { ident ~ ":" ~ type_expr }
```

### Current Codegen Patterns (codegen.rs)

**Type Declaration Output:**
```rust
// Input: type Order { id: String, customer: String }
// Output:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub customer: String,
}
```

**State Enum Output:**
```rust
// Input: state Pending(order: Order)
// Output:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderProcessorState {
    Pending { order: Order },
    Validated { order: Order, total: Money },
    // ...
}
```

**Effect Trait Output:**
```rust
// Input: effect calculate_total(order: Order) -> Money
// Output:
pub trait OrderProcessorEffects {
    fn calculate_total(&self, order: &Order) -> Money;
    fn process_payment(&self, total: &Money) -> Receipt;
}
```

**Transition Method Output:**
```rust
// Input: transition validate: Pending -> Validated | Failed
//        on validate(ctx: ValidationCtx) { ... }
// Output:
pub fn validate(&mut self, ctx: ValidationCtx, effects: &impl OrderProcessorEffects)
    -> Result<(), OrderProcessorError>
{
    match &self.state {
        OrderProcessorState::Pending { order } => {
            // handler body here
            Ok(())
        }
        _ => Err(OrderProcessorError::InvalidTransition {
            transition: "validate".to_string(),
            from: format!("{:?}", self.state),
        }),
    }
}
```

**Note**: The `effects` parameter is only included when the handler body uses `perform`. The existing `handler_uses_perform()` function in codegen.rs walks the handler AST to determine this. This behavior must be preserved in Phase 1. Handlers that don't use `perform` will not have an `effects` parameter in their generated method signature.

---

## Feature 1: Cargo build.rs Integration

### Requirements

**R1.1**: Create `gust-build` crate that provides `compile_gust_files()` function for use in `build.rs` scripts.

**R1.2**: The function must:
- Discover all `.gu` files in the project (configurable source directory)
- Parse each `.gu` file
- Generate corresponding `.g.rs` file alongside the `.gu` source (or in configured output directory)
- Only regenerate if `.gu` file is newer than `.g.rs` (incremental builds)
- Return compilation errors with file path, line number, and column number

**R1.3**: Support configuration via builder pattern:
```rust
GustBuilder::new()
    .source_dir("src/machines")
    .output_dir("src/generated")
    .target(Target::Rust)
    .compile()?;
```

**R1.4**: Emit `cargo:rerun-if-changed=<file.gu>` directives so Cargo knows when to re-run the build script.

**R1.5**: Provide helpful error messages when compilation fails (show source snippet with error location).

### Acceptance Criteria

**AC1.1**: A Rust project with a `build.rs` that calls `gust_build::compile_gust_files()` successfully generates `.g.rs` files during `cargo build`.

**AC1.2**: Running `cargo build` twice without changes does not regenerate `.g.rs` files (incremental build works).

**AC1.3**: Modifying a `.gu` file causes only that file to be recompiled on next `cargo build`.

**AC1.4**: Syntax errors in `.gu` files are reported with file:line:column format and fail the build.

**AC1.5**: Generated `.g.rs` files compile without warnings (when effect traits are implemented).

### Test Cases

**TC1.1 - Basic Integration**

File: `test_project/build.rs`
```rust
fn main() {
    gust_build::compile_gust_files().unwrap();
}
```

File: `test_project/src/counter.gu`
```gust
machine Counter {
    state Idle(count: i64)
    state Running(count: i64)

    transition start: Idle -> Running
    transition stop: Running -> Idle

    on start(ctx: Context) {
        goto Running(0);
    }

    on stop(ctx: Context) {
        goto Idle(count);
    }
}
```

Expected: `cargo build` generates `test_project/src/counter.g.rs` and compiles successfully.

**TC1.2 - Incremental Build**

Given: TC1.1 setup with successful build
When: Run `cargo build` again without changes
Then: Build completes instantly, `.g.rs` file mtime unchanged

**TC1.3 - Error Reporting**

File: `test_project/src/broken.gu`
```gust
machine Broken {
    state Start
    transition go: Start -> End  // End state not declared
}
```

Expected error output:
```
error: undeclared target state 'End'
  --> src/broken.gu:3:28
   |
3  |     transition go: Start -> End
   |                             ^^^ not found in machine states
```

**TC1.4 - Custom Directories**

File: `test_project/build.rs`
```rust
fn main() {
    gust_build::GustBuilder::new()
        .source_dir("gust_src")
        .output_dir("src/generated")
        .compile()
        .unwrap();
}
```

File: `test_project/gust_src/app.gu`
```gust
machine App {
    state Init
}
```

Expected: Generates `test_project/src/generated/app.g.rs`

### Implementation Guide

**Step 1: Create gust-build crate**

File: `D:\Projects\gust\gust-build\Cargo.toml`
```toml
[package]
name = "gust-build"
version = "0.1.0"
edition = "2021"
description = "Build script support for Gust - compile .gu files in build.rs"

[dependencies]
gust-lang = { path = "../gust-lang" }
walkdir = "2"
```

File: `D:\Projects\gust\gust-build\src\lib.rs`
```rust
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use gust_lang::{parse_program, RustCodegen, GoCodegen};

pub struct GustBuilder {
    source_dir: PathBuf,
    output_dir: Option<PathBuf>,
    target: Target,
}

pub enum Target {
    Rust,
    Go { package_name: String },
}

impl GustBuilder {
    pub fn new() -> Self {
        Self {
            source_dir: PathBuf::from("src"),
            output_dir: None,
            target: Target::Rust,
        }
    }

    pub fn source_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.source_dir = dir.into();
        self
    }

    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    pub fn target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    pub fn compile(self) -> Result<(), String> {
        let gu_files = find_gu_files(&self.source_dir)?;

        for gu_path in gu_files {
            // Emit rerun-if-changed directive
            println!("cargo:rerun-if-changed={}", gu_path.display());

            // Determine output path
            let output_path = self.compute_output_path(&gu_path)?;

            // Check if regeneration is needed
            if !needs_regeneration(&gu_path, &output_path)? {
                continue;
            }

            // Read and parse
            let source = fs::read_to_string(&gu_path)
                .map_err(|e| format!("Failed to read {}: {}", gu_path.display(), e))?;

            let program = parse_program(&source)
                .map_err(|e| format_parse_error(&gu_path, &source, &e))?;

            // Generate code
            let generated = match &self.target {
                Target::Rust => RustCodegen::new().generate(&program),
                Target::Go { package_name } => GoCodegen::new().generate(&program, package_name),
            };

            // Create output directory if needed
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create output dir: {}", e))?;
            }

            // Write output
            fs::write(&output_path, generated)
                .map_err(|e| format!("Failed to write {}: {}", output_path.display(), e))?;
        }

        Ok(())
    }

    fn compute_output_path(&self, gu_path: &Path) -> Result<PathBuf, String> {
        let stem = gu_path.file_stem()
            .ok_or("Invalid filename")?
            .to_string_lossy();

        let extension = match &self.target {
            Target::Rust => "g.rs",
            Target::Go { .. } => "g.go",
        };

        if let Some(ref output_dir) = self.output_dir {
            Ok(output_dir.join(format!("{}.{}", stem, extension)))
        } else {
            // Place alongside source
            let parent = gu_path.parent().ok_or("No parent directory")?;
            Ok(parent.join(format!("{}.{}", stem, extension)))
        }
    }
}

fn find_gu_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("gu") {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

fn needs_regeneration(gu_path: &Path, output_path: &Path) -> Result<bool, String> {
    if !output_path.exists() {
        return Ok(true);
    }

    let gu_meta = fs::metadata(gu_path)
        .map_err(|e| format!("Failed to stat {}: {}", gu_path.display(), e))?;
    let out_meta = fs::metadata(output_path)
        .map_err(|e| format!("Failed to stat {}: {}", output_path.display(), e))?;

    Ok(gu_meta.modified().unwrap() > out_meta.modified().unwrap())
}

fn format_parse_error(path: &Path, source: &str, error: &str) -> String {
    // pest errors include line/col info in the format:
    //   " --> N:M\n  |\nN | line content\n  | ^^^"
    // We prepend the file path to each location marker
    let mut result = String::new();
    for line in error.lines() {
        if line.starts_with(" --> ") {
            // Replace position with file:line:col
            let pos = line.trim_start_matches(" --> ");
            result.push_str(&format!("  --> {}:{}\n", path.display(), pos));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

pub fn compile_gust_files() -> Result<(), String> {
    GustBuilder::new().compile()
}
```

**Step 2: Update workspace Cargo.toml**

Add `gust-build` to workspace members:
```toml
[workspace]
members = [
    "gust-lang",
    "gust-runtime",
    "gust-cli",
    "gust-build",
]
```

**Step 3: Add validation to codegen**

Modify `D:\Projects\gust\gust-lang\src\codegen.rs` to add a `validate()` method that checks for:
- Undeclared target states in transitions
- Undeclared effects in `perform` statements
- Type mismatches in goto arguments

---

## Feature 2: Watch Mode

### Requirements

**R2.1**: Add `gust watch` command that monitors `.gu` files for changes and automatically regenerates `.g.rs` files.

**R2.2**: Watch mode must:
- Discover all `.gu` files in the source directory (default: `src/`)
- Monitor file system for changes using `notify` crate
- Regenerate on save (debounced by 100ms to handle editor multi-write)
- Show compilation status (success/failure) after each regeneration
- Continue running on errors (don't exit watch loop)

**R2.3**: Support `--dir` flag to specify watch directory.

**R2.4**: Support `--target` flag to specify output target (rust/go).

**R2.5**: Clear output and show timestamp on each regeneration for good UX.

### Acceptance Criteria

**AC2.1**: Running `gust watch` detects `.gu` file modifications and regenerates within 200ms.

**AC2.2**: Syntax errors are displayed but watch mode continues running.

**AC2.3**: Creating a new `.gu` file is detected and compiled.

**AC2.4**: Deleting a `.gu` file is detected and corresponding `.g.rs` is removed.

**AC2.5**: Ctrl+C cleanly exits watch mode.

### Test Cases

**TC2.1 - Basic Watch**

Setup:
```
test_project/
  src/
    app.gu
```

Run: `gust watch`
Action: Edit `src/app.gu` and save
Expected: Console shows "✓ Regenerated app.g.rs" within 200ms

**TC2.2 - Error Handling**

Setup: Same as TC2.1
Action: Introduce syntax error in `app.gu` and save
Expected: Error displayed, watch continues running
Action: Fix error and save
Expected: "✓ Regenerated app.g.rs"

**TC2.3 - New File Detection**

Setup: Watch running on `src/`
Action: Create `src/new_machine.gu`
Expected: "✓ Generated new_machine.g.rs"

**TC2.4 - File Deletion**

Setup: Watch running, `src/old.gu` and `src/old.g.rs` exist
Action: Delete `src/old.gu`
Expected: `src/old.g.rs` is removed, console shows "✓ Removed old.g.rs"

### Implementation Guide

**Step 1: Add notify dependency**

File: `D:\Projects\gust\gust-cli\Cargo.toml`
```toml
[dependencies]
gust-lang = { path = "../gust-lang" }
clap = { version = "4", features = ["derive"] }
notify = "6"
notify-debouncer-mini = "0.4"
```

**Step 2: Add watch command to CLI**

File: `D:\Projects\gust\gust-cli\src\main.rs`

Add to `Commands` enum:
```rust
/// Watch .gu files and regenerate on changes
Watch {
    /// Directory to watch (default: src)
    #[arg(short, long, default_value = "src")]
    dir: PathBuf,

    /// Target language (rust or go)
    #[arg(short, long, default_value = "rust")]
    target: String,

    /// Package name for Go output
    #[arg(short, long)]
    package: Option<String>,
},
```

Add watch implementation:
```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::time::Duration;
use std::sync::mpsc::channel;

fn watch_files(dir: PathBuf, target: String, package: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = channel();

    let mut debouncer = new_debouncer(Duration::from_millis(100), None, tx)?;
    debouncer.watcher().watch(&dir, RecursiveMode::Recursive)?;

    println!("👀 Watching {} for changes (Ctrl+C to stop)", dir.display());

    // Initial compilation of all files
    compile_all_gu_files(&dir, &target, &package)?;

    for events in rx {
        match events {
            Ok(events) => {
                for event in events {
                    if let Some(path) = event.path.to_str() {
                        if path.ends_with(".gu") {
                            match event.kind {
                                DebouncedEventKind::Any => {
                                    compile_single_file(&Path::new(path), &target, &package);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("Watch error: {:?}", e),
        }
    }

    Ok(())
}

fn compile_all_gu_files(dir: &Path, target: &str, package: &Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("gu") {
            compile_single_file(path, target, package);
        }
    }
    Ok(())
}

fn compile_single_file(input: &Path, target: &str, package: &Option<String>) {
    print!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("🔨 Compiling {} ...", input.display());

    let source = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Error reading file: {}", e);
            return;
        }
    };

    let program = match parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Parse error:\n{}", e);
            return;
        }
    };

    let stem = input.file_stem().unwrap().to_string_lossy();
    let parent = input.parent().unwrap_or_else(|| Path::new("."));

    match target {
        "rust" => {
            let code = RustCodegen::new().generate(&program);
            let out_path = parent.join(format!("{}.g.rs", stem));

            if let Err(e) = fs::write(&out_path, &code) {
                eprintln!("❌ Write error: {}", e);
                return;
            }

            println!("✓ Regenerated {}", out_path.display());
        }
        "go" => {
            let pkg = package.clone().unwrap_or_else(|| stem.to_string());
            let code = GoCodegen::new().generate(&program, &pkg);
            let out_path = parent.join(format!("{}.g.go", stem));

            if let Err(e) = fs::write(&out_path, &code) {
                eprintln!("❌ Write error: {}", e);
                return;
            }

            println!("✓ Regenerated {}", out_path.display());
        }
        _ => eprintln!("❌ Unknown target: {}", target),
    }
}
```

---

## Feature 3: Import Resolution

### Requirements

**R3.1**: `use` declarations in `.gu` files must resolve to Rust module paths and be emitted in generated `.g.rs` files.

**R3.2**: Support Rust path syntax:
- Absolute: `use std::collections::HashMap;`
- Crate-relative: `use crate::models::Order;`
- External crates: `use serde::Serialize;`

**R3.3**: Type names referenced in `.gu` files that were imported via `use` declarations should pass through unchanged to generated Rust code (no mapping needed).

**R3.4**: The parser already supports `use` declarations and stores them in `Program.uses`. Codegen must emit them at the top of the generated file.

**R3.5**: Support glob imports: `use crate::models::*;`

### Acceptance Criteria

**AC3.1**: A `.gu` file with `use crate::models::Order;` generates Rust code with that exact import.

**AC3.2**: Types imported via `use` are correctly referenced in generated structs and function signatures.

**AC3.3**: Generated code compiles when the imported modules exist in the host crate.

**AC3.4**: Import order matches declaration order in `.gu` file.

### Test Cases

**TC3.1 - Crate-Relative Import**

File: `src/payment.gu`
```gust
use crate::models::Money;
use crate::models::Receipt;

machine PaymentProcessor {
    state Pending(amount: Money)
    state Completed(receipt: Receipt)

    transition process: Pending -> Completed
}
```

Expected output in `src/payment.g.rs`:
```rust
// Generated by Gust compiler — do not edit manually
use serde::{Serialize, Deserialize};
use gust_runtime::prelude::*;
use crate::models::Money;
use crate::models::Receipt;

// ... rest of generated code
```

**TC3.2 - External Crate Import**

File: `src/app.gu`
```gust
use std::collections::HashMap;

type Config {
    settings: HashMap<String, String>,
}
```

Expected: Generated code includes `use std::collections::HashMap;`

**TC3.3 - Glob Import**

File: `src/processor.gu`
```gust
use crate::types::*;

machine Processor {
    state Ready(config: AppConfig)
}
```

Expected: Generated code includes `use crate::types::*;`

### Implementation Guide

**Step 1: Update codegen prelude emission**

Modify `D:\Projects\gust\gust-lang\src\codegen.rs`:

Change `emit_prelude()` method:
```rust
fn emit_prelude(&mut self, program: &Program) {
    self.line("// Generated by Gust compiler — do not edit manually");
    self.line("use serde::{Serialize, Deserialize};");
    self.line("use gust_runtime::prelude::*;");

    // Emit user-provided imports
    for use_path in &program.uses {
        let path_str = use_path.segments.join("::");
        self.line(&format!("use {};", path_str));
    }

    self.newline();
}
```

Update `generate()` method signature:
```rust
pub fn generate(mut self, program: &Program) -> String {
    self.emit_prelude(program);  // Pass program to access uses

    // ... rest unchanged
}
```

**Step 2: Update Go codegen similarly**

Modify `D:\Projects\gust\gust-lang\src\codegen_go.rs`:

Update `emit_prelude()`:
```rust
fn emit_prelude(&mut self, package_name: &str, program: &Program) {
    self.line("// Code generated by Gust compiler — DO NOT EDIT.");
    self.newline();
    self.line(&format!("package {package_name}"));
    self.newline();

    // Standard imports
    self.line("import (");
    self.indent += 1;
    self.line("\"encoding/json\"");
    self.line("\"fmt\"");

    // User-provided imports
    // Rust-style imports (crate::, ::) get commented out
    // Go-style imports (containing /) are emitted properly
    for use_path in &program.uses {
        let path_str = use_path.segments.join("::");
        if path_str.contains('/') {
            // Looks like a Go import path
            self.line(&format!("\"{}\"", path_str.replace("::", "/")));
        } else {
            // Rust-style import, comment it out
            self.line(&format!("// Gust import: use {}", path_str));
        }
    }

    self.indent -= 1;
    self.line(")");
    self.newline();

    // Suppress unused import warnings
    self.line("var _ = json.Marshal");
    self.line("var _ = fmt.Errorf");
    self.newline();
}
```

### Go Codegen

Go imports differ fundamentally from Rust:
- Rust-style imports (`use crate::models::Order`) are commented out in Go output
- Go-style imports (paths containing `/`, like `github.com/user/package`) are emitted as proper Go imports
- The distinction is made by checking if the path contains `/`

**Step 3: Update codegen for Go async changes**

In `codegen_go.rs`, modify `emit_transition_method()`:

```rust
fn emit_transition_method(
    &mut self,
    machine_name: &str,
    transition: &TransitionDecl,
    handlers: &[OnHandler],
    states: &[StateDecl],
    effects: &[EffectDecl],
) {
    let method_name = pascal_case(&transition.name);
    let handler = handlers.iter().find(|h| h.transition_name == transition.name);

    // Build parameter list
    let mut params = Vec::new();

    // Add context.Context for async handlers
    let is_async = handler.map(|h| h.is_async).unwrap_or(false);
    if is_async {
        params.push("ctx context.Context".to_string());
    }

    // Add handler params
    if let Some(h) = handler {
        for p in &h.params {
            params.push(format!("{} {}", p.name, self.type_expr_to_go(&p.ty)));
        }
    }

    // Add effects param if this handler uses perform
    let uses_effects = handler
        .map(|h| handler_uses_perform(&h.body))
        .unwrap_or(false);
    if uses_effects && !effects.is_empty() {
        params.push(format!("effects {machine_name}Effects"));
    }

    self.line(&format!(
        "func (m *{machine_name}) {method_name}({}) error {{",
        params.join(", ")
    ));
    self.indent += 1;

    // ... rest of method body unchanged
}
```

Also update `emit_prelude()` to add `"context"` import when async handlers exist:

```rust
fn emit_prelude(&mut self, package_name: &str, program: &Program) {
    // ... package declaration ...

    // Determine if context is needed
    let has_async = program.machines.iter().any(|m| {
        m.handlers.iter().any(|h| h.is_async)
    });

    self.line("import (");
    self.indent += 1;
    if has_async {
        self.line("\"context\"");
    }
    self.line("\"encoding/json\"");
    self.line("\"fmt\"");
    // ... user imports ...
    self.indent -= 1;
    self.line(")");
}
```

**Step 4: Add tests**

File: `D:\Projects\gust\gust-lang\tests\import_resolution.rs`
```rust
use gust_lang::{parse_program, RustCodegen};

#[test]
fn test_crate_relative_imports() {
    let source = r#"
        use crate::models::Order;
        use crate::models::Money;

        machine Test {
            state Start
        }
    "#;

    let program = parse_program(source).unwrap();
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("use crate::models::Order;"));
    assert!(generated.contains("use crate::models::Money;"));
}

#[test]
fn test_std_imports() {
    let source = r#"
        use std::collections::HashMap;

        type Config {
            data: HashMap<String, String>,
        }
    "#;

    let program = parse_program(source).unwrap();
    let generated = RustCodegen::new().generate(&program);

    assert!(generated.contains("use std::collections::HashMap;"));
    assert!(generated.contains("pub data: HashMap<String, String>"));
}
```

---

## Feature 4: Async Support

### Requirements

**R4.1**: Add `async` keyword to grammar for marking transition handlers as async.

**R4.2**: Async handlers generate `async fn` transition methods in Rust.

**R4.3**: Effect traits with async methods generate `async fn` in the trait definition.

**R4.4**: Generated async code requires `tokio` runtime (already in `gust-runtime` dependencies).

**R4.5**: Support `.await` syntax in handler bodies for calling async effects.

**R4.6**: The `perform` expression becomes `.await` when used in async context.

### Grammar Changes

Add to `grammar.pest`:

```pest
async_modifier = { "async" }
on_handler = { async_modifier? ~ "on" ~ ident ~ "(" ~ param_list ~ ")" ~ ("->" ~ type_expr)? ~ block }
effect_decl = { async_modifier? ~ "effect" ~ ident ~ "(" ~ field_list ~ ")" ~ "->" ~ type_expr }
```

**IMPORTANT**: The `async_modifier` rule is required because pest's literal strings like `"async"` are anonymous tokens that don't appear as named pairs in `.into_inner()`. Without a named rule, the parser would skip past "async" and "on" to the first named rule (ident), making async detection impossible.

### AST Changes

Modify `D:\Projects\gust\gust-lang\src\ast.rs`:

```rust
pub struct OnHandler {
    pub transition_name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub is_async: bool,  // NEW
}

pub struct EffectDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
    pub is_async: bool,  // NEW
}
```

### Acceptance Criteria

**AC4.1**: A handler marked `async on transition(...) { ... }` generates an async transition method.

**AC4.2**: Effects marked `async effect name(...) -> T` generate async trait methods.

**AC4.3**: `perform effect(args)` in async handler generates `effects.effect(args).await`.

**AC4.4**: Generated async code compiles with `#[tokio::test]` test harness.

**AC4.5**: Mixing sync and async handlers in the same machine is supported.

### Test Cases

**TC4.1 - Async Handler**

Input:
```gust
use crate::models::{Order, Money, Receipt};

machine AsyncProcessor {
    state Pending(order: Order)
    state Processed(receipt: Receipt)

    async effect calculate_total(order: Order) -> Money
    async effect charge_card(amount: Money) -> Receipt

    transition process: Pending -> Processed

    async on process(ctx: Context) {
        let total = perform calculate_total(order);
        let receipt = perform charge_card(total);
        goto Processed(receipt);
    }
}
```

Expected Rust output:
```rust
pub trait AsyncProcessorEffects {
    async fn calculate_total(&self, order: &Order) -> Money;
    async fn charge_card(&self, amount: &Money) -> Receipt;
}

impl AsyncProcessor {
    pub async fn process(&mut self, ctx: Context, effects: &impl AsyncProcessorEffects)
        -> Result<(), AsyncProcessorError>
    {
        match &self.state {
            AsyncProcessorState::Pending { order } => {
                let total = effects.calculate_total(order).await;
                let receipt = effects.charge_card(&total).await;
                self.state = AsyncProcessorState::Processed { receipt };
                Ok(())
            }
            _ => Err(AsyncProcessorError::InvalidTransition {
                transition: "process".to_string(),
                from: format!("{:?}", self.state),
            }),
        }
    }
}
```

**TC4.2 - Mixed Sync and Async**

Input:
```gust
machine Mixed {
    state Start
    state Middle
    state End

    effect sync_effect() -> i64
    async effect async_effect() -> String

    transition sync_step: Start -> Middle
    transition async_step: Middle -> End

    on sync_step(ctx: Context) {
        let x = perform sync_effect();
        goto Middle();
    }

    async on async_step(ctx: Context) {
        let y = perform async_effect();
        goto End();
    }
}
```

Expected:
- `sync_step()` is a normal `fn`
- `async_step()` is an `async fn`
- Trait has both sync and async methods

### Go Codegen

Go does NOT have async/await. The mapping for async features is:

**Async handlers:**
- `async on handler(params)` → generates normal Go method (Go is inherently concurrent via goroutines)
- Add `context.Context` as the first parameter to async handlers for cancellation support
- Caller is responsible for wrapping calls in goroutines if concurrent execution is needed

**Async effects:**
- `async effect name(params) -> T` → generates synchronous interface method
- The effect implementation can use goroutines internally if needed
- No `.await` equivalent in Go

**Perform statements:**
- `perform effect(args)` in sync handler → `effects.Effect(args)` (no change)
- `perform effect(args)` in async handler → still `effects.Effect(args)` (no `.await` in Go)

**Imports:**
- Add `"context"` to Go imports when any handler is async

**Example Go output for TC4.1:**

```go
type AsyncProcessorEffects interface {
    CalculateTotal(order Order) Money
    ChargeCard(amount Money) Receipt
}

func (m *AsyncProcessor) Process(ctx context.Context, ctxParam Context, effects AsyncProcessorEffects) error {
    if m.State != AsyncProcessorStatePending {
        return &AsyncProcessorError{Transition: "process", From: m.State.String()}
    }

    total := effects.CalculateTotal(*m.PendingData.Order)
    receipt := effects.ChargeCard(total)
    m.State = AsyncProcessorStateProcessed
    // ... set ProcessedData

    return nil
}
```

Note: Go runtime handles concurrency via goroutines. The `context.Context` parameter allows cancellation/timeout control.

### Implementation Guide

**Step 1: Update parser**

Modify `D:\Projects\gust\gust-lang\src\parser.rs`:

In `parse_on_handler()`:
```rust
fn parse_on_handler(pair: Pair<Rule>) -> OnHandler {
    let mut inner = pair.into_inner();

    // Check for async_modifier (named rule, so it appears in pairs)
    let first = inner.peek().unwrap();
    let is_async = if first.as_rule() == Rule::async_modifier {
        inner.next(); // consume the async_modifier
        true
    } else {
        false
    };

    let transition_name = inner.next().unwrap().as_str().to_string(); // ident
    let params = parse_param_list(inner.next().unwrap());

    // Check if next is a return type or a block
    let next = inner.next().unwrap();
    let (return_type, body) = match next.as_rule() {
        Rule::type_expr => {
            let rt = Some(parse_type_expr(next));
            let b = parse_block(inner.next().unwrap());
            (rt, b)
        }
        Rule::block => (None, parse_block(next)),
        _ => unreachable!(),
    };

    OnHandler {
        transition_name,
        params,
        return_type,
        body,
        is_async,
    }
}
```

In `parse_effect_decl()`:
```rust
fn parse_effect_decl(pair: Pair<Rule>) -> EffectDecl {
    let mut inner = pair.into_inner();

    // Check for async_modifier (named rule, so it appears in pairs)
    let first = inner.peek().unwrap();
    let is_async = if first.as_rule() == Rule::async_modifier {
        inner.next(); // consume the async_modifier
        true
    } else {
        false
    };

    let name = inner.next().unwrap().as_str().to_string(); // ident
    let params = parse_field_list(inner.next().unwrap());
    let return_type = parse_type_expr(inner.next().unwrap());

    EffectDecl {
        name,
        params,
        return_type,
        is_async,
    }
}
```

**Step 2: Update codegen**

Modify `D:\Projects\gust\gust-lang\src\codegen.rs`:

In `emit_effect_trait()`:
```rust
fn emit_effect_trait(&mut self, machine: &MachineDecl) {
    let trait_name = format!("{}Effects", machine.name);
    self.line(&format!("pub trait {trait_name} {{"));
    self.indent += 1;

    for effect in &machine.effects {
        let async_kw = if effect.is_async { "async " } else { "" };
        let params: Vec<String> = effect
            .params
            .iter()
            .map(|p| format!("{}: &{}", p.name, self.type_expr_to_rust(&p.ty)))
            .collect();
        let return_type = self.type_expr_to_rust(&effect.return_type);
        let all_params = if params.is_empty() {
            "&self".to_string()
        } else {
            format!("&self, {}", params.join(", "))
        };
        self.line(&format!(
            "{}fn {}({}) -> {};",
            async_kw,
            effect.name,
            all_params,
            return_type
        ));
    }

    self.indent -= 1;
    self.line("}");
}
```

In `emit_transition_method()`:
```rust
fn emit_transition_method(
    &mut self,
    machine_name: &str,
    state_enum: &str,
    transition: &TransitionDecl,
    handlers: &[OnHandler],
    states: &[StateDecl],
    effects: &[EffectDecl],
) {
    let error_type = format!("{machine_name}Error");

    let handler = handlers
        .iter()
        .find(|h| h.transition_name == transition.name);

    let is_async = handler.map(|h| h.is_async).unwrap_or(false);
    let async_kw = if is_async { "async " } else { "" };

    // ... rest of param building

    self.line(&format!(
        "pub {}fn {}({params_str}) -> Result<(), {error_type}> {{",
        async_kw,
        transition.name
    ));

    // ... rest unchanged
}
```

In `emit_statement()` for `Statement::Perform`:
```rust
Statement::Perform { effect, args } => {
    let arg_strs: Vec<String> = args.iter().map(|a| self.expr_to_rust(a)).collect();

    // Check if this effect is async by looking it up
    let is_effect_async = effects.iter()
        .find(|e| &e.name == effect)
        .map(|e| e.is_async)
        .unwrap_or(false);

    let await_suffix = if is_effect_async { ".await" } else { "" };

    self.line(&format!("effects.{}({}){};", effect, arg_strs.join(", "), await_suffix));
}
```

**Step 3: Pass effect context to codegen**

The `emit_statement()` and `expr_to_rust()` methods need access to the machine's effects list to determine if a perform should be `.await`. Update method signatures to thread the effects context through:

```rust
// Updated signatures:
fn emit_statement(
    &mut self,
    stmt: &Statement,
    state_enum: &str,
    states: &[StateDecl],
    effects: &[EffectDecl],  // NEW parameter
) { /* ... */ }

fn expr_to_rust(&self, expr: &Expr, effects: &[EffectDecl]) -> String {  // NEW parameter
    match expr {
        Expr::Perform(effect_name, args) => {
            let arg_strs: Vec<String> = args
                .iter()
                .map(|a| self.expr_to_rust(a, effects))
                .collect();

            let is_async = effects.iter()
                .find(|e| &e.name == effect_name)
                .map(|e| e.is_async)
                .unwrap_or(false);

            let await_suffix = if is_async { ".await" } else { "" };
            format!("effects.{}({}){}", effect_name, arg_strs.join(", "), await_suffix)
        }
        // All other arms must pass effects through recursive calls
        Expr::BinOp(left, op, right) => {
            format!(
                "({} {} {})",
                self.expr_to_rust(left, effects),
                self.binop_to_rust(op),
                self.expr_to_rust(right, effects)
            )
        }
        Expr::UnaryOp(op, expr) => {
            format!("({}{})", self.unaryop_to_rust(op), self.expr_to_rust(expr, effects))
        }
        Expr::FnCall(name, args) => {
            let arg_strs: Vec<String> = args
                .iter()
                .map(|a| self.expr_to_rust(a, effects))
                .collect();
            format!("{}({})", name, arg_strs.join(", "))
        }
        Expr::FieldAccess(base, field) => {
            format!("{}.{}", self.expr_to_rust(base, effects), field)
        }
        // Non-recursive cases remain unchanged
        Expr::IntLit(v) => format!("{v}"),
        Expr::FloatLit(v) => format!("{v}"),
        Expr::StringLit(s) => format!("\"{s}\".to_string()"),
        Expr::BoolLit(b) => if *b { "true" } else { "false" }.to_string(),
        Expr::Ident(name) => name.clone(),
    }
}
```

**Update all call sites:**

In `emit_statement()`:
```rust
Statement::Let { name, ty, value } => {
    if let Some(type_expr) = ty {
        self.line(&format!(
            "let {}: {} = {};",
            name,
            self.type_expr_to_rust(type_expr),
            self.expr_to_rust(value, effects)  // Pass effects
        ));
    } else {
        self.line(&format!("let {} = {};", name, self.expr_to_rust(value, effects)));  // Pass effects
    }
}
Statement::Return(expr) => {
    self.line(&format!("return {};", self.expr_to_rust(expr, effects)));  // Pass effects
}
Statement::If { condition, then_block, else_block } => {
    self.line(&format!("if {} {{", self.expr_to_rust(condition, effects)));  // Pass effects
    self.indent += 1;
    self.emit_block(then_block, state_enum, states, effects);  // Pass effects
    self.indent -= 1;
    if let Some(else_blk) = else_block {
        self.line("} else {");
        self.indent += 1;
        self.emit_block(else_blk, state_enum, states, effects);  // Pass effects
        self.indent -= 1;
    }
    self.line("}");
}
Statement::Goto { state, args } => {
    // ... existing code, but update expr_to_rust calls:
    let field_inits: Vec<String> = target
        .fields
        .iter()
        .zip(args.iter())
        .map(|(field, arg)| {
            format!("{}: {}", field.name, self.expr_to_rust(arg, effects))  // Pass effects
        })
        .collect();
    // ...
}
Statement::Perform { effect, args } => {
    let arg_strs: Vec<String> = args.iter().map(|a| self.expr_to_rust(a, effects)).collect();  // Pass effects
    let is_async = effects.iter()
        .find(|e| &e.name == effect)
        .map(|e| e.is_async)
        .unwrap_or(false);
    let await_suffix = if is_async { ".await" } else { "" };
    self.line(&format!("effects.{}({}){};", effect, arg_strs.join(", "), await_suffix));
}
Statement::Expr(expr) => {
    self.line(&format!("{};", self.expr_to_rust(expr, effects)));  // Pass effects
}
```

In `emit_block()`:
```rust
fn emit_block(&mut self, block: &Block, state_enum: &str, states: &[StateDecl], effects: &[EffectDecl]) {  // Add effects param
    for stmt in &block.statements {
        self.emit_statement(stmt, state_enum, states, effects);  // Pass effects
    }
}
```

In `emit_transition_method()`:
```rust
fn emit_transition_method(
    &mut self,
    machine_name: &str,
    state_enum: &str,
    transition: &TransitionDecl,
    handlers: &[OnHandler],
    states: &[StateDecl],
    effects: &[EffectDecl],  // Already has effects param
) {
    // ... existing code ...

    // Emit handler body if present
    if let Some(handler) = handler {
        self.emit_block(&handler.body, state_enum, states, effects);  // Pass effects
    }

    // ... rest unchanged ...
}
```

---

## Feature 5: Type System Improvements

### Requirements

**R5.1**: Support enum type declarations (not just struct types).

**R5.2**: Enums map to Rust enums and Go const+iota patterns.

**R5.3**: Support `Option<T>` and `Result<T, E>` in type expressions (already parsed via `Generic` variant).

**R5.4**: Support pattern matching on enums in handler bodies (new statement type).

**R5.5**: Support tuple types: `(String, i64)`.

### Grammar Changes

Add to `grammar.pest`:

```pest
type_decl = { struct_decl | enum_decl }

struct_decl = { "type" ~ ident ~ "{" ~ field_list ~ "}" }

enum_decl = { "enum" ~ ident ~ "{" ~ variant_list ~ "}" }
variant_list = { (variant ~ ("," ~ variant)* ~ ","?)? }
variant = { ident ~ ("(" ~ type_expr ~ ")")? }

// Pattern matching statement
match_stmt = { "match" ~ expr ~ "{" ~ match_arm* ~ "}" }
match_arm = { pattern ~ "=>" ~ (block | expr ~ ",") }
pattern = {
    | ident ~ "(" ~ ident ~ ")"  // Enum::Variant(binding)
    | ident                       // Enum::Variant or identifier
    | "_"                         // Wildcard
}

// Tuple types
type_expr = { tuple_type | generic_type | simple_type }
tuple_type = { "(" ~ type_expr ~ ("," ~ type_expr)+ ~ ")" }
```

### AST Changes

```rust
pub enum TypeDecl {
    Struct {
        name: String,
        fields: Vec<Field>,
    },
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
    },
}

impl TypeDecl {
    pub fn name(&self) -> &str {
        match self {
            TypeDecl::Struct { name, .. } | TypeDecl::Enum { name, .. } => name
        }
    }

    pub fn fields(&self) -> Option<&[Field]> {
        match self {
            TypeDecl::Struct { fields, .. } => Some(fields),
            TypeDecl::Enum { .. } => None,
        }
    }
}

pub struct EnumVariant {
    pub name: String,
    pub payload: Option<TypeExpr>,  // None for unit variants
}

pub enum TypeExpr {
    Simple(String),
    Generic(String, Vec<TypeExpr>),
    Tuple(Vec<TypeExpr>),  // NEW
}

pub enum Statement {
    Let { name: String, ty: Option<TypeExpr>, value: Expr },
    Return(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    Match { scrutinee: Expr, arms: Vec<MatchArm> },  // NEW
    Goto { state: String, args: Vec<Expr> },
    Perform { effect: String, args: Vec<Expr> },
    Expr(Expr),
}

pub struct MatchArm {
    pub pattern: Pattern,
    pub body: MatchBody,
}

pub enum Pattern {
    Variant { enum_name: String, variant: String, binding: Option<String> },
    Ident(String),
    Wildcard,
}

pub enum MatchBody {
    Block(Block),
    Expr(Expr),
}
```

### Acceptance Criteria

**AC5.1**: Enum declarations generate Rust enums with serde derives.

**AC5.2**: Enum variants with payloads map to Rust tuple variants.

**AC5.3**: `Option<T>` and `Result<T, E>` in field types generate correct Rust types.

**AC5.4**: Pattern matching on enums generates Rust match expressions.

**AC5.5**: Tuple types `(A, B, C)` generate Rust tuple types.

### Test Cases

**TC5.1 - Enum Declaration**

Input:
```gust
enum Status {
    Pending,
    Processing(String),
    Complete,
    Failed(String),
}

machine Workflow {
    state Active(status: Status)

    transition start: Active -> Active
}
```

Expected Rust output:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Processing(String),
    Complete,
    Failed(String),
}

// ... machine code
```

**TC5.2 - Option and Result**

Input:
```gust
type Task {
    id: String,
    result: Option<String>,
    error: Result<i64, String>,
}
```

Expected:
```rust
pub struct Task {
    pub id: String,
    pub result: Option<String>,
    pub error: Result<i64, String>,
}
```

**TC5.3 - Pattern Matching**

Input:
```gust
enum PaymentStatus {
    Pending,
    Authorized(String),
    Charged,
}

machine Payment {
    state Processing(status: PaymentStatus)
    state Done

    transition complete: Processing -> Done

    on complete(ctx: Context) {
        match status {
            PaymentStatus::Authorized(token) => {
                // use token
                goto Done();
            }
            PaymentStatus::Charged => {
                goto Done();
            }
            _ => {
                // other cases
            }
        }
    }
}
```

Expected: Generates Rust match with all arms.

**TC5.4 - Tuples**

Input:
```gust
type Coordinate {
    point: (f64, f64),
    metadata: (String, i64, bool),
}
```

Expected:
```rust
pub struct Coordinate {
    pub point: (f64, f64),
    pub metadata: (String, i64, bool),
}
```

### Go Codegen for Type System Features

**Enum declarations:**

Gust enums map to Go `const` with `iota` for unit variants. Variants with payloads require separate handling:

```go
// Gust: enum Status { Pending, Processing(String), Complete, Failed(String) }

// Unit variants as const+iota
type Status int
const (
    StatusPending Status = iota
    StatusComplete
)

// Payload variants as separate structs
type StatusProcessing struct {
    Value string `json:"value"`
}

type StatusFailed struct {
    Value string `json:"value"`
}
```

**Pattern matching:**

Gust `match` expressions map to Go `switch` statements:

```go
// Gust:
// match status {
//     PaymentStatus::Authorized(token) => { /* use token */ }
//     PaymentStatus::Charged => { /* ... */ }
//     _ => { /* ... */ }
// }

// Go:
switch status {
case PaymentStatusAuthorized:
    token := authorizedData.Value  // Access payload struct
    // use token
case PaymentStatusCharged:
    // ...
default:
    // ...
}
```

**Tuple types:**

Go does NOT have tuple types. Map tuples to:
1. Anonymous structs for inline use: `struct { field0 f64; field1 f64 }`
2. Named generated types for reuse: `type Tuple2[A, B] struct { V0 A; V1 B }`

```go
// Gust: type Coordinate { point: (f64, f64) }

// Go option 1 (anonymous struct):
type Coordinate struct {
    Point struct {
        V0 float64 `json:"v0"`
        V1 float64 `json:"v1"`
    } `json:"point"`
}

// Go option 2 (generated Tuple2 type, more idiomatic):
type Tuple2_f64_f64 struct {
    V0 float64 `json:"v0"`
    V1 float64 `json:"v1"`
}

type Coordinate struct {
    Point Tuple2_f64_f64 `json:"point"`
}
```

Recommendation: Use anonymous structs for simplicity in Phase 1. Generated tuple types can be added later if reuse is common.

**Go output for TC5.1:**

```go
type Status int

const (
    StatusPending Status = iota
    StatusComplete
)

type StatusProcessing struct {
    Value string `json:"value"`
}

type StatusFailed struct {
    Value string `json:"value"`
}

type WorkflowState int

const (
    WorkflowStateActive WorkflowState = iota
)

type WorkflowActiveData struct {
    Status Status `json:"status"`
}

// ... rest of machine code
```

**Go output for TC5.3:**

```go
// match status { ... } becomes:

switch m.ProcessingData.Status {
case PaymentStatusAuthorized:
    // Extract token from payload struct (implementation-specific)
    token := getAuthorizedToken(m.ProcessingData.Status)
    m.State = PaymentStateDone
    m.ProcessingData = nil
    m.DoneData = &PaymentDoneData{}
case PaymentStatusCharged:
    m.State = PaymentStateDone
    m.ProcessingData = nil
    m.DoneData = &PaymentDoneData{}
default:
    // other cases
}
```

### Migration Instructions for TypeDecl Breaking Change

The conversion of `TypeDecl` from a struct to an enum is a **BREAKING CHANGE** that requires updating all code that references `TypeDecl` fields.

**Files requiring changes:**

1. **`parser.rs`** - `parse_type_decl()` function:
   ```rust
   // BEFORE:
   fn parse_type_decl(pair: Pair<Rule>) -> TypeDecl {
       let mut inner = pair.into_inner();
       let name = inner.next().unwrap().as_str().to_string();
       let fields = parse_field_list(inner.next().unwrap());
       TypeDecl { name, fields }
   }

   // AFTER:
   fn parse_type_decl(pair: Pair<Rule>) -> TypeDecl {
       let mut inner = pair.into_inner();
       let name = inner.next().unwrap().as_str().to_string();
       let fields = parse_field_list(inner.next().unwrap());
       TypeDecl::Struct { name, fields }
   }
   ```

2. **`codegen.rs`** - `emit_type_decl()` function:
   ```rust
   // BEFORE:
   fn emit_type_decl(&mut self, decl: &TypeDecl) {
       self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
       self.line(&format!("pub struct {} {{", decl.name));
       self.indent += 1;
       for field in &decl.fields {
           // ...
       }
       self.indent -= 1;
       self.line("}");
   }

   // AFTER:
   fn emit_type_decl(&mut self, decl: &TypeDecl) {
       match decl {
           TypeDecl::Struct { name, fields } => {
               self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
               self.line(&format!("pub struct {name} {{"));
               self.indent += 1;
               for field in fields {
                   // ... existing code
               }
               self.indent -= 1;
               self.line("}");
           }
           TypeDecl::Enum { name, variants } => {
               // New enum codegen (added in Feature 5)
               self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
               self.line(&format!("pub enum {name} {{"));
               self.indent += 1;
               for variant in variants {
                   if let Some(ref payload) = variant.payload {
                       self.line(&format!("{}({}),", variant.name, self.type_expr_to_rust(payload)));
                   } else {
                       self.line(&format!("{},", variant.name));
                   }
               }
               self.indent -= 1;
               self.line("}");
           }
       }
   }
   ```

3. **`codegen_go.rs`** - `emit_type_decl()` function:
   ```rust
   // BEFORE:
   fn emit_type_decl(&mut self, decl: &TypeDecl) {
       self.line(&format!("type {} struct {{", decl.name));
       self.indent += 1;
       for field in &decl.fields {
           // ...
       }
       self.indent -= 1;
       self.line("}");
   }

   // AFTER:
   fn emit_type_decl(&mut self, decl: &TypeDecl) {
       match decl {
           TypeDecl::Struct { name, fields } => {
               self.line(&format!("type {name} struct {{"));
               self.indent += 1;
               for field in fields {
                   // ... existing code
               }
               self.indent -= 1;
               self.line("}");
           }
           TypeDecl::Enum { name, variants } => {
               // New Go enum codegen (const+iota pattern, added in Feature 5)
               // See "Go Codegen" section below for details
           }
       }
   }
   ```

**Helper methods** were added to `TypeDecl` to preserve API ergonomics:
- `name()` - Returns the type name for both Struct and Enum variants
- `fields()` - Returns `Some(&[Field])` for Struct, `None` for Enum

Use these helpers where appropriate to avoid verbose pattern matching.

### Implementation Guide

**Step 1: Update AST types**

Apply AST changes listed above to `D:\Projects\gust\gust-lang\src\ast.rs`, including the helper methods.

**Step 2: Update parser**

Add parsing for enum declarations, match statements, and tuple types to `D:\Projects\gust\gust-lang\src\parser.rs`.

**Step 3: Update codegen**

Modify `emit_type_decl()` to handle both struct and enum cases:

```rust
fn emit_type_decl(&mut self, decl: &TypeDecl) {
    match decl {
        TypeDecl::Struct { name, fields } => {
            self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
            self.line(&format!("pub struct {name} {{"));
            self.indent += 1;
            for field in fields {
                self.line(&format!(
                    "pub {}: {},",
                    field.name,
                    self.type_expr_to_rust(&field.ty)
                ));
            }
            self.indent -= 1;
            self.line("}");
        }
        TypeDecl::Enum { name, variants } => {
            self.line("#[derive(Debug, Clone, Serialize, Deserialize)]");
            self.line(&format!("pub enum {name} {{"));
            self.indent += 1;
            for variant in variants {
                if let Some(ref payload) = variant.payload {
                    self.line(&format!("{}({}),", variant.name, self.type_expr_to_rust(payload)));
                } else {
                    self.line(&format!("{},", variant.name));
                }
            }
            self.indent -= 1;
            self.line("}");
        }
    }
}
```

Update `type_expr_to_rust()` to handle tuples:

```rust
fn type_expr_to_rust(&self, ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Simple(name) => map_type_name(name),
        TypeExpr::Generic(name, args) => {
            let mapped = map_type_name(name);
            let arg_strs: Vec<String> = args.iter().map(|a| self.type_expr_to_rust(a)).collect();
            format!("{mapped}<{}>", arg_strs.join(", "))
        }
        TypeExpr::Tuple(types) => {
            let type_strs: Vec<String> = types.iter().map(|t| self.type_expr_to_rust(t)).collect();
            format!("({})", type_strs.join(", "))
        }
    }
}
```

Add match statement codegen to `emit_statement()`:

```rust
Statement::Match { scrutinee, arms } => {
    self.line(&format!("match {} {{", self.expr_to_rust(scrutinee)));
    self.indent += 1;
    for arm in arms {
        let pattern_str = self.pattern_to_rust(&arm.pattern);
        match &arm.body {
            MatchBody::Block(block) => {
                self.line(&format!("{pattern_str} => {{"));
                self.indent += 1;
                self.emit_block(block, state_enum, states);
                self.indent -= 1;
                self.line("}");
            }
            MatchBody::Expr(expr) => {
                self.line(&format!("{pattern_str} => {},", self.expr_to_rust(expr)));
            }
        }
    }
    self.indent -= 1;
    self.line("}");
}
```

Add pattern codegen:

```rust
fn pattern_to_rust(&self, pattern: &Pattern) -> String {
    match pattern {
        Pattern::Variant { enum_name, variant, binding } => {
            if let Some(ref b) = binding {
                format!("{}::{}({})", enum_name, variant, b)
            } else {
                format!("{}::{}", enum_name, variant)
            }
        }
        Pattern::Ident(name) => name.clone(),
        Pattern::Wildcard => "_".to_string(),
    }
}
```

---

## Constraints

### Backward Compatibility

**C1**: All existing `.gu` files from v0.1 POC must continue to compile without modifications.

**C2**: The generated Rust code structure (state enum, machine struct, transition methods) must remain the same for non-async machines.

**C3**: The `.g.rs` file extension and placement convention must not change.

**C4**: The `gust build` command with existing flags must work as before.

### What NOT to Change

**NC1**: Do NOT modify the pest grammar for existing features (states, transitions, effects, handlers, expressions). Only add new rules.

**NC2**: Do NOT change the AST node structure for existing types EXCEPT for `TypeDecl` which must be converted from a struct to an enum to support both struct and enum declarations. All existing code referencing `TypeDecl` fields must be updated to use the new `TypeDecl::Struct` variant. Add helper methods (`name()`, `fields()`) to preserve API ergonomics where possible. This is the only structural change allowed; all other AST types may only have fields added or new enum variants added.

**NC3**: Do NOT alter the runtime library's `Machine` or `Supervisor` traits.

**NC4**: Do NOT change the error enum structure (`{Machine}Error` with `InvalidTransition` and `Failed` variants).

### Performance Requirements

**P1**: Incremental builds with `gust-build` must skip unchanged files (mtime check).

**P2**: Watch mode must debounce file changes to avoid redundant compilations (100ms window).

**P3**: Parsing errors must be reported within 100ms for files under 1000 lines.

**P4**: Generated code must compile with zero warnings when effect traits are implemented.

### Testing Requirements

**T1**: Every new feature must have at least 3 test cases (happy path, error case, edge case).

**T2**: Integration tests must verify that generated code actually compiles with `rustc`.

**T3**: Add regression tests for all v0.1 example files to ensure backward compatibility.

**T4**: Performance tests for watch mode (measure regeneration latency).

**T5**: Error message tests to ensure helpful diagnostics.

---

## Verification Checklist

### Feature 1: Cargo build.rs Integration

- [ ] `gust-build` crate created with correct dependencies
- [ ] `GustBuilder::new().compile()` discovers and compiles all `.gu` files
- [ ] Incremental builds skip unchanged files (verified via mtime)
- [ ] `cargo:rerun-if-changed` directives emitted for all `.gu` files
- [ ] Compilation errors include file:line:column info
- [ ] Custom `source_dir` and `output_dir` work correctly
- [ ] Test project with `build.rs` compiles successfully
- [ ] Running `cargo build` twice without changes is instant

### Feature 2: Watch Mode

- [ ] `gust watch` command added to CLI
- [ ] File changes detected within 200ms
- [ ] Debouncing prevents duplicate compilations (100ms window)
- [ ] Syntax errors displayed but watch continues running
- [ ] New `.gu` files automatically compiled
- [ ] Deleted `.gu` files trigger `.g.rs` removal
- [ ] Ctrl+C cleanly exits watch loop
- [ ] Clear screen and timestamp on each regeneration

### Feature 3: Import Resolution

- [ ] `use` declarations parsed and stored in `Program.uses`
- [ ] Codegen emits imports at top of generated file
- [ ] Crate-relative imports (`use crate::models::Order`) work
- [ ] Standard library imports (`use std::collections::HashMap`) work
- [ ] External crate imports (`use serde::Serialize`) work
- [ ] Glob imports (`use crate::types::*`) work
- [ ] Import order preserved from `.gu` source
- [ ] Generated code with imports compiles successfully

### Feature 4: Async Support

- [ ] `async` keyword added to grammar for handlers and effects
- [ ] `OnHandler.is_async` and `EffectDecl.is_async` fields added to AST
- [ ] Parser correctly detects `async` keyword
- [ ] Async handlers generate `async fn` methods
- [ ] Async effects generate `async fn` in traits
- [ ] `perform effect(args)` in async handler generates `.await`
- [ ] Mixed sync/async handlers in same machine work
- [ ] Generated async code compiles with tokio
- [ ] `#[tokio::test]` test cases pass

### Feature 5: Type System Improvements

- [ ] `enum` declarations added to grammar
- [ ] `TypeDecl::Enum` variant added to AST with `EnumVariant` type
- [ ] Enum codegen produces correct Rust enums
- [ ] Unit variants (no payload) work
- [ ] Tuple variants (with payload) work
- [ ] `Option<T>` and `Result<T, E>` in types generate correctly
- [ ] Tuple types `(A, B, C)` parse and generate correctly
- [ ] `match` statement added to grammar and AST
- [ ] Pattern matching on enums generates Rust match expressions
- [ ] Wildcard patterns work

### Integration & Regression

- [ ] All v0.1 example files still compile unchanged
- [ ] `examples/order_processor.gu` generates identical `.g.rs` (or only improved)
- [ ] End-to-end test: `.gu` → build.rs → cargo build → tests pass
- [ ] Documentation updated with async examples
- [ ] Documentation updated with enum examples
- [ ] ARCHITECTURE.md reflects new features

### Code Quality

- [ ] All new code has inline comments explaining non-obvious logic
- [ ] Error messages are user-friendly (no raw pest errors exposed)
- [ ] Generated code is formatted consistently (4-space indent, clean)
- [ ] No compiler warnings in generated code
- [ ] Clippy passes on all new Rust code
- [ ] Tests cover error cases and edge cases

---

## File Map

### New Files to Create

1. **D:\Projects\gust\gust-build\Cargo.toml** - Build crate manifest (15 lines)
2. **D:\Projects\gust\gust-build\src\lib.rs** - Build script support library (200 lines)
3. **D:\Projects\gust\gust-lang\tests\import_resolution.rs** - Import tests (80 lines)
4. **D:\Projects\gust\gust-lang\tests\async_codegen.rs** - Async tests (150 lines)
5. **D:\Projects\gust\gust-lang\tests\enum_codegen.rs** - Enum tests (120 lines)
6. **D:\Projects\gust\docs\examples\async_payment.gu** - Async example (50 lines)
7. **D:\Projects\gust\docs\examples\enum_status.gu** - Enum example (40 lines)

### Test Dependencies

Integration tests need dev-dependencies. Update `D:\Projects\gust\gust-lang\Cargo.toml`:

```toml
[dev-dependencies]
# For creating temp directories in integration tests (if needed to test generated code compilation)
tempfile = "3"
```

Note: Basic tests that only verify AST parsing and string output don't need additional dependencies. The `tempfile` crate is only needed if tests need to write generated code to disk and compile it with rustc to verify correctness.

### Files to Modify

1. **D:\Projects\gust\Cargo.toml**
   - Add `gust-build` to workspace members

2. **D:\Projects\gust\gust-lang\src\grammar.pest**
   - Add `async_modifier` rule for async keyword detection
   - Update `on_handler` rule to use `async_modifier?` instead of `"async"?`
   - Update `effect_decl` rule to use `async_modifier?` instead of `"async"?`
   - Add `enum_decl` rule for enum type declarations
   - Add `variant_list` and `variant` rules for enum variants
   - Add `match_stmt` rule for pattern matching statements
   - Add `match_arm` and `pattern` rules for match expressions
   - Add `tuple_type` rule to `type_expr` alternatives

3. **D:\Projects\gust\gust-lang\src\ast.rs**
   - Add `is_async: bool` field to `OnHandler` struct
   - Add `is_async: bool` field to `EffectDecl` struct
   - Change `TypeDecl` from struct to enum with `Struct` and `Enum` variants
   - Add `impl TypeDecl` with `name()` and `fields()` helper methods
   - Add `EnumVariant` struct with `name` and `payload` fields
   - Add `Tuple(Vec<TypeExpr>)` variant to `TypeExpr` enum
   - Add `Match { scrutinee, arms }` variant to `Statement` enum
   - Add `MatchArm`, `Pattern`, `MatchBody` types for pattern matching

4. **D:\Projects\gust\gust-lang\src\parser.rs**
   - Update `parse_on_handler()` to check for `Rule::async_modifier` using `peek()` and `as_rule()`
   - Update `parse_effect_decl()` to check for `Rule::async_modifier` using `peek()` and `as_rule()`
   - Update `parse_type_decl()` to return `TypeDecl::Struct { name, fields }`
   - Add `parse_enum_decl()` function returning `TypeDecl::Enum { name, variants }`
   - Add `parse_variant()` function for parsing enum variants
   - Add `parse_match_stmt()` function for pattern matching
   - Add `parse_pattern()` function for match patterns
   - Update `parse_type_expr()` to handle `Rule::tuple_type`

5. **D:\Projects\gust\gust-lang\src\codegen.rs**
   - Update `emit_prelude()` signature to accept `&Program` parameter
   - Add loop in `emit_prelude()` to emit user imports from `program.uses`
   - Update `generate()` method to pass `program` to `emit_prelude()`
   - Update `emit_type_decl()` to match on `TypeDecl::Struct` and `TypeDecl::Enum`
   - Add enum variant codegen in `emit_type_decl()` Enum branch
   - Update `emit_effect_trait()` to prepend `async` keyword when `effect.is_async`
   - Update `emit_transition_method()` to prepend `async` keyword when `handler.is_async`
   - Update `emit_statement()` for `Statement::Perform` to add `.await` suffix when effect is async
   - Update `expr_to_rust()` for `Expr::Perform` to add `.await` suffix when effect is async (requires passing effects context)
   - Add `emit_statement()` branch for `Statement::Match` with pattern codegen
   - Add `pattern_to_rust()` helper function for pattern matching
   - Update `type_expr_to_rust()` to handle `TypeExpr::Tuple` case

6. **D:\Projects\gust\gust-lang\src\codegen_go.rs**
   - Update `emit_prelude()` signature to accept `&Program` parameter
   - Add logic in `emit_prelude()` to emit Go imports (detect `/` in path for Go-style imports)
   - Add `"context"` import detection based on async handlers in program
   - Update `generate()` method to pass `program` to `emit_prelude()`
   - Update `emit_transition_method()` to add `ctx context.Context` parameter for async handlers
   - Add enum codegen using const+iota pattern for unit variants
   - Add separate struct codegen for enum variants with payloads
   - Update match statement codegen to use Go `switch`
   - Add tuple type handling using anonymous structs or generated Tuple types

7. **D:\Projects\gust\gust-cli\src\main.rs**
   - Add `Watch` variant to `Commands` enum with `dir`, `target`, and `package` fields
   - Add `watch_files()` function using notify-debouncer-mini
   - Add `compile_all_gu_files()` helper for initial compilation in watch mode
   - Add `compile_single_file()` helper for per-file compilation

8. **D:\Projects\gust\gust-cli\Cargo.toml**
   - Add `notify = "6"` dependency
   - Add `notify-debouncer-mini = "0.4"` dependency
   - Add `walkdir = "2"` dependency

---

## Implementation Order

For optimal implementation, follow this sequence:

**Week 1:**
1. Feature 3 (Import Resolution) - Simplest, no grammar changes
2. Feature 5 (Type System) - Grammar and AST changes, foundation for others

**Week 2:**
3. Feature 4 (Async Support) - Builds on type system
4. Feature 1 (Build Integration) - Requires stable codegen from above

**Week 3:**
5. Feature 2 (Watch Mode) - Builds on Feature 1
6. Integration testing and documentation
7. Regression testing with v0.1 examples

This order minimizes rework and allows early testing of core features before adding tooling.

---

## Success Metrics

Phase 1 is complete when:

1. ✅ A real Rust project can include `.gu` files that compile automatically via `build.rs`
2. ✅ Developers can use `gust watch` for live reloading during development
3. ✅ Async handlers and effects work with tokio runtime
4. ✅ Enums, Option, Result, and tuples work in type declarations
5. ✅ All v0.1 examples still compile and produce identical output
6. ✅ Documentation includes working examples of all new features
7. ✅ CI pipeline runs full test suite and all tests pass

**Target date**: 3 weeks from start of implementation.

---

END OF SPEC
