# Phase 2 Spec: Developer Experience

## Prerequisites

Before starting Phase 2 implementation:

1. **Phase 1 is COMPLETE** - All Phase 1 features are implemented and tested:
   - Cargo build.rs integration (gust-build crate)
   - Watch mode (gust watch)
   - Import resolution (use declarations)
   - Async support (async handlers, async effects, .await)
   - Type system improvements (enums, Option/Result, tuples, pattern matching)

2. **Understanding of current architecture** - The codebase after Phase 1 includes:
   - Grammar with async, enum, match, tuple support
   - AST with is_async fields, TypeDecl enum, Match statements
   - Codegen emitting async fn, enum types, match expressions
   - gust-build crate for build.rs integration
   - gust watch command with file monitoring

3. **VS Code extension development knowledge** - Basic understanding of:
   - VS Code extension API
   - TextMate grammars (.tmLanguage.json)
   - Language Server Protocol (LSP)
   - tower-lsp crate for Rust LSP servers

## Current State (Post-Phase 1)

### File Structure
```
D:\Projects\gust\
├── gust-lang\
│   ├── src\
│   │   ├── grammar.pest      # Grammar with async, enum, match, tuples
│   │   ├── ast.rs            # AST with async flags, TypeDecl enum, Match
│   │   ├── parser.rs         # Parser with async/enum/match support
│   │   ├── codegen.rs        # Rust codegen with async, enums, imports
│   │   ├── codegen_go.rs     # Go codegen
│   │   └── lib.rs            # Public API
│   └── Cargo.toml
├── gust-runtime\
│   ├── src\
│   │   └── lib.rs            # Runtime traits
│   └── Cargo.toml
├── gust-cli\
│   ├── src\
│   │   └── main.rs           # CLI with build, parse, watch commands
│   └── Cargo.toml
├── gust-build\
│   ├── src\
│   │   └── lib.rs            # Build script integration
│   └── Cargo.toml
├── examples\
│   ├── order_processor.gu
│   ├── order_processor.g.rs
│   └── order_processor.g.go
└── Cargo.toml                # Workspace config
```

### Current Grammar (Relevant Rules)

```pest
program = { SOI ~ (use_decl | type_decl | machine_decl)* ~ EOI }

use_decl = { "use" ~ path ~ ";" }
path     = { ident ~ ("::" ~ ident)* }

type_decl   = { struct_decl | enum_decl }
struct_decl = { "type" ~ ident ~ "{" ~ field_list ~ "}" }
enum_decl   = { "enum" ~ ident ~ "{" ~ variant_list ~ "}" }

machine_decl = { "machine" ~ ident ~ "{" ~ machine_body ~ "}" }
machine_body = { machine_item* }
machine_item = { state_decl | transition_decl | on_handler | effect_decl }

state_decl      = { "state" ~ ident ~ ("(" ~ field_list ~ ")")? }
transition_decl = { "transition" ~ ident ~ ":" ~ ident ~ "->" ~ target_states }
effect_decl     = { "async"? ~ "effect" ~ ident ~ "(" ~ field_list ~ ")" ~ "->" ~ type_expr }
on_handler      = { "async"? ~ "on" ~ ident ~ "(" ~ param_list ~ ")" ~ ("->" ~ type_expr)? ~ block }

statement = { let_stmt | return_stmt | if_stmt | match_stmt | transition_stmt | effect_stmt | expr_stmt }
let_stmt        = { "let" ~ ident ~ (":" ~ type_expr)? ~ "=" ~ expr ~ ";" }
return_stmt     = { "return" ~ expr ~ ";" }
transition_stmt = { "goto" ~ ident ~ ("(" ~ expr_list ~ ")")? ~ ";" }
effect_stmt     = { "perform" ~ ident ~ "(" ~ expr_list ~ ")" ~ ";" }
if_stmt         = { "if" ~ expr ~ block ~ ("else" ~ (if_stmt | block))? }
match_stmt      = { "match" ~ expr ~ "{" ~ match_arm* ~ "}" }

ident = @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
```

### Keywords (For Syntax Highlighting)

From grammar analysis:
- **Declaration keywords**: `use`, `type`, `enum`, `machine`, `state`, `transition`, `effect`, `on`
- **Control flow**: `if`, `else`, `match`, `return`, `let`
- **Actions**: `goto`, `perform`, `async`
- **Operators**: `->`, `|`, `::`
- **Types**: `String`, `i64`, `i32`, `u64`, `u32`, `f64`, `f32`, `bool`, `Option`, `Result`, `Vec`
- **Literals**: `true`, `false`, string/number literals

---

## Feature 1: VS Code Extension

### Requirements

**R1.1**: Create a VS Code extension in `editors/vscode/` that provides syntax highlighting for `.gu` files.

**R1.2**: The extension must include:
- TextMate grammar (.tmLanguage.json) with scopes for all Gust syntax elements
- File icon for .gu files (distinct visual identity)
- Code snippets for common patterns (machine, state, transition, effect, handler)
- File nesting configuration to auto-collapse .g.rs and .g.go files under their .gu source

**R1.3**: Syntax highlighting must cover:
- Keywords (declaration, control flow, actions)
- Comments (// line comments)
- Strings (double-quoted)
- Numbers (integers, floats)
- Type names (capitalized identifiers)
- Function/effect/state names
- Operators and punctuation

**R1.4**: File icon should be a distinct color/shape visible in file explorer.

**R1.5**: Snippets should expand on Tab and include placeholder fields for quick editing.

**R1.6**: The extension must be publishable to VS Code marketplace (proper manifest, README, LICENSE).

### Acceptance Criteria

**AC1.1**: Opening a `.gu` file in VS Code shows syntax highlighting for all keywords, types, strings, and comments.

**AC1.2**: File explorer shows a distinct icon for `.gu` files (different from generic text icon).

**AC1.3**: Typing `machine` and pressing Tab inserts a full machine template with cursor at machine name.

**AC1.4**: Generated `.g.rs` and `.g.go` files appear nested under their `.gu` source in explorer (when file nesting is enabled).

**AC1.5**: Extension can be installed via `.vsix` file and works immediately without errors.

### Test Cases

**TC1.1 - Syntax Highlighting**

File: `test.gu`
```gust
use crate::models::Order;

type Money {
    cents: i64,
    currency: String,
}

machine PaymentProcessor {
    state Pending(amount: Money)
    state Charged(receipt: String)
    state Failed(reason: String)

    transition charge: Pending -> Charged | Failed

    async effect process_payment(amount: Money) -> String

    async on charge(ctx: Context) {
        let receipt = perform process_payment(amount);
        if receipt == "success" {
            goto Charged(receipt);
        } else {
            goto Failed("payment declined");
        }
    }
}
```

Expected highlighting:
- `use`, `type`, `machine`, `state`, `transition`, `async`, `effect`, `on`, `let`, `if`, `goto`, `perform` → keyword color
- `crate`, `models`, `Order`, `Money`, `String`, `i64`, `Context` → type color
- `"success"`, `"payment declined"` → string color
- `//` comments → comment color
- `->`, `|`, `::`, `()`, `{}` → punctuation color

**TC1.2 - File Icon**

Setup: Install extension, open workspace with `example.gu`
Expected: File explorer shows custom icon for `example.gu` (not default text icon)

**TC1.3 - Machine Snippet**

Action: Type `machine` in empty .gu file, press Tab
Expected:
```gust
machine ${1:MachineName} {
    state ${2:InitialState}

    transition ${3:transitionName}: ${2:InitialState} -> ${4:TargetState}

    on ${3:transitionName}(ctx: ${5:Context}) {
        ${0}
    }
}
```
Cursor at `MachineName`, Tab navigates through placeholders.

**TC1.4 - File Nesting**

Setup:
```
src/
  payment.gu
  payment.g.rs
  payment.g.go
```

Expected in VS Code explorer (with file nesting enabled):
```
src/
  ▶ payment.gu
    payment.g.rs
    payment.g.go
```

**TC1.5 - Extension Installation**

Action:
```bash
cd editors/vscode
npm install
npm run package
code --install-extension gust-*.vsix
```

Expected: Extension installs, shows in Extensions panel, .gu files get highlighting.

### Implementation Guide

**Step 1: Create Extension Directory Structure**

```
D:\Projects\gust\editors\vscode\
├── package.json
├── README.md
├── LICENSE
├── .vscodeignore
├── images\
│   └── icon.png
├── syntaxes\
│   └── gust.tmLanguage.json
└── snippets\
    └── gust.json
```

**Step 2: package.json**

File: `D:\Projects\gust\editors\vscode\package.json`
```json
{
  "name": "gust-lang",
  "displayName": "Gust Language Support",
  "description": "Syntax highlighting and snippets for the Gust state machine language",
  "version": "0.2.0",
  "publisher": "gust-lang",
  "engines": {
    "vscode": "^1.75.0"
  },
  "categories": [
    "Programming Languages",
    "Snippets"
  ],
  "keywords": [
    "gust",
    "state machine",
    "rust",
    "transpiler"
  ],
  "icon": "images/icon.png",
  "repository": {
    "type": "git",
    "url": "https://github.com/Dieshen/gust"
  },
  "activationEvents": [],
  "main": "./out/extension.js",
  "contributes": {
    "languages": [
      {
        "id": "gust",
        "aliases": [
          "Gust",
          "gust"
        ],
        "extensions": [
          ".gu"
        ],
        "configuration": "./language-configuration.json",
        "icon": {
          "light": "./images/icon.png",
          "dark": "./images/icon.png"
        }
      }
    ],
    "grammars": [
      {
        "language": "gust",
        "scopeName": "source.gust",
        "path": "./syntaxes/gust.tmLanguage.json"
      }
    ],
    "snippets": [
      {
        "language": "gust",
        "path": "./snippets/gust.json"
      }
    ],
    "configurationDefaults": {
      "[gust]": {
        "editor.tabSize": 4,
        "editor.insertSpaces": true,
        "editor.wordBasedSuggestions": "off"
      },
      "files.associations": {
        "*.gu": "gust"
      },
      "explorer.fileNesting.enabled": true,
      "explorer.fileNesting.patterns": {
        "*.gu": "${capture}.g.rs, ${capture}.g.go"
      }
    }
  },
  "scripts": {
    "vscode:prepublish": "npm run compile",
    "compile": "echo 'No TypeScript to compile'",
    "package": "vsce package",
    "publish": "vsce publish"
  },
  "devDependencies": {
    "@types/vscode": "^1.75.0",
    "@vscode/vsce": "^2.19.0"
  }
}
```

**Step 3: TextMate Grammar**

File: `D:\Projects\gust\editors\vscode\syntaxes\gust.tmLanguage.json`
```json
{
  "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
  "name": "Gust",
  "scopeName": "source.gust",
  "patterns": [
    { "include": "#comments" },
    { "include": "#keywords" },
    { "include": "#strings" },
    { "include": "#numbers" },
    { "include": "#types" },
    { "include": "#operators" },
    { "include": "#identifiers" }
  ],
  "repository": {
    "comments": {
      "patterns": [
        {
          "name": "comment.line.double-slash.gust",
          "begin": "//",
          "end": "$"
        }
      ]
    },
    "keywords": {
      "patterns": [
        {
          "name": "keyword.control.gust",
          "match": "\\b(if|else|match|return|let)\\b"
        },
        {
          "name": "keyword.declaration.gust",
          "match": "\\b(use|type|enum|machine|state|transition|effect|on)\\b"
        },
        {
          "name": "keyword.other.gust",
          "match": "\\b(async|goto|perform)\\b"
        },
        {
          "name": "constant.language.boolean.gust",
          "match": "\\b(true|false)\\b"
        }
      ]
    },
    "strings": {
      "patterns": [
        {
          "name": "string.quoted.double.gust",
          "begin": "\"",
          "end": "\"",
          "patterns": [
            {
              "name": "constant.character.escape.gust",
              "match": "\\\\."
            }
          ]
        }
      ]
    },
    "numbers": {
      "patterns": [
        {
          "name": "constant.numeric.float.gust",
          "match": "\\b\\d+\\.\\d+\\b"
        },
        {
          "name": "constant.numeric.integer.gust",
          "match": "\\b\\d+\\b"
        }
      ]
    },
    "types": {
      "patterns": [
        {
          "name": "support.type.primitive.gust",
          "match": "\\b(String|i64|i32|u64|u32|f64|f32|bool)\\b"
        },
        {
          "name": "support.type.builtin.gust",
          "match": "\\b(Option|Result|Vec)\\b"
        },
        {
          "name": "entity.name.type.gust",
          "match": "\\b[A-Z][a-zA-Z0-9_]*\\b"
        }
      ]
    },
    "operators": {
      "patterns": [
        {
          "name": "keyword.operator.arrow.gust",
          "match": "->"
        },
        {
          "name": "keyword.operator.namespace.gust",
          "match": "::"
        },
        {
          "name": "keyword.operator.pipe.gust",
          "match": "\\|"
        },
        {
          "name": "keyword.operator.comparison.gust",
          "match": "(==|!=|<=|>=|<|>)"
        },
        {
          "name": "keyword.operator.arithmetic.gust",
          "match": "(\\+|-|\\*|/|%)"
        },
        {
          "name": "keyword.operator.logical.gust",
          "match": "(&&|\\|\\||!)"
        },
        {
          "name": "keyword.operator.assignment.gust",
          "match": "="
        }
      ]
    },
    "identifiers": {
      "patterns": [
        {
          "name": "entity.name.function.gust",
          "match": "\\b[a-z_][a-zA-Z0-9_]*(?=\\s*\\()"
        },
        {
          "name": "variable.other.gust",
          "match": "\\b[a-z_][a-zA-Z0-9_]*\\b"
        }
      ]
    }
  }
}
```

**Step 4: Language Configuration**

File: `D:\Projects\gust\editors\vscode\language-configuration.json`
```json
{
  "comments": {
    "lineComment": "//"
  },
  "brackets": [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"]
  ],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"", "notIn": ["string"] }
  ],
  "surroundingPairs": [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"],
    ["\"", "\""]
  ],
  "folding": {
    "markers": {
      "start": "^\\s*//\\s*#?region\\b",
      "end": "^\\s*//\\s*#?endregion\\b"
    }
  },
  "indentationRules": {
    "increaseIndentPattern": "^.*\\{[^}\"']*$",
    "decreaseIndentPattern": "^\\s*\\}"
  }
}
```

**Step 5: Snippets**

File: `D:\Projects\gust\editors\vscode\snippets\gust.json`
```json
{
  "Machine": {
    "prefix": "machine",
    "body": [
      "machine ${1:MachineName} {",
      "\tstate ${2:InitialState}",
      "\tstate ${3:FinalState}",
      "",
      "\ttransition ${4:transitionName}: ${2:InitialState} -> ${3:FinalState}",
      "",
      "\ton ${4:transitionName}(ctx: ${5:Context}) {",
      "\t\t${0:goto ${3:FinalState}();}",
      "\t}",
      "}"
    ],
    "description": "Create a new state machine"
  },
  "State": {
    "prefix": "state",
    "body": [
      "state ${1:StateName}(${2:field}: ${3:Type})"
    ],
    "description": "Declare a state with fields"
  },
  "State (no fields)": {
    "prefix": "states",
    "body": [
      "state ${1:StateName}"
    ],
    "description": "Declare a state without fields"
  },
  "Transition": {
    "prefix": "transition",
    "body": [
      "transition ${1:name}: ${2:FromState} -> ${3:ToState}"
    ],
    "description": "Declare a transition"
  },
  "Transition (multi-target)": {
    "prefix": "transitionm",
    "body": [
      "transition ${1:name}: ${2:FromState} -> ${3:SuccessState} | ${4:FailureState}"
    ],
    "description": "Declare a transition with multiple targets"
  },
  "Effect": {
    "prefix": "effect",
    "body": [
      "effect ${1:effectName}(${2:param}: ${3:Type}) -> ${4:ReturnType}"
    ],
    "description": "Declare a synchronous effect"
  },
  "Async Effect": {
    "prefix": "aeffect",
    "body": [
      "async effect ${1:effectName}(${2:param}: ${3:Type}) -> ${4:ReturnType}"
    ],
    "description": "Declare an asynchronous effect"
  },
  "Handler": {
    "prefix": "on",
    "body": [
      "on ${1:transitionName}(${2:param}: ${3:Type}) {",
      "\t${0:goto ${4:TargetState}();}",
      "}"
    ],
    "description": "Implement a transition handler"
  },
  "Async Handler": {
    "prefix": "aon",
    "body": [
      "async on ${1:transitionName}(${2:param}: ${3:Type}) {",
      "\tlet ${4:result} = perform ${5:effectName}(${6:args});",
      "\t${0:goto ${7:TargetState}();}",
      "}"
    ],
    "description": "Implement an async transition handler"
  },
  "Type Struct": {
    "prefix": "type",
    "body": [
      "type ${1:TypeName} {",
      "\t${2:fieldName}: ${3:Type},",
      "}"
    ],
    "description": "Declare a struct type"
  },
  "Enum": {
    "prefix": "enum",
    "body": [
      "enum ${1:EnumName} {",
      "\t${2:VariantOne},",
      "\t${3:VariantTwo}(${4:Type}),",
      "}"
    ],
    "description": "Declare an enum type"
  },
  "Goto": {
    "prefix": "goto",
    "body": [
      "goto ${1:StateName}(${2:args});"
    ],
    "description": "Transition to a new state"
  },
  "Perform": {
    "prefix": "perform",
    "body": [
      "let ${1:result} = perform ${2:effectName}(${3:args});"
    ],
    "description": "Call an effect and bind result"
  },
  "If-Else": {
    "prefix": "if",
    "body": [
      "if ${1:condition} {",
      "\t${2}",
      "} else {",
      "\t${0}",
      "}"
    ],
    "description": "If-else statement"
  },
  "Match": {
    "prefix": "match",
    "body": [
      "match ${1:expr} {",
      "\t${2:Pattern} => {",
      "\t\t${3}",
      "\t}",
      "\t_ => {",
      "\t\t${0}",
      "\t}",
      "}"
    ],
    "description": "Match expression"
  },
  "Use": {
    "prefix": "use",
    "body": [
      "use ${1:crate::module::Type};"
    ],
    "description": "Import declaration"
  }
}
```

**Step 6: README and LICENSE**

File: `D:\Projects\gust\editors\vscode\README.md`
```markdown
# Gust Language Support for VS Code

Provides syntax highlighting, snippets, and file association for the Gust state machine language.

## Features

- **Syntax Highlighting**: Full support for Gust keywords, types, strings, comments, and operators
- **Code Snippets**: Quick templates for machines, states, transitions, effects, and handlers
- **File Icons**: Distinct icon for `.gu` files in the file explorer
- **File Nesting**: Auto-collapse generated `.g.rs` and `.g.go` files under their `.gu` source

## Usage

### Snippets

Type the prefix and press Tab to expand:

- `machine` - Create a new state machine
- `state` - Declare a state with fields
- `transition` - Declare a transition
- `effect` - Declare an effect
- `on` - Implement a handler
- `enum` - Declare an enum type
- `type` - Declare a struct type

### File Nesting

Generated files (`.g.rs`, `.g.go`) automatically nest under their `.gu` source in the file explorer. Enable file nesting in VS Code settings if not already enabled:

```json
{
  "explorer.fileNesting.enabled": true
}
```

## Requirements

- VS Code 1.75.0 or higher
- Gust compiler (`gust` CLI) for code generation

## Installation

Install from the VS Code Marketplace or from VSIX:

```bash
code --install-extension gust-lang-*.vsix
```

## Contributing

Issues and pull requests welcome at https://github.com/Dieshen/gust

## License

MIT License - see LICENSE file
```

File: `D:\Projects\gust\editors\vscode\LICENSE`
```
MIT License

Copyright (c) 2024 Gust Language Project

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

**Step 7: .vscodeignore**

File: `D:\Projects\gust\editors\vscode\.vscodeignore`
```
.vscode/**
.vscode-test/**
node_modules/**
*.vsix
.gitignore
```

**Step 8: Icon File**

Create or download a 128x128 PNG icon for `.gu` files. Suggested design:
- Color: Teal/cyan (matches "Gust" wind theme)
- Symbol: Stylized "G" or state machine diagram (circles + arrows)
- Save as: `D:\Projects\gust\editors\vscode\images\icon.png`

**Step 9: Build and Test**

```bash
cd D:\Projects\gust\editors\vscode
npm install
npm run package  # Creates gust-lang-0.2.0.vsix
code --install-extension gust-lang-0.2.0.vsix
```

Open a `.gu` file and verify:
- Syntax highlighting works
- File icon appears in explorer
- Snippets expand on Tab
- File nesting works (if `.g.rs` files present)

---

## Feature 2: Language Server (LSP)

### Requirements

**R2.1**: Create `gust-lsp` crate that implements a Language Server Protocol server for Gust.

**R2.2**: The LSP must provide:
- **Syntax diagnostics**: Parse errors with line/column, displayed in Problems panel
- **Go-to-definition**: Click on state/effect name → jump to declaration
- **Hover info**: Hover over state → show field types, hover over effect → show signature
- **Autocomplete**: After `goto ` → suggest valid target states, after `perform ` → suggest effects

**R2.3**: Use `tower-lsp` crate for LSP protocol handling.

**R2.4**: The server must:
- Parse `.gu` files on `textDocument/didOpen` and `textDocument/didChange`
- Cache parse results per document URI
- Report diagnostics on every parse
- Provide completions and hover based on cached AST

**R2.5**: VS Code extension must be updated to launch the LSP server.

**R2.6**: Server must handle invalid/incomplete input gracefully (partial parse recovery).

### Acceptance Criteria

**AC2.1**: Opening a `.gu` file with syntax errors shows red squiggles in the editor and errors in Problems panel.

**AC2.2**: Ctrl+Click on a state name in a `goto` statement jumps to the state declaration.

**AC2.3**: Hovering over a state name shows a tooltip with field names and types.

**AC2.4**: Typing `goto ` and pressing Ctrl+Space shows autocomplete suggestions for valid target states.

**AC2.5**: Typing `perform ` and pressing Ctrl+Space shows autocomplete suggestions for declared effects.

**AC2.6**: Hover over an effect name shows its parameter types and return type.

### Test Cases

**TC2.1 - Syntax Diagnostics**

File: `test.gu`
```gust
machine Broken {
    state Start

    transition go: Start -> End  // End not declared
}
```

Expected: Red squiggle under `End`, diagnostic message:
```
Undefined state 'End' in transition target
```

**TC2.2 - Go-to-Definition (State)**

File: `test.gu`
```gust
machine Test {
    state Idle(count: i64)  // Line 2
    state Running(count: i64)

    transition start: Idle -> Running

    on start(ctx: Context) {
        goto Running(0);  // Ctrl+Click on "Running" here
    }
}
```

Expected: Cursor jumps to line 3 (`state Running(count: i64)`)

**TC2.3 - Hover (State)**

File: `test.gu`
```gust
machine Test {
    state Pending(order: Order, total: Money)
    // ... hover over "Pending" in a goto statement
}
```

Expected hover tooltip:
```
state Pending
Fields:
  order: Order
  total: Money
```

**TC2.4 - Autocomplete (Goto)**

File: `test.gu`
```gust
machine Test {
    state Start
    state Middle
    state End

    transition step: Start -> Middle | End

    on step(ctx: Context) {
        goto   // Ctrl+Space here
    }
}
```

Expected autocomplete items:
- `Middle`
- `End`

(Not `Start` because handler is in `on step` which is `Start -> Middle | End`, only targets shown)

**TC2.5 - Autocomplete (Perform)**

File: `test.gu`
```gust
machine Test {
    state Active

    effect calculate(x: i64) -> i64
    effect validate(s: String) -> bool

    transition run: Active -> Active

    on run(ctx: Context) {
        let r = perform   // Ctrl+Space here
    }
}
```

Expected autocomplete items:
- `calculate`
- `validate`

**TC2.6 - Hover (Effect)**

File: `test.gu`
```gust
machine Test {
    async effect process_payment(amount: Money) -> Receipt
    // hover over "process_payment" in a perform statement
}
```

Expected hover tooltip:
```
async effect process_payment
Parameters:
  amount: Money
Returns: Receipt
```

### Implementation Guide

**Step 1: Create gust-lsp Crate**

File: `D:\Projects\gust\gust-lsp\Cargo.toml`
```toml
[package]
name = "gust-lsp"
version = "0.2.0"
edition = "2021"
description = "Language Server Protocol implementation for Gust"

[[bin]]
name = "gust-lsp"
path = "src/main.rs"

[dependencies]
gust-lang = { path = "../gust-lang" }
tower-lsp = "0.20"
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

**Step 2: Main LSP Server**

File: `D:\Projects\gust\gust-lsp\src\main.rs`
```rust
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use gust_lang::{parse_program, ast::Program};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
struct DocumentData {
    source: String,
    program: Option<Program>,
    parse_error: Option<String>,
}

struct GustLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentData>>>,
}

impl GustLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn parse_and_update(&self, uri: Url, text: String) {
        let program_result = parse_program(&text);

        let doc_data = match program_result {
            Ok(program) => DocumentData {
                source: text.clone(),
                program: Some(program),
                parse_error: None,
            },
            Err(error) => DocumentData {
                source: text.clone(),
                program: None,
                parse_error: Some(error),
            },
        };

        // Publish diagnostics
        let diagnostics = self.create_diagnostics(&doc_data);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;

        // Store document
        self.documents.write().await.insert(uri, doc_data);
    }

    fn create_diagnostics(&self, doc: &DocumentData) -> Vec<Diagnostic> {
        if let Some(ref error) = doc.parse_error {
            // Parse pest error to extract line/column
            let (line, col, message) = parse_error_location(error);

            vec![Diagnostic {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: (col + 1) as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                message: message.to_string(),
                source: Some("gust".to_string()),
                ..Default::default()
            }]
        } else {
            // TODO: Validation diagnostics (unreachable states, etc.)
            vec![]
        }
    }
}

fn parse_error_location(error_msg: &str) -> (usize, usize, String) {
    // pest errors have format like: " --> 5:12"
    // Extract line:column and clean message
    // Simplified parser - production version should use regex
    if let Some(pos) = error_msg.find("-->") {
        let after = &error_msg[pos + 3..].trim();
        if let Some(colon) = after.find(':') {
            let line_str = &after[..colon].trim();
            let rest = &after[colon + 1..];
            let col_end = rest.find(|c: char| !c.is_numeric()).unwrap_or(rest.len());
            let col_str = &rest[..col_end].trim();

            let line = line_str.parse::<usize>().unwrap_or(0).saturating_sub(1);
            let col = col_str.parse::<usize>().unwrap_or(0);

            // Extract clean message (everything before "-->")
            let message = error_msg[..pos].trim().to_string();
            return (line, col, message);
        }
    }

    (0, 0, error_msg.to_string())
}

#[tower_lsp::async_trait]
impl LanguageServer for GustLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![" ".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Gust LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.parse_and_update(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.parse_and_update(uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.write().await.remove(&params.text_document.uri);
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(ref program) = doc.program {
                // Find word at cursor position
                let word = word_at_position(&doc.source, position);

                // Search for state/effect definition
                if let Some(location) = find_definition(program, &doc.source, &word) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri,
                        range: location,
                    })));
                }
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(ref program) = doc.program {
                let word = word_at_position(&doc.source, position);

                // Generate hover info for states/effects
                if let Some(hover_text) = generate_hover(program, &word) {
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: hover_text,
                        }),
                        range: None,
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(ref program) = doc.program {
                // Check context - are we after "goto " or "perform "?
                let context = completion_context(&doc.source, position);

                let items = match context.as_str() {
                    "goto" => {
                        // Suggest valid target states based on current handler
                        suggest_states(program)
                    }
                    "perform" => {
                        // Suggest effects
                        suggest_effects(program)
                    }
                    _ => vec![],
                };

                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }
        }

        Ok(None)
    }
}

fn word_at_position(source: &str, position: Position) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if let Some(line) = lines.get(position.line as usize) {
        let chars: Vec<char> = line.chars().collect();
        let col = position.character as usize;

        if col >= chars.len() {
            return String::new();
        }

        // Find word boundaries
        let mut start = col;
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }

        let mut end = col;
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }

        chars[start..end].iter().collect()
    } else {
        String::new()
    }
}

fn find_definition(program: &Program, source: &str, word: &str) -> Option<Range> {
    // Search for state declaration matching word
    for machine in &program.machines {
        for state in &machine.states {
            if state.name == word {
                return find_range_in_source(source, &format!("state {}", word));
            }
        }

        // Search for effect declaration
        for effect in &machine.effects {
            if effect.name == word {
                return find_range_in_source(source, &format!("effect {}", word));
            }
        }
    }

    None
}

fn find_range_in_source(source: &str, needle: &str) -> Option<Range> {
    for (line_num, line) in source.lines().enumerate() {
        if let Some(col) = line.find(needle) {
            return Some(Range {
                start: Position {
                    line: line_num as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line_num as u32,
                    character: (col + needle.len()) as u32,
                },
            });
        }
    }
    None
}

fn generate_hover(program: &Program, word: &str) -> Option<String> {
    for machine in &program.machines {
        // Hover for state
        for state in &machine.states {
            if state.name == word {
                let mut info = format!("**state {}**\n\n", word);
                if !state.fields.is_empty() {
                    info.push_str("Fields:\n");
                    for field in &state.fields {
                        info.push_str(&format!("  - `{}`: {:?}\n", field.name, field.ty));
                    }
                }
                return Some(info);
            }
        }

        // Hover for effect
        for effect in &machine.effects {
            if effect.name == word {
                let async_kw = if effect.is_async { "async " } else { "" };
                let mut info = format!("**{}effect {}**\n\n", async_kw, word);
                if !effect.params.is_empty() {
                    info.push_str("Parameters:\n");
                    for param in &effect.params {
                        info.push_str(&format!("  - `{}`: {:?}\n", param.name, param.ty));
                    }
                }
                info.push_str(&format!("\nReturns: `{:?}`", effect.return_type));
                return Some(info);
            }
        }
    }

    None
}

fn completion_context(source: &str, position: Position) -> String {
    let lines: Vec<&str> = source.lines().collect();
    if let Some(line) = lines.get(position.line as usize) {
        let before_cursor = &line[..position.character as usize];

        if before_cursor.trim_end().ends_with("goto") {
            return "goto".to_string();
        } else if before_cursor.trim_end().ends_with("perform") {
            return "perform".to_string();
        }
    }

    String::new()
}

fn suggest_states(program: &Program) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for machine in &program.machines {
        for state in &machine.states {
            items.push(CompletionItem {
                label: state.name.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(format!("state with {} fields", state.fields.len())),
                ..Default::default()
            });
        }
    }

    items
}

fn suggest_effects(program: &Program) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for machine in &program.machines {
        for effect in &machine.effects {
            let async_label = if effect.is_async { " (async)" } else { "" };
            items.push(CompletionItem {
                label: effect.name.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(format!("effect{}", async_label)),
                ..Default::default()
            });
        }
    }

    items
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| GustLanguageServer::new(client));

    Server::new(stdin, stdout, socket).serve(service).await;
}
```

**Step 3: Update VS Code Extension to Launch LSP**

File: `D:\Projects\gust\editors\vscode\package.json` (update)

Add to `contributes`:
```json
"configuration": {
  "type": "object",
  "title": "Gust",
  "properties": {
    "gust.lsp.path": {
      "type": "string",
      "default": "gust-lsp",
      "description": "Path to the gust-lsp executable"
    }
  }
}
```

**Step 4: Extension TypeScript Client**

File: `D:\Projects\gust\editors\vscode\src\extension.ts`
```typescript
import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    // Get LSP executable path from config
    const config = workspace.getConfiguration('gust');
    const lspPath = config.get<string>('lsp.path', 'gust-lsp');

    // Server options
    const serverOptions: ServerOptions = {
        command: lspPath,
        args: [],
        transport: TransportKind.stdio
    };

    // Client options
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'gust' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher('**/*.gu')
        }
    };

    // Create and start client
    client = new LanguageClient(
        'gustLanguageServer',
        'Gust Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
```

**Step 5: Update package.json for TypeScript**

File: `D:\Projects\gust\editors\vscode\package.json` (update dependencies)
```json
"devDependencies": {
  "@types/vscode": "^1.75.0",
  "@types/node": "^18.0.0",
  "@vscode/vsce": "^2.19.0",
  "typescript": "^5.0.0",
  "vscode-languageclient": "^9.0.0"
},
"scripts": {
  "vscode:prepublish": "npm run compile",
  "compile": "tsc -p ./",
  "watch": "tsc -watch -p ./",
  "package": "vsce package",
  "publish": "vsce publish"
}
```

**Step 6: TypeScript Config**

File: `D:\Projects\gust\editors\vscode\tsconfig.json`
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "outDir": "out",
    "rootDir": "src",
    "sourceMap": true,
    "strict": true,
    "esModuleInterop": true
  },
  "include": ["src"],
  "exclude": ["node_modules"]
}
```

**Step 7: Build LSP Server**

```bash
cd D:\Projects\gust
cargo build --release --bin gust-lsp
# Copy to PATH or configure extension to find it
```

**Step 8: Test LSP**

1. Build extension with TypeScript client: `cd editors/vscode && npm run compile`
2. Package extension: `npm run package`
3. Install: `code --install-extension gust-lang-*.vsix`
4. Open a `.gu` file with errors
5. Verify diagnostics appear in Problems panel
6. Test go-to-definition (Ctrl+Click on state name)
7. Test hover (hover over state/effect)
8. Test autocomplete (after `goto ` or `perform `)

---

## Feature 3: Error Messages

### Requirements

**R3.1**: Create a structured error type `GustError` with fields for file path, line, column, message, optional suggestion, and source code snippet.

**R3.2**: Add a validation module `gust-lang/src/validate.rs` that performs semantic checks after parsing:
- Detect duplicate state names
- Detect unreachable states (states with no incoming transitions)
- Detect unused effects (effects never called via `perform`)
- Detect transitions referencing nonexistent states
- Detect `goto` targeting invalid states for the current transition

**R3.3**: Implement "did you mean?" suggestions using string similarity (e.g., `strsim` or `similar` crate) for:
- Misspelled state names in transitions and goto statements
- Misspelled effect names in perform statements
- Misspelled type names

**R3.4**: Format errors with colors using `colored` crate:
- Red for error markers and messages
- Yellow for warnings
- Cyan for suggestions
- Show file:line:col prefix
- Show source code snippet with caret pointer (`^`) under the error location

**R3.5**: Error messages must be actionable and human-friendly. Avoid parser jargon like "expected EOI" - translate to "expected end of file".

**R3.6**: Group multiple errors by file and display them all (don't stop at first error).

### Acceptance Criteria

**AC3.1**: Parse errors show file:line:col, a human-readable message, and a snippet of the source code with caret pointer.

**AC3.2**: Validation errors for unreachable states appear with warning severity and list the unreachable state names.

**AC3.3**: When a user types `goto Runing` (typo for `Running`), the error includes suggestion: "did you mean 'Running'?".

**AC3.4**: Colored output is displayed in terminal (colors disabled if NO_COLOR env var is set).

**AC3.5**: Running `gust build broken.gu` shows all parse and validation errors before exiting with non-zero status.

### Test Cases

**TC3.1 - Parse Error with Snippet**

File: `test.gu`
```gust
machine Test {
    state Start
    transition go: Start -> End  // Missing state End
```

Command: `gust build test.gu`

Expected output (colors shown as [COLOR]):
```
[RED]error:[RESET] unexpected end of file
 [CYAN]-->[RESET] test.gu:3:37
  [CYAN]|[RESET]
3 [CYAN]|[RESET]     transition go: Start -> End  // Missing state End
  [CYAN]|[RESET]                                     [RED]^[RESET]
  [CYAN]|[RESET]
  [CYAN]=[RESET] [CYAN]note:[RESET] expected closing brace '}'
```

**TC3.2 - Validation Error (Undefined State)**

File: `test.gu`
```gust
machine Test {
    state Start

    transition go: Start -> End
}
```

Command: `gust build test.gu`

Expected output:
```
[RED]error:[RESET] undefined state 'End' in transition target
 [CYAN]-->[RESET] test.gu:4:33
  [CYAN]|[RESET]
4 [CYAN]|[RESET]     transition go: Start -> End
  [CYAN]|[RESET]                                 [RED]^^^[RESET]
```

**TC3.3 - Did You Mean Suggestion**

File: `test.gu`
```gust
machine Test {
    state Running

    transition start: Idle -> Running

    on start(ctx: Context) {
        goto Runing();  // Typo
    }
}
```

Command: `gust build test.gu`

Expected output:
```
[RED]error:[RESET] undefined state 'Runing'
 [CYAN]-->[RESET] test.gu:7:14
  [CYAN]|[RESET]
7 [CYAN]|[RESET]         goto Runing();
  [CYAN]|[RESET]              [RED]^^^^^^[RESET]
  [CYAN]|[RESET]
  [CYAN]=[RESET] [YELLOW]help:[RESET] did you mean 'Running'?
```

**TC3.4 - Unreachable State Warning**

File: `test.gu`
```gust
machine Test {
    state Start
    state Unreachable

    transition go: Start -> Start
}
```

Command: `gust build test.gu`

Expected output:
```
[YELLOW]warning:[RESET] state 'Unreachable' is never reached
 [CYAN]-->[RESET] test.gu:3:11
  [CYAN]|[RESET]
3 [CYAN]|[RESET]     state Unreachable
  [CYAN]|[RESET]           [YELLOW]^^^^^^^^^^^[RESET]
  [CYAN]|[RESET]
  [CYAN]=[RESET] [CYAN]note:[RESET] no transitions target this state
```

**TC3.5 - Multiple Errors**

File: `test.gu`
```gust
machine Test {
    state Start
    state Middle

    transition go: Start -> End     // Error: End undefined
    transition next: Middle -> Nowhere  // Error: Nowhere undefined
}
```

Command: `gust build test.gu`

Expected output:
```
[RED]error:[RESET] undefined state 'End' in transition target
 [CYAN]-->[RESET] test.gu:5:34
  [CYAN]|[RESET]
5 [CYAN]|[RESET]     transition go: Start -> End
  [CYAN]|[RESET]                                  [RED]^^^[RESET]

[RED]error:[RESET] undefined state 'Nowhere' in transition target
 [CYAN]-->[RESET] test.gu:6:36
  [CYAN]|[RESET]
6 [CYAN]|[RESET]     transition next: Middle -> Nowhere
  [CYAN]|[RESET]                                    [RED]^^^^^^^[RESET]

[YELLOW]warning:[RESET] state 'Middle' is never reached
 [CYAN]-->[RESET] test.gu:3:11
  [CYAN]|[RESET]
3 [CYAN]|[RESET]     state Middle
  [CYAN]|[RESET]           [YELLOW]^^^^^^[RESET]

[RED]error:[RESET] build failed with 2 errors and 1 warning
```

### Implementation Guide

**Step 1: Add Dependencies**

File: `D:\Projects\gust\gust-lang\Cargo.toml` (update dependencies)
```toml
[dependencies]
pest = "2.7"
pest_derive = "2.7"
strsim = "0.11"  # Levenshtein distance for suggestions
colored = "2.1"
```

**Step 2: Create Error Type**

File: `D:\Projects\gust\gust-lang\src\error.rs`
```rust
use colored::*;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GustError {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub length: usize,
    pub message: String,
    pub severity: ErrorSeverity,
    pub suggestion: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorSeverity {
    Error,
    Warning,
}

impl GustError {
    pub fn error(file: String, line: usize, col: usize, message: String) -> Self {
        Self {
            file,
            line,
            col,
            length: 1,
            message,
            severity: ErrorSeverity::Error,
            suggestion: None,
            note: None,
        }
    }

    pub fn warning(file: String, line: usize, col: usize, message: String) -> Self {
        Self {
            file,
            line,
            col,
            length: 1,
            message,
            severity: ErrorSeverity::Warning,
            suggestion: None,
            note: None,
        }
    }

    pub fn with_length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }

    pub fn with_suggestion(mut self, suggestion: String) -> Self {
        self.suggestion = Some(suggestion);
        self
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.note = Some(note);
        self
    }

    pub fn format(&self, source: &str) -> String {
        let mut output = String::new();

        // Header: error/warning: message
        let severity_label = match self.severity {
            ErrorSeverity::Error => "error".red().bold(),
            ErrorSeverity::Warning => "warning".yellow().bold(),
        };
        output.push_str(&format!("{}: {}\n", severity_label, self.message.bold()));

        // Location: --> file:line:col
        output.push_str(&format!(
            " {} {}:{}:{}\n",
            "-->".cyan().bold(),
            self.file,
            self.line + 1,
            self.col
        ));

        // Source snippet
        let lines: Vec<&str> = source.lines().collect();
        if let Some(line_text) = lines.get(self.line) {
            let line_num = (self.line + 1).to_string();
            let line_num_width = line_num.len();

            // Empty line before
            output.push_str(&format!("{} {}\n", " ".repeat(line_num_width), "|".cyan()));

            // Source line
            output.push_str(&format!(
                "{} {} {}\n",
                line_num.cyan().bold(),
                "|".cyan(),
                line_text
            ));

            // Caret pointer
            let pointer_color = match self.severity {
                ErrorSeverity::Error => Color::Red,
                ErrorSeverity::Warning => Color::Yellow,
            };
            let caret = "^".repeat(self.length.max(1)).color(pointer_color).bold();
            output.push_str(&format!(
                "{} {} {}{}\n",
                " ".repeat(line_num_width),
                "|".cyan(),
                " ".repeat(self.col),
                caret
            ));
        }

        // Suggestion
        if let Some(ref suggestion) = self.suggestion {
            output.push_str(&format!(
                "{} {} {}: {}\n",
                " ".repeat(self.file.len() + 10),
                "|".cyan(),
                "=".cyan().bold(),
                format!("help: {}", suggestion).yellow()
            ));
        }

        // Note
        if let Some(ref note) = self.note {
            output.push_str(&format!(
                "{} {} {}: {}\n",
                " ".repeat(self.file.len() + 10),
                "|".cyan(),
                "=".cyan().bold(),
                format!("note: {}", note).cyan()
            ));
        }

        output
    }
}

impl fmt::Display for GustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GustError {}
```

**Step 3: Create Validation Module**

File: `D:\Projects\gust\gust-lang\src\validate.rs`
```rust
use crate::ast::{Program, Machine, State, Transition, Effect};
use crate::error::{GustError, ErrorSeverity};
use strsim::levenshtein;
use std::collections::{HashSet, HashMap};

pub struct ValidationContext {
    pub errors: Vec<GustError>,
    pub file: String,
    pub source: String,
}

impl ValidationContext {
    pub fn new(file: String, source: String) -> Self {
        Self {
            errors: Vec::new(),
            file,
            source,
        }
    }

    pub fn add_error(&mut self, error: GustError) {
        self.errors.push(error);
    }

    pub fn has_errors(&self) -> bool {
        self.errors.iter().any(|e| e.severity == ErrorSeverity::Error)
    }
}

pub fn validate_program(program: &Program, ctx: &mut ValidationContext) {
    for machine in &program.machines {
        validate_machine(machine, ctx);
    }
}

fn validate_machine(machine: &Machine, ctx: &mut ValidationContext) {
    // Check for duplicate state names
    let mut state_names = HashSet::new();
    for state in &machine.states {
        if !state_names.insert(&state.name) {
            let error = GustError::error(
                ctx.file.clone(),
                state.line.unwrap_or(0),
                state.col.unwrap_or(0),
                format!("duplicate state '{}'", state.name),
            )
            .with_length(state.name.len());
            ctx.add_error(error);
        }
    }

    // Build state index
    let state_set: HashSet<_> = machine.states.iter().map(|s| s.name.as_str()).collect();

    // Check transition targets
    for transition in &machine.transitions {
        for target in &transition.targets {
            if !state_set.contains(target.as_str()) {
                let suggestion = find_similar_name(target, &state_set);
                let mut error = GustError::error(
                    ctx.file.clone(),
                    transition.line.unwrap_or(0),
                    transition.col.unwrap_or(0),
                    format!("undefined state '{}' in transition target", target),
                )
                .with_length(target.len());

                if let Some(similar) = suggestion {
                    error = error.with_suggestion(format!("did you mean '{}'?", similar));
                }

                ctx.add_error(error);
            }
        }
    }

    // Check for unreachable states
    let mut reachable_states = HashSet::new();
    for transition in &machine.transitions {
        for target in &transition.targets {
            reachable_states.insert(target.as_str());
        }
    }

    for state in &machine.states {
        if !reachable_states.contains(state.name.as_str()) && state.name != machine.states[0].name {
            let warning = GustError::warning(
                ctx.file.clone(),
                state.line.unwrap_or(0),
                state.col.unwrap_or(0),
                format!("state '{}' is never reached", state.name),
            )
            .with_length(state.name.len())
            .with_note("no transitions target this state".to_string());
            ctx.add_error(warning);
        }
    }

    // Check for unused effects
    let effect_set: HashSet<_> = machine.effects.iter().map(|e| e.name.as_str()).collect();
    let mut used_effects = HashSet::new();

    // Scan handlers for perform statements (simplified - full impl would walk AST)
    for handler in &machine.handlers {
        for stmt in &handler.body {
            if let Some(effect_name) = extract_perform_effect(stmt) {
                used_effects.insert(effect_name);
            }
        }
    }

    for effect in &machine.effects {
        if !used_effects.contains(effect.name.as_str()) {
            let warning = GustError::warning(
                ctx.file.clone(),
                effect.line.unwrap_or(0),
                effect.col.unwrap_or(0),
                format!("effect '{}' is never used", effect.name),
            )
            .with_length(effect.name.len())
            .with_note("no 'perform' statements call this effect".to_string());
            ctx.add_error(warning);
        }
    }
}

fn find_similar_name<'a>(target: &str, candidates: &HashSet<&'a str>) -> Option<&'a str> {
    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let distance = levenshtein(target, candidate);
        if distance < best_distance && distance <= 2 {
            best_distance = distance;
            best_match = Some(*candidate);
        }
    }

    best_match
}

fn extract_perform_effect(stmt: &crate::ast::Statement) -> Option<&str> {
    // Simplified - real implementation would recursively walk statement AST
    // For now, return None - full impl needed
    None
}
```

**Step 4: Integrate Validation into CLI**

File: `D:\Projects\gust\gust-cli\src\main.rs` (update build command)
```rust
use gust_lang::{parse_program, codegen, validate_program, ValidationContext, GustError};
use colored::*;
use std::process;

fn build_command(file: &str, target: &str, package: Option<String>, output: Option<String>) {
    let source = std::fs::read_to_string(file).expect("Failed to read file");

    // Parse
    let program = match parse_program(&source) {
        Ok(prog) => prog,
        Err(e) => {
            let error = parse_pest_error(e, file.to_string(), &source);
            eprintln!("{}", error.format(&source));
            process::exit(1);
        }
    };

    // Validate
    let mut ctx = ValidationContext::new(file.to_string(), source.clone());
    validate_program(&program, &mut ctx);

    // Print all errors and warnings
    for error in &ctx.errors {
        eprintln!("{}", error.format(&source));
    }

    // Exit if errors
    if ctx.has_errors() {
        let error_count = ctx.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).count();
        let warning_count = ctx.errors.len() - error_count;
        eprintln!(
            "\n{}: build failed with {} {} and {} {}",
            "error".red().bold(),
            error_count,
            if error_count == 1 { "error" } else { "errors" },
            warning_count,
            if warning_count == 1 { "warning" } else { "warnings" }
        );
        process::exit(1);
    }

    // Proceed with codegen...
}

fn parse_pest_error(pest_err: pest::error::Error<Rule>, file: String, source: &str) -> GustError {
    // Extract line/col from pest error
    let (line, col) = match pest_err.line_col {
        pest::error::LineColLocation::Pos((l, c)) => (l - 1, c),
        pest::error::LineColLocation::Span((l, c), _) => (l - 1, c),
    };

    let message = humanize_pest_message(&pest_err.variant);

    GustError::error(file, line, col, message)
        .with_note("check syntax and closing braces".to_string())
}

fn humanize_pest_message(variant: &pest::error::ErrorVariant<Rule>) -> String {
    match variant {
        pest::error::ErrorVariant::ParsingError { positives, .. } => {
            if positives.is_empty() {
                "unexpected input".to_string()
            } else {
                format!("expected {:?}", positives)
            }
        }
        pest::error::ErrorVariant::CustomError { message } => message.clone(),
    }
}
```

**Step 5: Update AST to Include Line/Col Info**

File: `D:\Projects\gust\gust-lang\src\ast.rs` (add optional fields)
```rust
pub struct State {
    pub name: String,
    pub fields: Vec<Field>,
    pub line: Option<usize>,  // Add this
    pub col: Option<usize>,   // Add this
}

pub struct Transition {
    pub name: String,
    pub from: String,
    pub targets: Vec<String>,
    pub line: Option<usize>,  // Add this
    pub col: Option<usize>,   // Add this
}

pub struct Effect {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
    pub is_async: bool,
    pub line: Option<usize>,  // Add this
    pub col: Option<usize>,   // Add this
}
```

**Step 6: Update Parser to Capture Line/Col**

File: `D:\Projects\gust\gust-lang\src\parser.rs` (update parsing functions)
```rust
fn parse_state(pair: Pair<Rule>) -> State {
    let span = pair.as_span();
    let (line, col) = span.start_pos().line_col();

    // ... existing parsing logic ...

    State {
        name,
        fields,
        line: Some(line - 1),
        col: Some(col - 1),
    }
}
```

---

## Feature 4: Tooling

### Requirements

**R4.1**: Implement `gust init [project_name]` command that scaffolds a new Gust project with:
- `Cargo.toml` configured with gust-build dependency
- `build.rs` that compiles .gu files
- `src/main.rs` with starter code
- `src/machine.gu` with example state machine

**R4.2**: Implement `gust fmt <file.gu>` command that formats .gu files:
- Parse to AST, then emit canonically formatted code
- 4-space indentation (configurable via .gust-fmt.toml)
- Blank lines between declarations (type, machine, states, transitions, handlers)
- Aligned field types in struct declarations
- Preserve comments in original locations

**R4.3**: Implement `gust check <file.gu>` command that validates without codegen:
- Parse .gu file
- Run validation pass
- Report errors and warnings
- Exit with status 0 if no errors, 1 if errors found
- Faster than `gust build` for quick feedback

**R4.4**: Implement `gust diagram <file.gu>` command that generates Mermaid state diagrams:
- Parse .gu file
- Emit Mermaid stateDiagram-v2 markdown
- States become nodes
- Transitions become edges with labels
- Output to stdout or file (`-o diagram.md`)

**R4.5**: `gust fmt` should be idempotent - running twice produces same output.

**R4.6**: `gust init` should detect if project already exists and ask for confirmation before overwriting.

### Acceptance Criteria

**AC4.1**: Running `gust init my-project` creates directory structure with working Cargo project that compiles a sample .gu file.

**AC4.2**: Running `gust fmt unformatted.gu` rewrites the file with consistent formatting.

**AC4.3**: Running `gust check broken.gu` reports errors without generating .g.rs files.

**AC4.4**: Running `gust diagram payment.gu -o diagram.md` creates a Mermaid diagram file that renders correctly on GitHub.

**AC4.5**: Running `gust fmt file.gu` twice produces identical output both times.

**AC4.6**: Running `gust init` in existing project directory prompts user before overwriting files.

### Test Cases

**TC4.1 - Init Command**

Command:
```bash
gust init payment-service
cd payment-service
cargo build
cargo run
```

Expected directory structure:
```
payment-service/
├── Cargo.toml
├── build.rs
└── src/
    ├── main.rs
    └── machine.gu
```

Expected `Cargo.toml`:
```toml
[package]
name = "payment-service"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "0.2"

[build-dependencies]
gust-build = "0.2"
```

Expected `build.rs`:
```rust
fn main() {
    gust_build::compile_gust_files();
}
```

Expected `src/machine.gu`:
```gust
machine Example {
    state Idle
    state Active(count: i64)
    state Done

    transition start: Idle -> Active
    transition finish: Active -> Done

    on start(ctx: Context) {
        goto Active(0);
    }

    on finish(ctx: Context) {
        goto Done();
    }
}
```

Expected `cargo build` output: Success, generates `src/machine.g.rs`

**TC4.2 - Format Command**

Input file `unformatted.gu`:
```gust
machine Test{state Start(x:i64,y:String)
state End
transition go:Start->End
on go(ctx:Context){goto End();}}
```

Command: `gust fmt unformatted.gu`

Expected output (file rewritten):
```gust
machine Test {
    state Start(x: i64, y: String)
    state End

    transition go: Start -> End

    on go(ctx: Context) {
        goto End();
    }
}
```

**TC4.3 - Check Command**

File: `broken.gu`
```gust
machine Test {
    state Start

    transition go: Start -> Missing
}
```

Command: `gust check broken.gu`

Expected output:
```
error: undefined state 'Missing' in transition target
 --> broken.gu:4:33
  |
4 |     transition go: Start -> Missing
  |                                 ^^^^^^^

error: build failed with 1 error and 0 warnings
```

Exit code: 1

Expected: NO `.g.rs` file generated

**TC4.4 - Diagram Command**

File: `payment.gu`
```gust
machine Payment {
    state Pending
    state Authorized
    state Captured
    state Failed

    transition authorize: Pending -> Authorized | Failed
    transition capture: Authorized -> Captured | Failed
    transition fail: Pending -> Failed
}
```

Command: `gust diagram payment.gu -o diagram.md`

Expected output file `diagram.md`:
```markdown
# Payment State Machine

\`\`\`mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Authorized: authorize
    Pending --> Failed: authorize
    Authorized --> Captured: capture
    Authorized --> Failed: capture
    Pending --> Failed: fail
\`\`\`
```

**TC4.5 - Format Idempotence**

Command:
```bash
gust fmt test.gu
cp test.gu test1.gu
gust fmt test.gu
diff test.gu test1.gu
```

Expected: `diff` shows no differences (exit code 0)

**TC4.6 - Init Overwrite Protection**

Setup: Create directory `my-project/` with a file inside

Command:
```bash
mkdir my-project
echo "existing" > my-project/README.md
gust init my-project
```

Expected output:
```
Directory 'my-project' already exists. Overwrite? (y/N):
```

If user enters `n`: Exit without changes
If user enters `y`: Proceed with scaffolding

### Implementation Guide

**Step 1: Add Tooling Subcommands to CLI**

File: `D:\Projects\gust\gust-cli\src\main.rs` (update main)
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gust")]
#[command(about = "Gust state machine compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        file: String,
        #[arg(long, default_value = "rust")]
        target: String,
        #[arg(long)]
        package: Option<String>,
        #[arg(short, long)]
        output: Option<String>,
    },
    Init {
        name: String,
    },
    Fmt {
        file: String,
        #[arg(long)]
        check: bool,
    },
    Check {
        file: String,
    },
    Diagram {
        file: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    Watch {
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { file, target, package, output } => {
            build_command(&file, &target, package, output);
        }
        Commands::Init { name } => {
            init_command(&name);
        }
        Commands::Fmt { file, check } => {
            fmt_command(&file, check);
        }
        Commands::Check { file } => {
            check_command(&file);
        }
        Commands::Diagram { file, output } => {
            diagram_command(&file, output);
        }
        Commands::Watch { path } => {
            watch_command(&path);
        }
    }
}
```

**Step 2: Implement Init Command**

File: `D:\Projects\gust\gust-cli\src\commands\init.rs`
```rust
use std::fs;
use std::path::Path;
use std::io::{self, Write};

pub fn init_command(name: &str) {
    let project_path = Path::new(name);

    // Check if directory exists
    if project_path.exists() {
        print!("Directory '{}' already exists. Overwrite? (y/N): ", name);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return;
        }
    }

    // Create directory structure
    fs::create_dir_all(project_path.join("src")).expect("Failed to create src directory");

    // Write Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
gust-runtime = "0.2"

[build-dependencies]
gust-build = "0.2"
"#,
        name
    );
    fs::write(project_path.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

    // Write build.rs
    let build_rs = r#"fn main() {
    gust_build::compile_gust_files();
}
"#;
    fs::write(project_path.join("build.rs"), build_rs).expect("Failed to write build.rs");

    // Write src/main.rs
    let main_rs = r#"mod machine;

fn main() {
    println!("Gust machine generated successfully!");
}
"#;
    fs::write(project_path.join("src/main.rs"), main_rs).expect("Failed to write src/main.rs");

    // Write src/machine.gu
    let machine_gu = r#"machine Example {
    state Idle
    state Active(count: i64)
    state Done

    transition start: Idle -> Active
    transition finish: Active -> Done

    on start(ctx: Context) {
        goto Active(0);
    }

    on finish(ctx: Context) {
        goto Done();
    }
}
"#;
    fs::write(project_path.join("src/machine.gu"), machine_gu)
        .expect("Failed to write src/machine.gu");

    println!("Created Gust project '{}' successfully!", name);
    println!("\nNext steps:");
    println!("  cd {}", name);
    println!("  cargo build");
    println!("  cargo run");
}
```

**Step 3: Implement Format Command**

File: `D:\Projects\gust\gust-cli\src\commands\fmt.rs`
```rust
use gust_lang::{parse_program, format_program};
use std::fs;
use std::process;

pub fn fmt_command(file: &str, check: bool) {
    let source = fs::read_to_string(file).expect("Failed to read file");

    let program = match parse_program(&source) {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    let formatted = format_program(&program);

    if check {
        if source != formatted {
            eprintln!("File {} is not formatted", file);
            process::exit(1);
        } else {
            println!("File {} is already formatted", file);
        }
    } else {
        fs::write(file, formatted).expect("Failed to write formatted file");
        println!("Formatted {}", file);
    }
}
```

**Step 4: Implement Check Command**

File: `D:\Projects\gust\gust-cli\src\commands\check.rs`
```rust
use gust_lang::{parse_program, validate_program, ValidationContext, ErrorSeverity};
use std::fs;
use std::process;

pub fn check_command(file: &str) {
    let source = fs::read_to_string(file).expect("Failed to read file");

    let program = match parse_program(&source) {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    let mut ctx = ValidationContext::new(file.to_string(), source.clone());
    validate_program(&program, &mut ctx);

    for error in &ctx.errors {
        eprintln!("{}", error.format(&source));
    }

    if ctx.has_errors() {
        let error_count = ctx.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).count();
        let warning_count = ctx.errors.len() - error_count;
        eprintln!(
            "\nerror: check failed with {} error(s) and {} warning(s)",
            error_count, warning_count
        );
        process::exit(1);
    } else {
        println!("No errors found in {}", file);
    }
}
```

**Step 5: Implement Diagram Command**

File: `D:\Projects\gust\gust-cli\src\commands\diagram.rs`
```rust
use gust_lang::{parse_program, Program, Machine};
use std::fs;
use std::process;

pub fn diagram_command(file: &str, output: Option<String>) {
    let source = fs::read_to_string(file).expect("Failed to read file");

    let program = match parse_program(&source) {
        Ok(prog) => prog,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    let diagram = generate_mermaid(&program);

    match output {
        Some(output_file) => {
            fs::write(&output_file, &diagram).expect("Failed to write diagram file");
            println!("Diagram written to {}", output_file);
        }
        None => {
            println!("{}", diagram);
        }
    }
}

fn generate_mermaid(program: &Program) -> String {
    let mut output = String::new();

    for machine in &program.machines {
        output.push_str(&format!("# {} State Machine\n\n", machine.name));
        output.push_str("```mermaid\n");
        output.push_str("stateDiagram-v2\n");

        // Start state
        if let Some(first_state) = machine.states.first() {
            output.push_str(&format!("    [*] --> {}\n", first_state.name));
        }

        // Transitions
        for transition in &machine.transitions {
            for target in &transition.targets {
                output.push_str(&format!(
                    "    {} --> {}: {}\n",
                    transition.from, target, transition.name
                ));
            }
        }

        output.push_str("```\n");
    }

    output
}
```

**Step 6: Add Formatter to gust-lang**

File: `D:\Projects\gust\gust-lang\src\format.rs`
```rust
use crate::ast::*;

pub fn format_program(program: &Program) -> String {
    let mut output = String::new();

    for (i, use_decl) in program.uses.iter().enumerate() {
        output.push_str(&format!("use {};\n", use_decl.path));
        if i == program.uses.len() - 1 && !program.types.is_empty() {
            output.push('\n');
        }
    }

    for (i, type_decl) in program.types.iter().enumerate() {
        output.push_str(&format_type_decl(type_decl));
        if i < program.types.len() - 1 {
            output.push('\n');
        }
    }

    if !program.types.is_empty() && !program.machines.is_empty() {
        output.push('\n');
    }

    for machine in &program.machines {
        output.push_str(&format_machine(machine));
    }

    output
}

fn format_type_decl(type_decl: &TypeDecl) -> String {
    match type_decl {
        TypeDecl::Struct { name, fields } => {
            let mut s = format!("type {} {{\n", name);
            for field in fields {
                s.push_str(&format!("    {}: {:?},\n", field.name, field.ty));
            }
            s.push_str("}\n");
            s
        }
        TypeDecl::Enum { name, variants } => {
            let mut s = format!("enum {} {{\n", name);
            for variant in variants {
                s.push_str(&format!("    {},\n", variant.name));
            }
            s.push_str("}\n");
            s
        }
    }
}

fn format_machine(machine: &Machine) -> String {
    let mut output = format!("machine {} {{\n", machine.name);

    // States
    for state in &machine.states {
        if state.fields.is_empty() {
            output.push_str(&format!("    state {}\n", state.name));
        } else {
            output.push_str(&format!("    state {}(", state.name));
            for (i, field) in state.fields.iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(&format!("{}: {:?}", field.name, field.ty));
            }
            output.push_str(")\n");
        }
    }

    output.push('\n');

    // Transitions
    for transition in &machine.transitions {
        output.push_str(&format!(
            "    transition {}: {} -> {}\n",
            transition.name,
            transition.from,
            transition.targets.join(" | ")
        ));
    }

    output.push('\n');

    // Effects
    for effect in &machine.effects {
        let async_kw = if effect.is_async { "async " } else { "" };
        output.push_str(&format!("    {}effect {}(", async_kw, effect.name));
        for (i, param) in effect.params.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push_str(&format!("{}: {:?}", param.name, param.ty));
        }
        output.push_str(&format!(") -> {:?}\n", effect.return_type));
    }

    if !machine.effects.is_empty() {
        output.push('\n');
    }

    // Handlers (simplified - full impl would format statements)
    for handler in &machine.handlers {
        let async_kw = if handler.is_async { "async " } else { "" };
        output.push_str(&format!("    {}on {}(", async_kw, handler.transition));
        for (i, param) in handler.params.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push_str(&format!("{}: {:?}", param.name, param.ty));
        }
        output.push_str(") {\n");
        // TODO: Format statements properly
        output.push_str("        // handler body\n");
        output.push_str("    }\n");
    }

    output.push_str("}\n");
    output
}
```

---

## Constraints

1. **Phase 1 Dependency**: Phase 2 cannot begin until Phase 1 is fully implemented and tested. All async, type system, and build.rs features must be working.

2. **LSP Performance**: The language server must respond to requests within 100ms for files under 1000 lines. Larger files should not block the editor UI.

3. **Error Message Clarity**: All error messages must be understandable by developers unfamiliar with Gust internals. Avoid exposing parser implementation details.

4. **Format Preservation**: `gust fmt` must preserve comments in their original locations (above declarations, inline with code).

5. **Backward Compatibility**: Changes to error reporting and validation must not break existing .gu files that are syntactically valid.

6. **Extension Distribution**: VS Code extension must be packageable as .vsix and publishable to VS Code marketplace without requiring manual setup.

7. **Cross-Platform**: All tooling commands (init, fmt, check, diagram) must work on Windows, macOS, and Linux.

8. **Color Output**: Colored error output must respect NO_COLOR environment variable and terminal capability detection (disable colors if not supported).

9. **Idempotence**: `gust fmt` must be idempotent - formatting a file twice produces identical output.

10. **Validation Coverage**: Validation pass must catch common errors (undefined states, unreachable states, unused effects) but avoid false positives.

---

## Verification Checklist

Before marking Phase 2 complete, verify:

### Feature 1: VS Code Extension
- [ ] Extension installs via .vsix without errors
- [ ] .gu files show syntax highlighting for all keywords, types, strings, comments
- [ ] File icon appears for .gu files in explorer
- [ ] Snippets expand on Tab for machine, state, transition, effect, on, enum, type
- [ ] File nesting works (`.g.rs` and `.g.go` nest under `.gu`)
- [ ] Extension works on Windows, macOS, Linux

### Feature 2: LSP
- [ ] LSP server compiles and runs without errors
- [ ] Syntax errors appear in Problems panel with correct line/col
- [ ] Go-to-definition works for states and effects (Ctrl+Click)
- [ ] Hover shows info for states (fields) and effects (signature)
- [ ] Autocomplete suggests states after `goto ` and effects after `perform `
- [ ] LSP responds within 100ms for typical .gu files
- [ ] Extension launches LSP server automatically

### Feature 3: Error Messages
- [ ] Parse errors show file:line:col, message, and source snippet with caret
- [ ] Validation errors show for undefined states, unreachable states, unused effects
- [ ] "Did you mean?" suggestions appear for similar names (Levenshtein distance ≤ 2)
- [ ] Errors display in color (red for errors, yellow for warnings, cyan for notes)
- [ ] Colors disabled when NO_COLOR env var is set
- [ ] Multiple errors displayed together (not just first error)

### Feature 4: Tooling
- [ ] `gust init my-project` creates valid Cargo project with .gu file
- [ ] `cargo build` in initialized project succeeds
- [ ] `gust fmt file.gu` formats file with canonical style
- [ ] `gust fmt` is idempotent (running twice produces same output)
- [ ] `gust check file.gu` validates without codegen
- [ ] `gust diagram file.gu -o out.md` generates valid Mermaid diagram
- [ ] `gust init` asks before overwriting existing directory
- [ ] All commands work on Windows, macOS, Linux

### Integration Tests
- [ ] Full workflow: init → edit .gu → check → build → run
- [ ] LSP + extension: open .gu → see diagnostics → hover → autocomplete → format
- [ ] Error recovery: broken .gu → see errors → fix → errors disappear
- [ ] Large file handling: 500+ line .gu file parses and formats in <500ms

---

## File Map

### New Files to Create

```
D:\Projects\gust\
├── editors\vscode\
│   ├── package.json
│   ├── README.md
│   ├── LICENSE
│   ├── .vscodeignore
│   ├── tsconfig.json
│   ├── language-configuration.json
│   ├── images\
│   │   └── icon.png
│   ├── syntaxes\
│   │   └── gust.tmLanguage.json
│   ├── snippets\
│   │   └── gust.json
│   └── src\
│       └── extension.ts
├── gust-lsp\
│   ├── Cargo.toml
│   └── src\
│       └── main.rs
└── gust-lang\src\
    ├── error.rs
    ├── validate.rs
    └── format.rs
```

### Files to Modify

```
D:\Projects\gust\
├── gust-lang\
│   ├── Cargo.toml              # Add strsim, colored dependencies
│   ├── src\
│   │   ├── lib.rs              # Export error, validate, format modules
│   │   ├── ast.rs              # Add line/col fields to State, Transition, Effect
│   │   └── parser.rs           # Capture line/col from pest spans
├── gust-cli\
│   ├── Cargo.toml              # Add colored dependency
│   └── src\
│       ├── main.rs             # Add init, fmt, check, diagram subcommands
│       └── commands\
│           ├── init.rs         # New module
│           ├── fmt.rs          # New module
│           ├── check.rs        # New module
│           └── diagram.rs      # New module
└── Cargo.toml                  # Add gust-lsp to workspace members
```

---

**END OF SPEC**