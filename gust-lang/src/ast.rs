/// Abstract Syntax Tree for the Gust language
/// Every .gu file parses into a `Program` containing types and machines.

#[derive(Debug, Clone)]
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub channels: Vec<ChannelDecl>,
    pub machines: Vec<MachineDecl>,
}

#[derive(Debug, Clone)]
pub struct UsePath {
    pub segments: Vec<String>,
}

// === Type Declarations ===

#[derive(Debug, Clone)]
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
            TypeDecl::Struct { name, .. } => name,
            TypeDecl::Enum { name, .. } => name,
        }
    }

    pub fn fields(&self) -> &[Field] {
        match self {
            TypeDecl::Struct { fields, .. } => fields,
            TypeDecl::Enum { .. } => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Vec<TypeExpr>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Simple(String),
    Generic(String, Vec<TypeExpr>),
    Tuple(Vec<TypeExpr>),
}

// === Machine Declarations ===

#[derive(Debug, Clone)]
pub struct MachineDecl {
    pub name: String,
    pub generic_params: Vec<GenericParam>,
    pub sends: Vec<String>,
    pub receives: Vec<String>,
    pub supervises: Vec<SupervisionSpec>,
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StateDecl {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct TransitionDecl {
    pub name: String,
    pub from: String,
    pub targets: Vec<String>, // e.g., Validated | Failed
    pub timeout: Option<DurationSpec>,
}

#[derive(Debug, Clone)]
pub struct EffectDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub transition_name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
}

// === Statements & Expressions ===

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    Return(Expr),
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    Goto {
        state: String,
        args: Vec<Expr>,
    },
    Perform {
        effect: String,
        args: Vec<Expr>,
    },
    Send {
        channel: String,
        message: Expr,
    },
    Spawn {
        machine: String,
        args: Vec<Expr>,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Variant {
        enum_name: Option<String>,
        variant: String,
        bindings: Vec<String>,
    },
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone)]
pub struct ChannelDecl {
    pub name: String,
    pub message_type: TypeExpr,
    pub capacity: Option<i64>,
    pub mode: ChannelMode,
}

#[derive(Debug, Clone, Copy)]
pub enum ChannelMode {
    Broadcast,
    Mpsc,
}

#[derive(Debug, Clone)]
pub struct SupervisionSpec {
    pub child_machine: String,
    pub strategy: SupervisionStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

#[derive(Debug, Clone, Copy)]
pub struct DurationSpec {
    pub value: i64,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    Millis,
    Seconds,
    Minutes,
    Hours,
}
