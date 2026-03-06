use std::{
    collections::{BTreeMap, HashSet},
    fmt::Display,
};

use anyhow::Result;
use logos::Logos;

use super::parser_errors::{
    ErrorLevel, ParseError, ParseErrorType, SourcePosition, calculate_position,
};

#[derive(Logos, Debug, Copy, Clone)]
#[logos(skip r"[ \t\f]+")]
pub enum Token<'source> {
    #[regex(r"[a-zA-Z_][a-zA-Z_0-9]*", |lex| lex.slice())]
    Id(&'source str),
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice())]
    Float(&'source str),
    #[regex(r"[0-9]+", |lex| lex.slice())]
    Integer(&'source str),
    #[regex(r"0x[a-fA-F0-9]+", |lex| lex.slice())]
    HexNumber(&'source str),
    #[regex("\"[^\"]*\"", |lex| lex.slice())]
    Bytes(&'source str),
    #[regex("s\"[^\"]*\"", |lex| lex.slice())]
    Str(&'source str),
    #[token(":")]
    Colon,
    #[token("schema")]
    Schema,
    #[token("impl")]
    Impl,
    #[token("for")]
    For,
    #[token("enum")]
    Enum,
    #[token("init")]
    Init,
    #[token("fn")]
    Fn,
    #[token("status")]
    Status,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("int")]
    Int,
    #[token("bool")]
    Bool,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("{")]
    BraceOpen,
    #[token("}")]
    BraceClose,
    #[token("(")]
    ParenOpen,
    #[token(")")]
    ParenClose,
    #[token("=")]
    EqualAssign,
    #[token("==")]
    Equal,
    #[token("!=")]
    NotEqual,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
    #[token("&&")]
    And,
    #[token("||")]
    Or,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Multiply,
    #[token("/")]
    Divide,
    #[token("%")]
    Modulo,
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("::")]
    DoubleColon,
    #[token("\n")]
    NewLine,
    #[regex(r"//[^\n]*\n")]
    Comment,
    #[token("as")]
    As,
}

#[derive(Logos, Debug, Copy, Clone)]
pub enum StringToken<'source> {
    #[regex(r"[a-fA-F0-9]*[a-fA-F][a-fA-F0-9]*", priority = 3, callback = |lex| {
        let s = lex.slice();
        if s.len().is_multiple_of(2) { Some(s) } else { None }
    })]
    HexString(&'source str),
    #[token(".", priority = 2)]
    Dot,
    #[token("{", priority = 2)]
    BraceOpen,
    #[token("}", priority = 2)]
    BraceClose,
    #[token(":", priority = 2)]
    Colon,
    #[regex(r"[a-zA-Z_][a-zA-Z_0-9]*", priority = 2, callback = |lex| lex.slice())]
    Id(&'source str),
    #[regex(r"[0-9]+", priority = 2, callback = |lex| lex.slice())]
    Integer(&'source str),
    #[regex(r".", priority = 1, callback = |lex| lex.slice())]
    Other(&'source str),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id(String);

impl Id {
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Id(value)
    }
}

impl From<&str> for Id {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl Id {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    Literal(Vec<u8>),
    Variable {
        name: String,
        format: Option<String>,
        length: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

impl From<Token<'_>> for BinaryOp {
    fn from(token: Token<'_>) -> Self {
        match token {
            Token::Plus => Self::Add,
            Token::Minus => Self::Subtract,
            Token::Multiply => Self::Multiply,
            Token::Divide => Self::Divide,
            Token::Modulo => Self::Modulo,
            Token::Equal => Self::Equal,
            Token::NotEqual => Self::NotEqual,
            Token::Less => Self::Less,
            Token::LessEqual => Self::LessEqual,
            Token::Greater => Self::Greater,
            Token::GreaterEqual => Self::GreaterEqual,
            Token::And => Self::And,
            Token::Or => Self::Or,
            _ => panic!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Boolean(bool),
    Identifier(Id),
    QualifiedIdentifier(Id, Id),
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    StringInterpolation {
        parts: Vec<InterpolationPart>,
    },
    Cast {
        expr: Box<Expr>,
        target_type: DataType,
    },
}

impl Expr {
    fn binary_op(a: Expr, op: Token<'_>, b: Expr) -> Self {
        Expr::BinaryOp {
            left: Box::new(a),
            op: op.into(),
            right: Box::new(b),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    Assign(Id, Expr),
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    If {
        condition: Expr,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
    },
}

#[derive(Debug, Clone)]
pub struct Init {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub variants: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Int,
    Bool,
    Enum(String),
    Float,
    Bytes,
    String,
}

impl Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted = match self {
            DataType::Int => "int",
            DataType::Bool => "bool",
            DataType::Enum(enum_type) => enum_type.as_str(),
            DataType::Float => "float",
            DataType::Bytes => "bytes",
            DataType::String => "string",
        };
        write!(f, "{formatted}")
    }
}

impl DataType {
    pub fn is_numeric(&self) -> bool {
        *self == Self::Int || *self == Self::Float
    }
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub param_type: DataType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Member {
    Enum(Enum),
    Init(Init),
    Command(Command),
    Status(Status),
}

#[derive(Debug, Clone)]
pub struct Impl {
    pub schema: String,
    pub name: String,
    pub init: Option<Init>,
    pub status: Option<Status>,
    pub commands: BTreeMap<String, Command>,
    pub enums: Vec<Enum>,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub settings: BTreeMap<Id, Expr>,
}

#[derive(Debug, Clone)]
pub struct RigFile {
    pub settings: Settings,
    pub impl_block: Impl,
}

impl RigFile {
    pub fn get_supported_status_fields(&self) -> HashSet<String> {
        let mut implemented_status = HashSet::new();
        if let Some(status) = &self.impl_block.status {
            for stmt in &status.statements {
                if let Statement::FunctionCall { name, args } = stmt
                    && name == "set_var"
                    && let Some(Expr::String(var_name)) = args.first()
                {
                    implemented_status.insert(var_name.clone());
                }
            }
        }
        implemented_status
    }
}

impl Default for RigFile {
    fn default() -> Self {
        Self {
            settings: Default::default(),
            impl_block: Impl {
                schema: String::new(),
                name: String::new(),
                init: None,
                status: None,
                commands: BTreeMap::new(),
                enums: vec![],
            },
        }
    }
}

peg::parser! {
    pub grammar rig<'source>() for [Token<'source>] {
        rule settings() -> Settings
            = assigns:assign()* {
                Settings {
                    settings: assigns
                        .into_iter()
                        .map(|statement| {
                            match statement {
                                Statement::Assign(id, expr) => (id, expr),
                                _ => panic!("Expected assign statement in settings"),
                            }
                        })
                        .collect()
                }
            }

        rule integer() -> i64
            = [Token::Integer(num)] {?
                num.parse::<i64>().or(Err("Invalid integer"))
            } /
              [Token::HexNumber(num)] {?
                  u32::from_str_radix(&num[2..], 16)
                      .map(|n| n as i64)
                      .or(Err("Invalid hexadecimal number"))
              }

        rule float() -> f64
            = [Token::Float(num)] {?
                num.parse::<f64>().or(Err("Invalid float"))
            }

        rule enum_variant() -> (String, u32)
            = [Token::Id(name)] [Token::EqualAssign] integer:integer() {
                (name.to_string(), integer as u32)
            }

        rule enum_member() -> Member
            = [Token::Enum] [Token::Id(name)] [Token::BraceOpen]
              variants:(enum_variant() ** [Token::Comma]) [Token::Comma]?
              [Token::BraceClose] {
                Member::Enum(Enum {
                    name: name.to_string(),
                    variants: variants.into_iter().collect(),
                })
            }

        rule type_spec() -> DataType
            = [Token::Int] { DataType::Int }
            / [Token::Bool] { DataType::Bool }
            / [Token::Id(data_type)] { DataType::Enum(data_type.to_string()) }

        rule parameter() -> Parameter
            = param_type:type_spec() [Token::Id(name)] {
                Parameter {
                    param_type,
                    name: name.to_string(),
                }
            }

        rule statement() -> Statement
            = if_statement() / function_call_stmt() / var_assign_statement()

        rule function_call_stmt() -> Statement
            = [Token::Id(name)] [Token::ParenOpen]
              args:(expr() ** [Token::Comma]) [Token::Comma]?
              [Token::ParenClose] [Token::Semicolon] {
                Statement::FunctionCall {
                    name: name.to_string(),
                    args
                }
            }

        rule var_assign_statement() -> Statement
            = [Token::Id(var)] [Token::EqualAssign] expr:expr() [Token::Semicolon] {
                Statement::Assign(Id(var.to_string()), expr)
            }

        rule if_statement() -> Statement
            = [Token::If] condition:expr() [Token::BraceOpen]
              then_body:statement()*
              [Token::BraceClose]
              else_body:([Token::Else] else_part:(
                  nested_if:if_statement() { vec![nested_if] } /
                  [Token::BraceOpen] body:statement()* [Token::BraceClose] { body }
              ) { else_part })?
            {
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                }
            }

        rule init() -> Member
            = [Token::Init] [Token::BraceOpen] statements:statement()* [Token::BraceClose] {
                Member::Init(Init { statements })
            }

        rule command() -> Member
            = [Token::Fn] [Token::Id(name)] [Token::ParenOpen]
              params:(parameter() ** [Token::Comma]) [Token::Comma]?
              [Token::ParenClose] [Token::BraceOpen]
              statements:statement()*
              [Token::BraceClose] {
                Member::Command(Command {
                    name: name.to_string(),
                    parameters: params,
                    statements,
                })
            }

        rule status() -> Member
            = [Token::Status] [Token::BraceOpen] statements:statement()* [Token::BraceClose] {
                Member::Status(Status { statements })
            }

        rule member() -> Member
            = member:(init() / enum_member() / command() / status()) {
                member
            }

        rule impl_block() -> Impl
            =
                [Token::Impl]
                [Token::Id(schema)]
                [Token::For]
                [Token::Id(name)]
                [Token::BraceOpen]
                members:member()*
                [Token::BraceClose]
            {
                let mut init = None;
                let mut status = None;
                let mut commands = BTreeMap::new();
                let mut enums = Vec::new();

                for member in members {
                    match member {
                        Member::Init(i) => init = Some(i),
                        Member::Status(s) => status = Some(s),
                        Member::Command(command) => {
                            commands.insert(command.name.clone(), command);
                        },
                        Member::Enum(e) => enums.push(e),
                    }
                }

                Impl {
                    schema: schema.to_string(),
                    name: name.to_string(),
                    init,
                    status,
                    commands,
                    enums,
                }
            }
        pub rule rig_file() -> RigFile
            = settings:settings() impl_block:impl_block() {
                RigFile {
                    settings,
                    impl_block,
                }
            }

        rule assign() -> Statement
            = [Token::Id(id)] [Token::EqualAssign] expr:expr() [Token::Semicolon] {
                Statement::Assign(Id(id.into()), expr)
            }

        pub rule atomic_expr() -> Expr
            = integer:integer() {
                Expr::Integer(integer)
            }
            / float:float() {
                Expr::Float(float)
            }
            / [Token::Bytes(s)] {?
                let content = &s[1..s.len()-1];

                if content.contains('{') && content.contains('}') {
                    let parts = parse_string_interpolation(content)?;
                    Ok(Expr::StringInterpolation { parts })
                } else {
                    let bytes: Vec<_> = content
                        .as_bytes()
                        .iter()
                        .filter(|byte| char::from(**byte) != '.')
                        .copied()
                        .collect();

                    let bytes = bytes
                        .chunks(2)
                        .map(|chunk| {
                            Ok(u8::from_str_radix(std::str::from_utf8(chunk)?, 16)?)
                        })
                        .collect::<Result<Vec<_>>>()
                        .map_err(|err| "Parsing bytes literal failed")?;
                    Ok(Expr::Bytes(bytes))
                }
            }
            / [Token::Str(s)] {
                Expr::String(s[2..s.len()-1].to_string())
            }
            / [Token::True] { Expr::Boolean(true) }
            / [Token::False] { Expr::Boolean(false) }
            / [Token::Id(scope)] [Token::DoubleColon] [Token::Id(id)] {
                Expr::QualifiedIdentifier(scope.into(), id.into())
            }
            / [Token::Id(id)] {
                Expr::Identifier(id.into())
            }

        rule expr() -> Expr = precedence! {
            a:(@) op:([Token::Or] / [Token::And]) b:@ {
                Expr::binary_op(a, op, b)
            }
            --
            a:(@) op:([Token::Equal] / [Token::NotEqual]) b:@ {
                Expr::binary_op(a, op, b)
            }
            --
            a:(@) op:(
                  [Token::Less] /
                  [Token::LessEqual] /
                  [Token::Greater] /
                  [Token::GreaterEqual]
            ) b:@ {
                Expr::binary_op(a, op, b)
            }
            --
            a:(@) op:([Token::Plus] / [Token::Minus]) b:@ {
                Expr::binary_op(a, op, b)
            }
            --
            a:(@) op:([Token::Multiply] / [Token::Divide] / [Token::Modulo]) b:@ {
                Expr::binary_op(a, op, b)
            }
            --
            a:(@) [Token::As] target_type:type_spec() {
                Expr::Cast {
                    expr: Box::new(a),
                    target_type,
                }
            }
            --
            [Token::ParenOpen] expr:expr() [Token::ParenClose] { expr }
            --
            expr:atomic_expr() { expr }
        }
    }
}

peg::parser! {
    pub grammar string_interpolation<'source>() for [StringToken<'source>] {
        rule hex_literal() -> Vec<u8>
            = hex:(
                [StringToken::HexString(hex)] { hex } /
                [StringToken::Integer(int)] {?
                    if int.len() % 2 == 0 && int.chars().all(|c| c.is_ascii_hexdigit()) {
                        Ok(int)
                    } else {
                        Err("Not valid hex")
                    }
                }
            ) {?
                let mut bytes = Vec::new();
                for chunk in hex.as_bytes().chunks(2) {
                    if let Ok(hex_str) = std::str::from_utf8(chunk)
                        && let Ok(byte_val) = u8::from_str_radix(hex_str, 16) {
                            bytes.push(byte_val);
                        }
                }
                Ok(bytes)
            }

        rule variable_spec() -> InterpolationPart
            = [StringToken::BraceOpen] name:([StringToken::Id(id)] { id.to_string() })
              format_and_length:(
                  [StringToken::Colon] format:([StringToken::Id(fmt)] { fmt.to_string() })
                  [StringToken::Colon] length:([StringToken::Integer(len)] {?
                      len.parse::<usize>().or(Err("Invalid length"))
                  }) {
                      (Some(format), length)
                  } /
                  [StringToken::Colon] length:([StringToken::Integer(len)] {?
                      len.parse::<usize>().or(Err("Invalid length"))
                  }) {
                      (None, length)
                  }
              ) [StringToken::BraceClose] {
                  let (format, length) = format_and_length;
                  InterpolationPart::Variable { name, format, length }
              }

        rule literal_content() -> Vec<u8>
            = content:([StringToken::Other(ch)] { ch.as_bytes().to_vec() })+ {
                content.into_iter().flatten().collect()
            }

                rule interpolation_part() -> InterpolationPart
            = hex:hex_literal() { InterpolationPart::Literal(hex) }
            / var:variable_spec() { var }
            / id:([StringToken::Id(id)] { InterpolationPart::Literal(id.as_bytes().to_vec()) }) { id }
            / content:literal_content() { InterpolationPart::Literal(content) }

        pub rule parse_interpolation() -> Vec<InterpolationPart>
            = parts:(interpolation_part() / [StringToken::Dot] { InterpolationPart::Literal(vec![]) })* {
                let mut result = Vec::new();
                let mut current_literal = Vec::new();

                for part in parts {
                    match part {
                        InterpolationPart::Literal(bytes) => {
                            if !bytes.is_empty() {
                                current_literal.extend_from_slice(&bytes);
                            }
                        }
                        var @ InterpolationPart::Variable { .. } => {
                            if !current_literal.is_empty() {
                                result.push(InterpolationPart::Literal(current_literal.clone()));
                                current_literal.clear();
                            }
                            result.push(var);
                        }
                    }
                }

                if !current_literal.is_empty() {
                    result.push(InterpolationPart::Literal(current_literal));
                }

                result
            }
    }
}

fn parse_string_interpolation(template: &str) -> Result<Vec<InterpolationPart>, &'static str> {
    let tokens: Vec<_> = StringToken::lexer(template)
        .collect::<Result<_, _>>()
        .map_err(|_| "Lexer failed")?;

    string_interpolation::parse_interpolation(&tokens).map_err(|_| "Parser failed")
}

pub fn parse_atomic_expr(expr: &str) -> Result<Expr, &str> {
    let tokens: Vec<_> = Token::lexer(expr)
        .collect::<Result<_, _>>()
        .map_err(|_| "Lexer failed")?;
    rig::atomic_expr(&tokens).map_err(|_| "Parsing atomic expr failed")
}

pub fn parse_rig_file(source: &str) -> Result<RigFile, ParseError> {
    parse_with_level(source, ErrorLevel::Normal)
}

pub struct TokenWithPosition<'source> {
    pub token: Token<'source>,
    pub position: SourcePosition,
}

pub fn parse_with_level(source: &str, level: ErrorLevel) -> Result<RigFile, ParseError> {
    let mut lexer = Token::lexer(source);
    let mut tokens_with_positions = Vec::new();

    while let Some(token_result) = lexer.next() {
        match token_result {
            Ok(token) => {
                if !matches!(token, Token::Comment | Token::NewLine) {
                    let span = lexer.span();
                    let position = calculate_position(source, span.start);
                    tokens_with_positions.push(TokenWithPosition { token, position });
                }
            }
            Err(_) => {
                let span = lexer.span();
                let position = calculate_position(source, span.start);
                return Err(ParseError {
                    position,
                    error_type: Box::new(ParseErrorType::Tokenization {
                        message: format!("Unable to tokenize input at position {}", span.start),
                        context: "Invalid character or token".to_string(),
                    }),
                    source: source.to_string(),
                    level,
                });
            }
        }
    }

    let tokens: Vec<Token> = tokens_with_positions.iter().map(|t| t.token).collect();

    rig::rig_file(&tokens).map_err(|peg_error| {
        let error_msg = format!("{peg_error}");

        let position = if peg_error.location < tokens_with_positions.len() {
            tokens_with_positions[peg_error.location].position.clone()
        } else if !tokens_with_positions.is_empty() {
            tokens_with_positions[0].position.clone()
        } else {
            SourcePosition::new(1, 1, 0)
        };

        let found = if peg_error.location < tokens_with_positions.len() {
            Some(format!(
                "{:?}",
                tokens_with_positions[peg_error.location].token
            ))
        } else {
            Some("unexpected token".to_string())
        };

        let expected = peg_error
            .expected
            .tokens()
            .map(|token| token.to_string())
            .collect();

        ParseError {
            position,
            error_type: Box::new(ParseErrorType::Syntax {
                expected,
                found,
                context: format!("Failed to parse rig file structure. PEG error: {error_msg}"),
                peg_error: Some(error_msg),
                user_friendly_message: None,
            }),
            source: source.to_string(),
            level,
        }
    })
}

#[cfg(test)]
mod tests {
    use anyhow::bail;

    use super::*;

    fn create_semantic_error(
        source: &str,
        line: usize,
        column: usize,
        message: &str,
        suggestion: Option<&str>,
    ) -> ParseError {
        ParseError {
            position: SourcePosition::new(line, column, 0),
            error_type: Box::new(ParseErrorType::Semantic {
                message: message.to_string(),
                suggestion: suggestion.map(|s| s.to_string()),
            }),
            source: source.to_string(),
            level: ErrorLevel::Normal,
        }
    }

    #[test]
    fn test_parse_simple_dsl() {
        let dsl_source = r#"
            version = 1;
            impl TestSchema for TestRig {
                init {}
                fn test_command() {}
                status {}
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.schema, "TestSchema");
        assert_eq!(rig_file.impl_block.name, "TestRig");
        assert!(rig_file.impl_block.init.is_some());
        assert!(rig_file.impl_block.status.is_some());
        assert_eq!(rig_file.impl_block.commands.len(), 1);
        assert_eq!(rig_file.impl_block.enums.len(), 0);
        assert_eq!(rig_file.settings.settings.len(), 1);
    }

    #[test]
    fn test_parse_complex_dsl() {
        let dsl_source = r#"
            version = 2;
            baudrate = 9600;
            impl Transceiver for IC7300 {
                enum TestEnum {
                    A = 0,
                    B = 1,
                }
                init {}
                fn command1() {}
                fn command2() {}
                status {}
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.schema, "Transceiver");
        assert_eq!(rig_file.impl_block.name, "IC7300");
        assert!(rig_file.impl_block.init.is_some());
        assert!(rig_file.impl_block.status.is_some());
        assert_eq!(rig_file.impl_block.commands.len(), 2);
        assert_eq!(rig_file.impl_block.enums.len(), 1);
        assert_eq!(rig_file.impl_block.enums[0].name, "TestEnum");
        assert_eq!(rig_file.impl_block.enums[0].variants.len(), 2);
        assert_eq!(rig_file.settings.settings.len(), 2);
    }

    #[test]
    fn test_parse_function_with_parameters() {
        let dsl_source = r#"
            impl Test for Rig {
                fn set_freq(int freq, bool enabled) {
                    write(s"test");
                    read(s"response");
                    command = s"test_command";
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.commands.len(), 1);
        let cmd = &rig_file.impl_block.commands["set_freq"];
        assert_eq!(cmd.name, "set_freq");
        assert_eq!(cmd.parameters.len(), 2);
        assert_eq!(cmd.parameters[0].param_type, DataType::Int);
        assert_eq!(cmd.parameters[0].name, "freq");
        assert_eq!(cmd.parameters[1].param_type, DataType::Bool);
        assert_eq!(cmd.parameters[1].name, "enabled");
        assert_eq!(cmd.statements.len(), 3);
    }

    #[test]
    fn test_parse_ic7300_subset() -> Result<()> {
        let dsl_source = r#"
            version = 1;
            impl Transceiver for IC7300 {
                enum Vfo {
                    A = 0,
                    B = 1,
                }
                enum Mode {
                    LSB = 0,
                    USB = 1,
                    AM = 2,
                }
                init {
                    write("FEFE94E0.1A050071.00.FD");
                    read("FEFE94E01A05007100FD.FEFEE094FBFD");
                }
                fn set_freq(int freq, Vfo target) {
                    command = "FEFE94E0.25.{target:1}.{freq:4}.FD";
                    write(command);
                }
                status {}
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;

        assert_eq!(rig_file.impl_block.schema, "Transceiver");
        assert_eq!(rig_file.impl_block.name, "IC7300");
        assert_eq!(rig_file.impl_block.enums.len(), 2);
        assert_eq!(rig_file.impl_block.commands.len(), 1);

        let vfo_enum = &rig_file.impl_block.enums[0];
        assert_eq!(vfo_enum.name, "Vfo");
        assert_eq!(vfo_enum.variants.len(), 2);
        assert_eq!(vfo_enum.variants.get("A"), Some(&0));
        assert_eq!(vfo_enum.variants.get("B"), Some(&1));

        let cmd = &rig_file.impl_block.commands["set_freq"];
        assert_eq!(cmd.name, "set_freq");
        assert_eq!(cmd.parameters.len(), 2);
        assert_eq!(
            cmd.parameters[1].param_type,
            DataType::Enum("Vfo".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_parse_invalid_dsl() {
        let invalid_dsl = "invalid syntax here";
        let result = parse_rig_file(invalid_dsl);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(
            error.error_type.as_ref(),
            ParseErrorType::Syntax { .. }
        ));
        let error_msg = error.to_string();
        assert!(error_msg.contains("Syntax error"));
        assert!(error_msg.contains("line 1"));
    }

    #[test]
    fn test_parse_empty_string() {
        let result = parse_rig_file("");
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(
            error.error_type.as_ref(),
            ParseErrorType::Syntax { .. }
        ));
    }

    #[test]
    fn test_tokenization_error_messages() {
        let invalid_chars = "impl Test for Rig { \x00 }";
        let result = parse_rig_file(invalid_chars);
        assert!(result.is_err());
        let error = result.unwrap_err();
        if let ParseErrorType::Tokenization { message, .. } = error.error_type.as_ref() {
            assert_eq!(error.position.line, 1);
            assert!(message.contains("Unable to tokenize"));
        } else {
            panic!("Expected tokenization error");
        }
    }

    #[test]
    fn test_syntax_error_messages() {
        let missing_brace = "impl Test for Rig {";
        let result = parse_rig_file(missing_brace);
        assert!(result.is_err());
        let error = result.unwrap_err();
        if let ParseErrorType::Syntax { .. } = error.error_type.as_ref() {
            assert_eq!(error.position.line, 1);
        } else {
            panic!("Expected syntax error");
        }
    }

    #[test]
    fn test_semantic_error_creation() {
        let source = "test source";
        let error = create_semantic_error(
            source,
            5,
            10,
            "Test semantic error",
            Some("Try using 'int' instead"),
        );

        if let ParseErrorType::Semantic {
            message,
            suggestion,
            ..
        } = error.error_type.as_ref()
        {
            assert_eq!(error.position.line, 5);
            assert_eq!(error.position.column, 10);
            assert_eq!(message, "Test semantic error");
            assert_eq!(suggestion, &Some("Try using 'int' instead".to_string()));
        } else {
            panic!("Expected semantic error");
        }
    }

    #[test]
    fn test_parse_minimal_structure() {
        let dsl_source = r#"
            impl Test for Minimal {
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.schema, "Test");
        assert_eq!(rig_file.impl_block.name, "Minimal");
        assert!(rig_file.impl_block.init.is_none());
        assert!(rig_file.impl_block.status.is_none());
        assert_eq!(rig_file.impl_block.commands.len(), 0);
        assert_eq!(rig_file.impl_block.enums.len(), 0);
        assert_eq!(rig_file.settings.settings.len(), 0);
    }

    #[test]
    fn test_parse_only_commands() {
        let dsl_source = r#"
            impl Test for Commands {
                fn cmd1() {}
                fn cmd2() {}
                fn cmd3() {}
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.schema, "Test");
        assert_eq!(rig_file.impl_block.name, "Commands");
        assert!(rig_file.impl_block.init.is_none());
        assert!(rig_file.impl_block.status.is_none());
        assert_eq!(rig_file.impl_block.commands.len(), 3);
        assert_eq!(rig_file.impl_block.enums.len(), 0);
    }

    #[test]
    fn test_parse_real_ic7300_file() {
        let ic7300_content =
            std::fs::read_to_string("rigs/IC-7300.rig").expect("Failed to read IC7300.rig");

        let result = parse_rig_file(&ic7300_content);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.schema, "Transceiver");
        assert_eq!(rig_file.impl_block.name, "IC7300");

        assert!(rig_file.impl_block.commands.len() > 5);

        assert_eq!(rig_file.impl_block.enums.len(), 2);
        assert_eq!(rig_file.impl_block.enums[0].name, "Vfo");
        assert_eq!(rig_file.impl_block.enums[1].name, "Mode");
    }

    #[test]
    fn test_identifiers_not_tokens() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_func() {
                    write(s"test");
                    read(s"response");
                    command = s"test_command";
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        assert_eq!(rig_file.impl_block.commands.len(), 1);
        let cmd = &rig_file.impl_block.commands["test_func"];
        assert_eq!(cmd.name, "test_func");
        assert_eq!(cmd.statements.len(), 3);

        match &cmd.statements[0] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "write");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expr::String(s) => assert_eq!(s, "test"),
                    _ => panic!("Expected string for write"),
                }
            }
            _ => panic!("Expected function call for write"),
        }

        match &cmd.statements[1] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "read");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expr::String(s) => assert_eq!(s, "response"),
                    _ => panic!("Expected string for read"),
                }
            }
            _ => panic!("Expected function call for read"),
        }

        match &cmd.statements[2] {
            Statement::Assign(var, _) => {
                assert_eq!(var.0, "command");
            }
            _ => panic!("Expected variable assignment"),
        }
    }

    #[test]
    fn test_simple_expressions() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    x = 42;
                    y = s"hello";
                    z = identifier;
                    w = a + b;
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        let cmd = &rig_file.impl_block.commands["test"];
        assert_eq!(cmd.statements.len(), 4);

        match &cmd.statements[0] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "x");
                match expr {
                    Expr::Integer(n) => assert_eq!(*n, 42),
                    _ => panic!("Expected integer"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[1] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "y");
                match expr {
                    Expr::String(s) => assert_eq!(s, "hello"),
                    _ => panic!("Expected string"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[2] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "z");
                match expr {
                    Expr::Identifier(id) => assert_eq!(id.0, "identifier"),
                    _ => panic!("Expected identifier"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[3] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "w");
                match expr {
                    Expr::BinaryOp {
                        op: BinaryOp::Add, ..
                    } => {
                        // This is expected
                    }
                    _ => panic!("Expected binary operation"),
                }
            }
            _ => panic!("Expected assignment"),
        }
    }

    #[test]
    fn test_integer_and_float_parsing() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_numbers() {
                    int_var = 42;
                    float_var = 3.5;
                    hex_var = 0xFF;
                    result = int_var + float_var;
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        let cmd = &rig_file.impl_block.commands["test_numbers"];
        assert_eq!(cmd.statements.len(), 4);

        match &cmd.statements[0] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "int_var");
                match expr {
                    Expr::Integer(n) => assert_eq!(*n, 42),
                    _ => panic!("Expected integer, got {expr:?}"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[1] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "float_var");
                match expr {
                    Expr::Float(n) => assert_eq!(*n, 3.5),
                    _ => panic!("Expected float, got {expr:?}"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[2] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "hex_var");
                match expr {
                    Expr::Integer(n) => assert_eq!(*n, 255),
                    _ => panic!("Expected hex integer, got {expr:?}"),
                }
            }
            _ => panic!("Expected assignment"),
        }

        match &cmd.statements[3] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "result");
                match expr {
                    Expr::BinaryOp {
                        op: BinaryOp::Add, ..
                    } => {}
                    _ => panic!("Expected binary operation, got {expr:?}"),
                }
            }
            _ => panic!("Expected assignment"),
        }
    }

    #[test]
    fn test_binary_operations() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_arithmetic() {
                    x = (pitch - 127.5) * 0.425;
                    y = a + b;
                    z = c / d;
                    result = (a + b) * (c - d);
                }
                fn test_comparisons() {
                    if a == b {
                        write("AA");
                    } else if a > b {
                        write("BB");
                    } else if a <= b {
                        write("CC");
                    }
                }
                fn test_logical() {
                    if a && b || c {
                        write("DD");
                    }
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        assert_eq!(rig_file.impl_block.commands.len(), 3);

        let arithmetic_cmd = &rig_file.impl_block.commands["test_arithmetic"];
        assert_eq!(arithmetic_cmd.name, "test_arithmetic");
        assert_eq!(arithmetic_cmd.statements.len(), 4);

        match &arithmetic_cmd.statements[0] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.0, "x");
                match expr {
                    Expr::BinaryOp {
                        op: BinaryOp::Multiply,
                        ..
                    } => {}
                    _ => bail!("Expected binary operation (multiplication)"),
                }
            }
            _ => bail!("Expected assignment statement"),
        }

        let comparison_cmd = &rig_file.impl_block.commands["test_comparisons"];
        assert_eq!(comparison_cmd.name, "test_comparisons");
        assert_eq!(comparison_cmd.statements.len(), 1);

        match &comparison_cmd.statements[0] {
            Statement::If { condition, .. } => match condition {
                Expr::BinaryOp {
                    op: BinaryOp::Equal,
                    ..
                } => {}
                _ => bail!("Expected equality comparison"),
            },
            _ => bail!("Expected if statement"),
        }
        Ok(())
    }

    #[test]
    fn test_string_interpolation_parsing() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_interpolation() {
                    command = "FEFE94E0.25.{vfo:1}.{freq:int_lu:4}.FD";
                    write(command);
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test_interpolation"];
        assert_eq!(cmd.statements.len(), 2);

        match &cmd.statements[0] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.as_str(), "command");
                match expr {
                    Expr::StringInterpolation { parts } => {
                        assert_eq!(parts.len(), 4);

                        match &parts[0] {
                            InterpolationPart::Literal(bytes) => {
                                assert_eq!(bytes, &[0xFE, 0xFE, 0x94, 0xE0, 0x25]);
                            }
                            _ => panic!("Expected literal part"),
                        }

                        match &parts[1] {
                            InterpolationPart::Variable {
                                name,
                                format,
                                length,
                            } => {
                                assert_eq!(name, "vfo");
                                assert_eq!(format, &None);
                                assert_eq!(*length, 1);
                            }
                            _ => panic!("Expected variable part"),
                        }

                        match &parts[2] {
                            InterpolationPart::Variable {
                                name,
                                format,
                                length,
                            } => {
                                assert_eq!(name, "freq");
                                assert_eq!(format, &Some("int_lu".to_string()));
                                assert_eq!(*length, 4);
                            }
                            _ => panic!("Expected variable part"),
                        }

                        match &parts[3] {
                            InterpolationPart::Literal(bytes) => {
                                assert_eq!(bytes, &[0xFD]);
                            }
                            _ => panic!("Expected literal part"),
                        }
                    }
                    _ => panic!("Expected string interpolation"),
                }
            }
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_generic_function_calls() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_func() {
                    write(s"data");
                    read("AABBCC.DD");
                    send_command(s"AT", s"OK");
                    delay(100);
                    custom_func();
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());

        let rig_file = result.unwrap();
        let cmd = &rig_file.impl_block.commands["test_func"];
        assert_eq!(cmd.statements.len(), 5);

        match &cmd.statements[0] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "write");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected function call"),
        }

        match &cmd.statements[1] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "read");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected function call"),
        }

        match &cmd.statements[2] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "send_command");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected function call"),
        }

        match &cmd.statements[3] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "delay");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expr::Integer(n) => assert_eq!(*n, 100),
                    _ => panic!("Expected integer"),
                }
            }
            _ => panic!("Expected function call"),
        }

        match &cmd.statements[4] {
            Statement::FunctionCall { name, args } => {
                assert_eq!(name, "custom_func");
                assert_eq!(args.len(), 0);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_missing_semicolon_error() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    x = 42  // Missing semicolon
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
        assert!(error.to_string().contains("semicolon") || error.to_string().contains("Semicolon"));
    }

    #[test]
    fn test_invalid_enum_syntax() {
        let dsl_source = r#"
            impl Test for Rig {
                enum TestEnum {
                    A = "invalid",  // Should be integer
                    B = 1,
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
    }

    #[test]
    fn test_malformed_string_interpolation() {
        let dsl_source = r#"
             impl Test for Rig {
                 fn test() {
                     command = "FEFE{var:}FD";  // Malformed - missing length
                 }
             }
         "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
    }

    #[test]
    fn test_invalid_parameter_types() {
        let dsl_source = r#"
             impl Test for Rig {
                 fn test(123invalid param) {  // Invalid - starts with number
                 }
             }
         "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
    }

    #[test]
    fn test_missing_impl_block() {
        let dsl_source = r#"
            version = 1;
            // Missing impl block
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
    }

    #[test]
    fn test_empty_functions_and_blocks() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                enum EmptyEnum {
                }
                init {
                }
                fn empty_function() {
                }
                status {
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        assert_eq!(rig_file.impl_block.enums.len(), 1);
        assert_eq!(rig_file.impl_block.enums[0].variants.len(), 0);
        assert!(rig_file.impl_block.init.is_some());
        assert_eq!(
            rig_file.impl_block.init.as_ref().unwrap().statements.len(),
            0
        );
        assert_eq!(rig_file.impl_block.commands.len(), 1);
        assert_eq!(
            rig_file.impl_block.commands["empty_function"]
                .statements
                .len(),
            0
        );
        Ok(())
    }

    #[test]
    fn test_hex_number_parsing() -> Result<()> {
        let dsl_source = r#"
            version = 0xFF;
            baudrate = 0x2580;
            impl Test for Rig {
                fn test() {
                    x = 0xABCD;
                    y = 0x1;
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        assert_eq!(rig_file.settings.settings.len(), 2);

        let cmd = &rig_file.impl_block.commands["test"];
        assert_eq!(cmd.statements.len(), 2);

        match &cmd.statements[0] {
            Statement::Assign(var, expr) => {
                assert_eq!(var.as_str(), "x");
                match expr {
                    Expr::Integer(n) => assert_eq!(*n, 0xABCD),
                    _ => panic!("Expected hex integer"),
                }
            }
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_float_parsing() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    x = 4.14159;
                    y = 0.5;
                    z = 123.456;
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        assert_eq!(cmd.statements.len(), 3);

        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::Float(f) => assert!((f - 4.14159).abs() < 1e-6),
                _ => panic!("Expected float"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_very_long_identifier() -> Result<()> {
        let long_id = "a".repeat(100);
        let dsl_source = format!(
            r#"
            impl Test for Rig {{
                fn test() {{
                    {} = 42;
                }}
            }}
        "#,
            long_id
        );

        let rig_file = parse_rig_file(&dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        match &cmd.statements[0] {
            Statement::Assign(var, _) => {
                assert_eq!(var.as_str(), long_id);
            }
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_comments_in_various_positions() -> Result<()> {
        let dsl_source = r#"
            // Top level comment
            version = 1; // Inline comment
            impl Test for Rig { // Comment after brace
                // Comment in impl block
                enum TestEnum {
                    A = 0, // Comment after enum variant
                    B = 1,
                }
                fn test() { // Comment in function
                    // Comment before statement
                    x = 42; // Comment after statement
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        assert_eq!(rig_file.impl_block.enums.len(), 1);
        assert_eq!(rig_file.impl_block.commands.len(), 1);
        Ok(())
    }

    #[test]
    fn test_string_interpolation_zero_length() {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    command = "FEFE{var:int_lu:0}FD";
                }
            }
        "#;

        let result = parse_rig_file(dsl_source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_string_interpolation_large_length() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    command = "FEFE{var:int_lu:1000}FD";
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::StringInterpolation { parts } => match &parts[1] {
                    InterpolationPart::Variable { length, .. } => {
                        assert_eq!(*length, 1000);
                    }
                    _ => panic!("Expected variable part"),
                },
                _ => panic!("Expected string interpolation"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_mixed_hex_patterns() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    command = "FEFE{var:2}94E0{freq:int_lu:4}FD";
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::StringInterpolation { parts } => {
                    assert_eq!(parts.len(), 5);
                    match &parts[0] {
                        InterpolationPart::Literal(bytes) => {
                            assert_eq!(bytes, &[0xFE, 0xFE]);
                        }
                        _ => panic!("Expected literal part"),
                    }
                    match &parts[1] {
                        InterpolationPart::Variable { name, length, .. } => {
                            assert_eq!(name, "var");
                            assert_eq!(*length, 2);
                        }
                        _ => panic!("Expected variable part"),
                    }
                }
                _ => panic!("Expected string interpolation"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_deeply_nested_expressions() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    result = ((a + b) * (c - d)) / ((e + f) - (g * h));
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::BinaryOp {
                    op: BinaryOp::Divide,
                    ..
                } => {}
                _ => panic!("Expected deeply nested binary operation"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_qualified_identifier_parsing() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                enum TestEnum {
                    A = 0,
                    B = 1,
                }
                fn test() {
                    x = TestEnum::A;
                    y = SomeOther::Value;
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        assert_eq!(cmd.statements.len(), 2);

        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::QualifiedIdentifier(scope, id) => {
                    assert_eq!(scope.as_str(), "TestEnum");
                    assert_eq!(id.as_str(), "A");
                }
                _ => panic!("Expected qualified identifier"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }

    #[test]
    fn test_mixed_expression_types() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    result = 42 + 3.14 + identifier;
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let cmd = &rig_file.impl_block.commands["test"];
        match &cmd.statements[0] {
            Statement::Assign(_, expr) => match expr {
                Expr::BinaryOp { .. } => {}
                _ => panic!("Expected binary operation with mixed types"),
            },
            _ => panic!("Expected assignment"),
        }
        Ok(())
    }
}
