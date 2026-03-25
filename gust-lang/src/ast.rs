//! Abstract Syntax Tree (AST) for the Gust language.
//!
//! Every `.gu` source file is parsed into a [`Program`], the top-level node
//! containing use-paths, type declarations, channel declarations, and machine
//! declarations. The AST is produced by [`crate::parser::parse_program`] and
//! consumed by the validator and code generators.

/// Root AST node representing an entire `.gu` source file.
///
/// A program is composed of four kinds of top-level declarations that appear
/// in any order: imports (`use`), types (`type` / `enum`), channels, and
/// machines.
#[derive(Debug, Clone)]
pub struct Program {
    /// Import paths (e.g. `use std::collections::HashMap`).
    pub uses: Vec<UsePath>,
    /// Struct and enum type declarations defined outside machines.
    pub types: Vec<TypeDecl>,
    /// Channel declarations for inter-machine communication.
    pub channels: Vec<ChannelDecl>,
    /// State machine declarations -- the core abstraction in Gust.
    pub machines: Vec<MachineDecl>,
}

/// A `use` import path (e.g. `use std::collections::HashMap`).
///
/// Segments are the individual components split by `::`.
#[derive(Debug, Clone)]
pub struct UsePath {
    /// Path segments (e.g. `["std", "collections", "HashMap"]`).
    pub segments: Vec<String>,
}

// === Type Declarations ===

/// A top-level type declaration -- either a struct or an enum.
///
/// In Gust syntax:
/// ```text
/// type Config { retries: i64, timeout: i64 }
/// enum Status { Ok, Error(String) }
/// ```
#[derive(Debug, Clone)]
pub enum TypeDecl {
    /// A product type with named fields: `type Name { field: Type, ... }`.
    Struct {
        /// The type name (PascalCase by convention).
        name: String,
        /// Named fields with their types.
        fields: Vec<Field>,
    },
    /// A sum type with variants: `enum Name { A, B(Type), ... }`.
    Enum {
        /// The enum name (PascalCase by convention).
        name: String,
        /// Enum variants, each optionally carrying payload types.
        variants: Vec<EnumVariant>,
    },
}

impl TypeDecl {
    /// Returns the declared name of this type.
    pub fn name(&self) -> &str {
        match self {
            TypeDecl::Struct { name, .. } => name,
            TypeDecl::Enum { name, .. } => name,
        }
    }

    /// Returns the fields for a struct variant, or an empty slice for enums.
    pub fn fields(&self) -> &[Field] {
        match self {
            TypeDecl::Struct { fields, .. } => fields,
            TypeDecl::Enum { .. } => &[],
        }
    }
}

/// A single variant of a Gust `enum` declaration.
///
/// Variants may carry zero or more positional payload types (e.g. `Error(String)`).
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// Variant name (PascalCase by convention).
    pub name: String,
    /// Positional payload types (empty for unit variants like `Ok`).
    pub payload: Vec<TypeExpr>,
}

/// A named field in a struct or state declaration.
#[derive(Debug, Clone)]
pub struct Field {
    /// Field name (snake_case by convention).
    pub name: String,
    /// The field's type expression.
    pub ty: TypeExpr,
}

/// A type expression appearing in field declarations, parameters, or return types.
///
/// Covers the type syntax supported by Gust: unit `()`, simple names, generics,
/// and tuples.
#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// The unit type `()`.
    Unit,
    /// A simple named type (e.g. `String`, `i64`, `Config`).
    Simple(String),
    /// A generic type (e.g. `Vec<String>`, `Option<i64>`).
    Generic(String, Vec<TypeExpr>),
    /// A tuple type (e.g. `(i64, String)`).
    Tuple(Vec<TypeExpr>),
}

// === Machine Declarations ===

/// A state machine declaration -- the primary abstraction in Gust.
///
/// Machines contain states, transitions between states, handler
/// implementations for each transition, and optional effect declarations
/// for side-effect injection.
///
/// ```text
/// machine OrderProcessor {
///     state Pending(order_id: String)
///     state Confirmed
///     transition confirm: Pending -> Confirmed
///     effect save_order(id: String) -> bool
///     on confirm(ctx: Ctx) { goto Confirmed; }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MachineDecl {
    /// Machine name (PascalCase by convention).
    pub name: String,
    /// Generic type parameters (e.g. `<T: Send>`).
    pub generic_params: Vec<GenericParam>,
    /// Channel names this machine sends to.
    pub sends: Vec<String>,
    /// Channel names this machine receives from.
    pub receives: Vec<String>,
    /// Child machines supervised by this machine.
    pub supervises: Vec<SupervisionSpec>,
    /// State declarations (the first state is the initial state).
    pub states: Vec<StateDecl>,
    /// Transition declarations defining valid state changes.
    pub transitions: Vec<TransitionDecl>,
    /// Handler implementations for transitions.
    pub handlers: Vec<OnHandler>,
    /// Effect declarations for external side effects.
    pub effects: Vec<EffectDecl>,
}

/// A generic type parameter with optional trait bounds.
///
/// Corresponds to `<T: Bound1 + Bound2>` in machine declarations.
#[derive(Debug, Clone)]
pub struct GenericParam {
    /// Parameter name (e.g. `T`).
    pub name: String,
    /// Trait bounds (e.g. `["Send", "Sync"]`).
    pub bounds: Vec<String>,
}

/// A state declaration within a machine.
///
/// States may carry named fields that hold data while the machine is in
/// that state. The first declared state is the machine's initial state.
///
/// ```text
/// state Pending(order_id: String, retries: i64)
/// state Complete
/// ```
#[derive(Debug, Clone)]
pub struct StateDecl {
    /// State name (PascalCase by convention).
    pub name: String,
    /// Fields carried by this state (empty for unit states).
    pub fields: Vec<Field>,
}

/// A transition declaration defining a valid state change.
///
/// Transitions name an event, specify a source state, and list one or more
/// possible target states (for branching transitions). An optional timeout
/// can trigger automatic transition after a duration.
///
/// ```text
/// transition validate: Pending -> Validated | Failed timeout 30s
/// ```
#[derive(Debug, Clone)]
pub struct TransitionDecl {
    /// Transition name (snake_case by convention).
    pub name: String,
    /// Source state name.
    pub from: String,
    /// Target state names (e.g. `["Validated", "Failed"]`).
    pub targets: Vec<String>,
    /// Optional timeout duration that auto-triggers this transition.
    pub timeout: Option<DurationSpec>,
}

/// An effect declaration for external side effects.
///
/// Effects define an interface for I/O or other impure operations that the
/// machine needs but does not implement. Code generators produce a trait
/// (Rust) or interface (Go) from these declarations.
///
/// ```text
/// effect save_order(id: String) -> bool
/// async effect fetch_price(symbol: String) -> f64
/// ```
#[derive(Debug, Clone)]
pub struct EffectDecl {
    /// Effect name (snake_case by convention).
    pub name: String,
    /// Parameters the effect accepts.
    pub params: Vec<Field>,
    /// Return type of the effect.
    pub return_type: TypeExpr,
    /// Whether the effect is async (requires `.await` in generated code).
    pub is_async: bool,
}

/// A handler implementation for a transition.
///
/// Handlers contain the logic that runs when a transition is triggered.
/// They receive parameters, can access the current state via `ctx`, perform
/// effects, and must end with a `goto` to transition to a new state.
///
/// ```text
/// on validate(ctx: Ctx, input: String) {
///     if input != "" { goto Validated; }
///     else { goto Failed; }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OnHandler {
    /// Name of the transition this handler implements.
    pub transition_name: String,
    /// Handler parameters (first is typically the context parameter).
    pub params: Vec<Param>,
    /// Optional return type (currently unsupported by codegen).
    pub return_type: Option<TypeExpr>,
    /// The handler body containing statements.
    pub body: Block,
    /// Whether this handler is async.
    pub is_async: bool,
}

/// A named parameter with a type annotation.
#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub ty: TypeExpr,
}

// === Statements & Expressions ===

/// A block of statements (the body of a handler, if-branch, or match arm).
#[derive(Debug, Clone)]
pub struct Block {
    /// Ordered list of statements in this block.
    pub statements: Vec<Statement>,
}

/// A statement within a handler body or block.
#[derive(Debug, Clone)]
pub enum Statement {
    /// Variable binding: `let name: Type = value;` or `let name = value;`.
    Let {
        /// Variable name.
        name: String,
        /// Optional explicit type annotation.
        ty: Option<TypeExpr>,
        /// Initializer expression.
        value: Expr,
    },
    /// Return statement: `return expr;` (currently rejected by the validator).
    Return(Expr),
    /// Conditional: `if cond { ... } else { ... }`.
    If {
        /// Condition expression.
        condition: Expr,
        /// Block executed when the condition is true.
        then_block: Block,
        /// Optional block executed when the condition is false.
        else_block: Option<Block>,
    },
    /// State transition: `goto StateName(arg1, arg2);`.
    ///
    /// Arguments are positionally zipped with the target state's declared fields.
    Goto {
        /// Target state name.
        state: String,
        /// Arguments to pass as the target state's field values.
        args: Vec<Expr>,
    },
    /// Effect invocation as a statement: `perform effect_name(args);`.
    Perform {
        /// Effect name.
        effect: String,
        /// Arguments to the effect.
        args: Vec<Expr>,
    },
    /// Channel send: `send channel_name(message);`.
    Send {
        /// Target channel name.
        channel: String,
        /// Message expression to send.
        message: Expr,
    },
    /// Spawn a child machine: `spawn MachineName(args);`.
    Spawn {
        /// Machine name to spawn.
        machine: String,
        /// Arguments for the spawned machine's initial state.
        args: Vec<Expr>,
    },
    /// Pattern match: `match expr { pattern => { ... } }`.
    Match {
        /// Expression being matched.
        scrutinee: Expr,
        /// Match arms with patterns and bodies.
        arms: Vec<MatchArm>,
    },
    /// A bare expression used as a statement.
    Expr(Expr),
}

/// A single arm in a `match` expression.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// The pattern to match against.
    pub pattern: Pattern,
    /// The block to execute when the pattern matches.
    pub body: Block,
}

/// A pattern in a `match` arm.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard pattern `_` that matches anything.
    Wildcard,
    /// Simple identifier pattern (binds or matches a variable).
    Ident(String),
    /// Enum variant pattern: `EnumName::Variant(binding1, binding2)`.
    Variant {
        /// Optional qualifying enum name (e.g. `Status` in `Status::Ok`).
        enum_name: Option<String>,
        /// Variant name.
        variant: String,
        /// Variable names bound from the variant's payload.
        bindings: Vec<String>,
    },
}

/// An expression node in the AST.
///
/// `Perform` is both an expression and a statement in Gust, allowing
/// `let x = perform effect(args);` to capture effect return values.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal (e.g. `42`).
    IntLit(i64),
    /// Floating-point literal (e.g. `3.14`).
    FloatLit(f64),
    /// String literal (e.g. `"hello"`).
    StringLit(String),
    /// Boolean literal (`true` or `false`).
    BoolLit(bool),
    /// Identifier reference (e.g. `count`, `ctx`).
    Ident(String),
    /// Field access (e.g. `ctx.count`, `order.id`).
    FieldAccess(Box<Expr>, String),
    /// Function call (e.g. `to_string(value)`).
    FnCall(String, Vec<Expr>),
    /// Binary operation (e.g. `a + b`, `x == y`).
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    /// Unary operation (e.g. `!flag`, `-value`).
    UnaryOp(UnaryOp, Box<Expr>),
    /// Effect invocation as an expression: `perform effect_name(args)`.
    Perform(String, Vec<Expr>),
    /// Qualified enum path: `Enum::Variant`.
    Path(String, String),
}

/// Binary operators supported in Gust expressions.
#[derive(Debug, Clone)]
pub enum BinOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Modulo (`%`).
    Mod,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Neq,
    /// Less than (`<`).
    Lt,
    /// Less than or equal (`<=`).
    Lte,
    /// Greater than (`>`).
    Gt,
    /// Greater than or equal (`>=`).
    Gte,
    /// Logical AND (`&&`).
    And,
    /// Logical OR (`||`).
    Or,
}

/// Unary operators supported in Gust expressions.
#[derive(Debug, Clone)]
pub enum UnaryOp {
    /// Logical negation (`!`).
    Not,
    /// Arithmetic negation (`-`).
    Neg,
}

/// A channel declaration for inter-machine communication.
///
/// ```text
/// channel events: Event { capacity: 1024, mode: broadcast }
/// ```
#[derive(Debug, Clone)]
pub struct ChannelDecl {
    /// Channel name.
    pub name: String,
    /// Type of messages carried by this channel.
    pub message_type: TypeExpr,
    /// Optional buffer capacity (defaults to implementation-defined).
    pub capacity: Option<i64>,
    /// Communication mode (broadcast or mpsc).
    pub mode: ChannelMode,
}

/// The communication mode for a channel.
#[derive(Debug, Clone, Copy)]
pub enum ChannelMode {
    /// Broadcast: every subscriber receives every message.
    Broadcast,
    /// Multi-producer, single-consumer.
    Mpsc,
}

/// Specification for a supervised child machine.
#[derive(Debug, Clone)]
pub struct SupervisionSpec {
    /// Name of the child machine being supervised.
    pub child_machine: String,
    /// Restart strategy applied when the child fails.
    pub strategy: SupervisionStrategy,
}

/// Erlang/OTP-inspired supervision restart strategies.
#[derive(Debug, Clone, Copy)]
pub enum SupervisionStrategy {
    /// Restart only the failed child.
    OneForOne,
    /// Restart all children when one fails.
    OneForAll,
    /// Restart the failed child and all children started after it.
    RestForOne,
}

/// A duration specification for transition timeouts.
///
/// Represents a value with a time unit (e.g. `30s`, `500ms`).
#[derive(Debug, Clone, Copy)]
pub struct DurationSpec {
    /// Numeric value of the duration.
    pub value: i64,
    /// Time unit (milliseconds, seconds, minutes, or hours).
    pub unit: TimeUnit,
}

/// Time units supported in duration specifications.
#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    /// Milliseconds (`ms`).
    Millis,
    /// Seconds (`s`).
    Seconds,
    /// Minutes (`m`).
    Minutes,
    /// Hours (`h`).
    Hours,
}
