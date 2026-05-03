//! Abstract Syntax Tree for the Gust language.
//! Every .gu file parses into a `Program` containing types and machines.

/// Source location captured from the parser. All values are 1-based.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    /// 1-based line where the node starts.
    pub start_line: usize,
    /// 1-based column where the node starts.
    pub start_col: usize,
    /// 1-based line where the node ends (inclusive).
    pub end_line: usize,
    /// 1-based column where the node ends (exclusive).
    pub end_col: usize,
}

/// Top-level parsed Gust program — the root of the AST.
#[derive(Debug, Clone)]
pub struct Program {
    /// `use foo::bar;` imports in declaration order.
    pub uses: Vec<UsePath>,
    /// Top-level `type` and `enum` declarations.
    pub types: Vec<TypeDecl>,
    /// Top-level `channel` declarations.
    pub channels: Vec<ChannelDecl>,
    /// Top-level `machine` declarations.
    pub machines: Vec<MachineDecl>,
}

/// A dotted-path import specifier, e.g. `use std::EngineFailure;`.
#[derive(Debug, Clone)]
pub struct UsePath {
    /// Path segments in order (e.g. `["std", "EngineFailure"]`).
    pub segments: Vec<String>,
    /// Source span of the full `use` declaration.
    pub span: Span,
}

// === Type Declarations ===

/// A top-level user-defined type (`type` struct or `enum`).
#[derive(Debug, Clone)]
pub enum TypeDecl {
    /// A record type declared with the `type` keyword.
    Struct {
        /// Type name.
        name: String,
        /// Ordered list of fields.
        fields: Vec<Field>,
        /// Source span of the declaration.
        span: Span,
    },
    /// A sum type declared with the `enum` keyword.
    Enum {
        /// Enum type name.
        name: String,
        /// Variants in declaration order.
        variants: Vec<EnumVariant>,
        /// Source span of the declaration.
        span: Span,
    },
}

impl TypeDecl {
    /// Name of the declared type.
    pub fn name(&self) -> &str {
        match self {
            TypeDecl::Struct { name, .. } => name,
            TypeDecl::Enum { name, .. } => name,
        }
    }

    /// Fields of a struct declaration, or `&[]` for enums.
    pub fn fields(&self) -> &[Field] {
        match self {
            TypeDecl::Struct { fields, .. } => fields,
            TypeDecl::Enum { .. } => &[],
        }
    }

    /// Source span of the declaration.
    pub fn span(&self) -> Span {
        match self {
            TypeDecl::Struct { span, .. } => *span,
            TypeDecl::Enum { span, .. } => *span,
        }
    }
}

/// An enum variant with optional positional payload types.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// Variant name.
    pub name: String,
    /// Positional payload types. Empty for unit variants.
    pub payload: Vec<TypeExpr>,
}

/// A named field in a struct or state definition.
#[derive(Debug, Clone)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Field type expression.
    pub ty: TypeExpr,
}

/// A type expression that can appear in field, parameter, or return
/// positions.
#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// The unit type `()`.
    Unit,
    /// A bare identifier type (primitive, user-defined, or generic param).
    Simple(String),
    /// A generic application like `Vec<T>` or `Result<T, E>`.
    Generic(String, Vec<TypeExpr>),
    /// A tuple type like `(i64, String)`.
    Tuple(Vec<TypeExpr>),
}

// === Machine Declarations ===

/// A `machine` declaration — the central unit of a Gust program.
#[derive(Debug, Clone)]
pub struct MachineDecl {
    /// Machine name.
    pub name: String,
    /// Generic type parameters (e.g. `<T: Clone>`).
    pub generic_params: Vec<GenericParam>,
    /// Channel names this machine produces messages to.
    pub sends: Vec<String>,
    /// Channel names this machine consumes messages from.
    pub receives: Vec<String>,
    /// Child machines this machine supervises.
    pub supervises: Vec<SupervisionSpec>,
    /// State declarations in declaration order.
    pub states: Vec<StateDecl>,
    /// Transition declarations in declaration order.
    pub transitions: Vec<TransitionDecl>,
    /// Handler bodies (`on <transition>` blocks).
    pub handlers: Vec<OnHandler>,
    /// Declared effects and actions attached to this machine.
    pub effects: Vec<EffectDecl>,
    /// Source span of the machine declaration.
    pub span: Span,
}

/// A generic type parameter on a machine or type.
#[derive(Debug, Clone)]
pub struct GenericParam {
    /// Parameter name (e.g. `T`).
    pub name: String,
    /// Trait bounds (e.g. `["Clone", "Send"]`).
    pub bounds: Vec<String>,
}

/// A `state` declaration inside a machine.
#[derive(Debug, Clone)]
pub struct StateDecl {
    /// State name.
    pub name: String,
    /// State-carried fields (empty for unit-only states).
    pub fields: Vec<Field>,
    /// Source span of the state declaration.
    pub span: Span,
}

/// A `transition` declaration inside a machine.
#[derive(Debug, Clone)]
pub struct TransitionDecl {
    /// Transition name.
    pub name: String,
    /// Name of the originating state.
    pub from: String,
    /// Possible target state names (e.g. `Validated | Failed`).
    pub targets: Vec<String>,
    /// Optional `timeout <duration>` clause.
    pub timeout: Option<DurationSpec>,
    /// Source span of the transition declaration.
    pub span: Span,
}

/// Classifies a side-effectful declaration as `effect` (replay-safe /
/// idempotent) or `action` (not idempotent, externally visible).
///
/// Both share the same syntactic shape and codegen lowering in v0.1;
/// replay-aware runtimes and workflow tooling use the distinction to
/// decide retry and checkpoint semantics. See issue #40.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// `effect` — replay-safe / idempotent side effect.
    Effect,
    /// `action` — not replay-safe, externally visible side effect.
    Action,
}

impl EffectKind {
    /// The source keyword that introduced this declaration.
    pub fn keyword(self) -> &'static str {
        match self {
            EffectKind::Effect => "effect",
            EffectKind::Action => "action",
        }
    }

    /// Stable generated-code annotation text for downstream tooling.
    pub fn annotation_description(self) -> &'static str {
        match self {
            EffectKind::Effect => "replay-safe / idempotent",
            EffectKind::Action => "not replay-safe / externally visible",
        }
    }
}

/// An `effect` or `action` declaration inside a machine.
#[derive(Debug, Clone)]
pub struct EffectDecl {
    /// Effect/action name.
    pub name: String,
    /// Parameter list.
    pub params: Vec<Field>,
    /// Return type (use [`TypeExpr::Unit`] for no return).
    pub return_type: TypeExpr,
    /// True if the declaration used `async`.
    pub is_async: bool,
    /// Distinguishes `effect` (replay-safe) from `action` (not replay-safe).
    /// See [`EffectKind`] and issue #40.
    pub kind: EffectKind,
    /// Source span of the declaration.
    pub span: Span,
}

/// An `on <transition>` handler block.
#[derive(Debug, Clone)]
pub struct OnHandler {
    /// Name of the transition this handler implements.
    pub transition_name: String,
    /// Handler parameters (e.g. `ctx: FromState`).
    pub params: Vec<Param>,
    /// Optional explicit return type.
    pub return_type: Option<TypeExpr>,
    /// Handler body.
    pub body: Block,
    /// True if the handler is declared `async`.
    pub is_async: bool,
    /// Source span of the handler.
    pub span: Span,
}

/// A handler or match-arm parameter (`name: Type`).
#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter binding name.
    pub name: String,
    /// Parameter type.
    pub ty: TypeExpr,
}

// === Statements & Expressions ===

/// A sequence of statements forming a handler body, branch, or match arm.
#[derive(Debug, Clone)]
pub struct Block {
    /// Statements in source order.
    pub statements: Vec<Statement>,
}

/// A statement inside a handler body or branch.
#[derive(Debug, Clone)]
pub enum Statement {
    /// `let name[: Ty] = value;`
    Let {
        /// Binding name.
        name: String,
        /// Optional type annotation.
        ty: Option<TypeExpr>,
        /// Initializer expression.
        value: Expr,
    },
    /// `return expr;`
    Return(Expr),
    /// `if condition { then_block } [else else_block]`
    If {
        /// Condition expression.
        condition: Expr,
        /// Body of the `then` branch.
        then_block: Block,
        /// Body of the `else` branch, if present.
        else_block: Option<Block>,
        /// Source span of the `if` keyword through the end of the
        /// trailing block.
        span: Span,
    },
    /// `goto State(args...);`
    Goto {
        /// Target state name.
        state: String,
        /// Positional arguments to populate the target state's fields.
        args: Vec<Expr>,
        /// Source span of the `goto` statement.
        span: Span,
    },
    /// `perform effect(args...);` as a statement (discarding the return).
    Perform {
        /// Effect/action name.
        effect: String,
        /// Positional arguments.
        args: Vec<Expr>,
        /// Source span of the `perform` call.
        span: Span,
    },
    /// `send channel(message);`
    Send {
        /// Target channel name.
        channel: String,
        /// Message expression.
        message: Expr,
        /// Source span of the `send` statement.
        span: Span,
    },
    /// `spawn Machine(args...);`
    Spawn {
        /// Machine name to spawn as a child.
        machine: String,
        /// Constructor arguments.
        args: Vec<Expr>,
        /// Source span of the `spawn` statement.
        span: Span,
    },
    /// `match scrutinee { arm* }`
    Match {
        /// Expression being matched against.
        scrutinee: Expr,
        /// Match arms in source order.
        arms: Vec<MatchArm>,
    },
    /// An expression evaluated for its side effects.
    Expr(Expr),
}

/// A single arm of a `match` statement.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// Pattern to match.
    pub pattern: Pattern,
    /// Body evaluated when the pattern matches.
    pub body: Block,
}

/// A pattern in a `match` arm.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard pattern `_`.
    Wildcard,
    /// Plain identifier pattern (binds the scrutinee).
    Ident(String),
    /// Enum-variant pattern, optionally qualified with the enum name.
    Variant {
        /// Optional enum name qualifier (e.g. `Color::Red` has `enum_name = Some("Color")`).
        enum_name: Option<String>,
        /// Variant name.
        variant: String,
        /// Positional binding names for the variant payload.
        bindings: Vec<String>,
    },
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal.
    IntLit(i64),
    /// Floating-point literal.
    FloatLit(f64),
    /// String literal.
    StringLit(String),
    /// Boolean literal.
    BoolLit(bool),
    /// Identifier reference.
    Ident(String),
    /// Field-access expression `expr.field`.
    FieldAccess(Box<Expr>, String),
    /// Function or constructor call `name(args...)`.
    FnCall(String, Vec<Expr>),
    /// Binary operator expression `lhs op rhs`.
    BinOp(Box<Expr>, BinOp, Box<Expr>, Span),
    /// Unary operator expression `op operand`.
    UnaryOp(UnaryOp, Box<Expr>),
    /// Perform expression `perform effect(args...)`. Allowed in both
    /// statement and expression positions. The trailing [`Span`] records
    /// the source location of the `perform` keyword through the closing
    /// parenthesis so diagnostics can point at the call site.
    Perform(String, Vec<Expr>, Span),
    /// Qualified enum path `Enum::Variant`.
    Path(String, String),
}

/// A binary arithmetic, comparison, or logical operator.
#[derive(Debug, Clone)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `<=`
    Lte,
    /// `>`
    Gt,
    /// `>=`
    Gte,
    /// `&&`
    And,
    /// `||`
    Or,
}

/// A unary operator.
#[derive(Debug, Clone)]
pub enum UnaryOp {
    /// Logical not `!`.
    Not,
    /// Arithmetic negation `-`.
    Neg,
}

/// A `channel` declaration.
#[derive(Debug, Clone)]
pub struct ChannelDecl {
    /// Channel name.
    pub name: String,
    /// Message type carried by the channel.
    pub message_type: TypeExpr,
    /// Optional bounded capacity.
    pub capacity: Option<i64>,
    /// Delivery mode (broadcast vs MPSC).
    pub mode: ChannelMode,
    /// Source span of the channel declaration.
    pub span: Span,
}

/// Channel delivery mode.
#[derive(Debug, Clone, Copy)]
pub enum ChannelMode {
    /// Every subscriber receives every message.
    Broadcast,
    /// Exactly one consumer receives each message (multi-producer, single-consumer).
    Mpsc,
}

/// A `supervises` child-machine specification.
#[derive(Debug, Clone)]
pub struct SupervisionSpec {
    /// Name of the supervised child machine.
    pub child_machine: String,
    /// Restart strategy to apply on failure.
    pub strategy: SupervisionStrategy,
}

/// Restart strategy for supervised children (mirrors Erlang/OTP semantics).
#[derive(Debug, Clone, Copy)]
pub enum SupervisionStrategy {
    /// Restart only the failing child.
    OneForOne,
    /// Restart all children when any fails.
    OneForAll,
    /// Restart the failing child and all children started after it.
    RestForOne,
}

/// A duration literal like `5s` or `250ms`.
#[derive(Debug, Clone, Copy)]
pub struct DurationSpec {
    /// Numeric magnitude.
    pub value: i64,
    /// Unit of time.
    pub unit: TimeUnit,
}

/// Time units recognised by the parser.
#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    /// Milliseconds.
    Millis,
    /// Seconds.
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
}
