/// Abstract Syntax Tree for the Gust language
/// Every .gu file parses into a `Program` containing types and machines.

#[derive(Debug, Clone)]
pub struct Program {
    pub uses: Vec<UsePath>,
    pub types: Vec<TypeDecl>,
    pub machines: Vec<MachineDecl>,
}

#[derive(Debug, Clone)]
pub struct UsePath {
    pub segments: Vec<String>,
}

// === Type Declarations ===

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<Field>,
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
}

// === Machine Declarations ===

#[derive(Debug, Clone)]
pub struct MachineDecl {
    pub name: String,
    pub states: Vec<StateDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub handlers: Vec<OnHandler>,
    pub effects: Vec<EffectDecl>,
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
}

#[derive(Debug, Clone)]
pub struct EffectDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub return_type: TypeExpr,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub transition_name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
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
    Expr(Expr),
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
