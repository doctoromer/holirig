use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

use super::parser::{
    BinaryOp, DataType, Expr, Id, InterpolationPart, RigFile, Statement, parse_atomic_expr,
};
use crate::{data_format::DataFormat, data_format::DataFormatError, runtime::parser::Enum};

#[derive(Debug, Error)]
pub enum InterpreterError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("Unknown qualified identifier: {scope}::{name}")]
    UnknownQualifiedIdentifier { scope: String, name: String },
    #[error("Unknown command: {0}")]
    UnknownCommand(String),
    #[error("Command '{command}' expects {expected} arguments, got {found}")]
    WrongArgumentCount {
        command: String,
        expected: usize,
        found: usize,
    },
    #[error("Unknown function: {0}")]
    UnknownFunction(String),
    #[error("If condition must be a boolean, got: {0:?}")]
    NonBooleanCondition(Value),
    #[error("Division by zero")]
    DivisionByZero,
    #[error("Modulo by zero")]
    ModuloByZero,
    #[error("Invalid operation {op:?} for {type_name}")]
    InvalidOperation {
        op: BinaryOp,
        type_name: &'static str,
    },
    #[error("Cannot compare enum types: {0}, {1}")]
    EnumTypeMismatch(String, String),
    #[error("Type mismatch in binary operation: {left:?} {op:?} {right:?}")]
    BinaryTypeMismatch {
        left: Value,
        op: BinaryOp,
        right: Value,
    },
    #[error("Cannot interpolate value type: {0:?}")]
    CannotInterpolate(Value),
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    #[error("Invalid enum value: {value} for enum {enum_name}")]
    InvalidEnumValue { value: i64, enum_name: String },
    #[error("Invalid cast from {from:?} to {to:?}")]
    InvalidCast { from: Value, to: DataType },
    #[error("Got invalid response: {0:?}")]
    InvalidResponse(Vec<u8>),
    #[error("{0}")]
    InvalidArguments(String),
    #[error("Response too short: expected {expected} bytes at offset {offset}")]
    ResponseTooShort { expected: usize, offset: usize },
    #[error(
        "Response doesn't match template at offset {offset}: expected {expected:?}, got {actual:?}"
    )]
    ResponseMismatch {
        offset: usize,
        expected: Vec<u8>,
        actual: Vec<u8>,
    },
    #[error("Unknown parameter: {key} in command {command}")]
    UnknownParameter { key: String, command: String },
    #[error("Missing parameter {param} in command {command}")]
    MissingParameter { param: String, command: String },
    #[error("Unknown parameters: {0}")]
    UnknownParameters(String),
    #[error(transparent)]
    DataFormat(#[from] DataFormatError),
    #[error("Failed to decode {length} bytes using format {format:?}, data: {data:?}")]
    DecodeFailed {
        length: usize,
        format: String,
        data: Vec<u8>,
        #[source]
        source: DataFormatError,
    },
    #[error(transparent)]
    External(#[from] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Bytes(Vec<u8>),
    String(String),
    Boolean(bool),
    EnumVariant {
        enum_name: String,
        variant_name: String,
        value: u32,
    },
    Unit,
}

impl From<Value> for serde_json::Value {
    fn from(value: Value) -> Self {
        (&value).into()
    }
}

impl From<&Value> for serde_json::Value {
    fn from(value: &Value) -> Self {
        match value {
            Value::Integer(integer) => (*integer).into(),
            Value::Float(float) => (*float).into(),
            Value::Bytes(_) => todo!(),
            Value::String(string) => string.clone().into(),
            Value::Boolean(boolean) => (*boolean).into(),
            Value::EnumVariant { variant_name, .. } => variant_name.as_str().into(),
            Value::Unit => todo!(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{i}"),
            Value::Float(fl) => write!(f, "{fl}"),
            Value::Bytes(b) => {
                if let Ok(string) = String::from_utf8(b.clone()) {
                    write!(f, "{string}")
                } else {
                    write!(f, "{b:?}")
                }
            }
            Value::String(string) => {
                write!(f, "{string}")
            }
            Value::Boolean(b) => write!(f, "{b}"),
            Value::EnumVariant {
                enum_name,
                variant_name,
                value,
            } => {
                write!(f, "{enum_name}::{variant_name}({value})")
            }
            Value::Unit => write!(f, "()"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    variables: HashMap<String, Value>,
    parent: Option<Box<Env>>,
    enums: HashMap<String, HashMap<String, u32>>,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parent(parent: Env) -> Self {
        Env {
            variables: HashMap::new(),
            parent: Some(Box::new(parent)),
            enums: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: String, value: Value) {
        self.variables.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.variables.get(name) {
            Some(value.clone())
        } else if let Some(parent) = &self.parent {
            parent.get(name)
        } else {
            None
        }
    }

    pub fn get_enum_variant(&self, enum_name: &str, variant_name: &str) -> Option<u32> {
        self.enums
            .get(enum_name)
            .and_then(|variants| variants.get(variant_name).copied())
            .or_else(|| {
                self.parent
                    .as_ref()
                    .and_then(|parent| parent.get_enum_variant(enum_name, variant_name))
            })
    }

    pub fn get_enum_variant_by_value(&self, enum_name: &str, value: u32) -> Option<String> {
        self.enums
            .get(enum_name)
            .and_then(|variants| {
                variants
                    .iter()
                    .find(|(_, v)| **v == value)
                    .map(|(name, _)| name.clone())
            })
            .or_else(|| {
                self.parent
                    .as_ref()
                    .and_then(|parent| parent.get_enum_variant_by_value(enum_name, value))
            })
    }

    pub fn register_enum(&mut self, enum_def: &Enum) {
        self.enums.insert(
            enum_def.name.clone(),
            enum_def.variants.clone().into_iter().collect(),
        );
    }
}

pub trait ExternalApi: Send + Sync {
    fn write(&self, data: &[u8]) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn read(&self, size: usize) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
    fn set_var(&self, var: &str, value: Value) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct Interpreter {
    rig_file: RigFile,
}

impl Interpreter {
    pub fn new(rig_file: RigFile) -> Self {
        Self { rig_file }
    }

    pub fn rig_file(&self) -> &RigFile {
        &self.rig_file
    }

    pub fn create_env(&self) -> Result<Env, InterpreterError> {
        let mut env = Env::new();

        for (id, expr) in &self.rig_file.settings.settings {
            let value = self.evaluate_expression(expr, &mut env)?;
            env.set(id.to_string(), value);
        }

        for enum_def in &self.rig_file.impl_block.enums {
            env.register_enum(enum_def);
        }

        Ok(env)
    }

    pub async fn execute_command_with_env(
        &self,
        name: &str,
        args: &[Value],
        api: &impl ExternalApi,
        env: &mut Env,
    ) -> Result<(), InterpreterError> {
        let command = self
            .rig_file
            .impl_block
            .commands
            .get(name)
            .ok_or_else(|| InterpreterError::UnknownCommand(name.to_string()))?;
        if args.len() != command.parameters.len() {
            return Err(InterpreterError::WrongArgumentCount {
                command: command.name.clone(),
                expected: command.parameters.len(),
                found: args.len(),
            });
        }

        let mut local_env = Env::with_parent(env.clone());
        for (param, arg) in command.parameters.iter().zip(args.iter()) {
            local_env.set(param.name.clone(), arg.clone());
        }

        for statement in &command.statements {
            self.execute_statement(statement, api, &mut local_env)
                .await?;
        }

        Ok(())
    }

    pub async fn execute_init_with_env(
        &self,
        api: &impl ExternalApi,
        env: &mut Env,
    ) -> Result<(), InterpreterError> {
        if let Some(init) = &self.rig_file.impl_block.init {
            for statement in &init.statements {
                self.execute_statement(statement, api, env).await?;
            }
        }
        Ok(())
    }

    pub async fn execute_status_with_env(
        &self,
        api: &impl ExternalApi,
        env: &mut Env,
    ) -> Result<(), InterpreterError> {
        if let Some(status) = &self.rig_file.impl_block.status {
            for statement in &status.statements {
                self.execute_statement(statement, api, env).await?;
            }
        }
        Ok(())
    }

    async fn execute_function_call(
        &self,
        name: &str,
        args: &[Expr],
        api: &impl ExternalApi,
        env: &mut Env,
    ) -> Result<(), InterpreterError> {
        match name {
            "read" => {
                match args {
                    [Expr::StringInterpolation { parts }] => {
                        let expected_length = parts
                            .iter()
                            .map(|part| match part {
                                InterpolationPart::Literal(bytes) => bytes.len(),
                                InterpolationPart::Variable { length, .. } => *length,
                            })
                            .sum();

                        let response = api.read(expected_length).await?;

                        parse_response_with_template(parts, &response, env)?;
                    }
                    [Expr::Bytes(bytes)] => {
                        let response = api.read(bytes.len()).await?;
                        if &response != bytes {
                            return Err(InterpreterError::InvalidResponse(response));
                        }
                    }
                    _ => {
                        return Err(InterpreterError::InvalidArguments(format!(
                            "Expected template string in read, got: {args:?}"
                        )));
                    }
                };
                Ok(())
            }
            "write" => {
                let args = args
                    .iter()
                    .map(|arg| self.evaluate_expression(arg, env))
                    .collect::<Result<Vec<_>, InterpreterError>>()?;

                let bytes = match &args[..] {
                    [Value::Bytes(bytes)] => bytes.clone(),
                    [Value::String(string)] => string.as_bytes().to_vec(),
                    _ => {
                        return Err(InterpreterError::InvalidArguments(format!(
                            "Expected one bytes or string argument in write, got: {args:?}"
                        )));
                    }
                };
                api.write(&bytes).await?;
                Ok(())
            }
            "set_var" => {
                let args = args
                    .iter()
                    .map(|arg| self.evaluate_expression(arg, env))
                    .collect::<Result<Vec<_>, InterpreterError>>()?;

                let [Value::String(var), value] = &args[..] else {
                    return Err(InterpreterError::InvalidArguments(format!(
                        "Expected string and value arguments in set_var, got: {args:?}"
                    )));
                };

                api.set_var(var, value.clone())?;
                Ok(())
            }
            _ => Err(InterpreterError::UnknownFunction(name.to_string())),
        }
    }

    async fn execute_statement(
        &self,
        statement: &Statement,
        api: &impl ExternalApi,
        env: &mut Env,
    ) -> Result<(), InterpreterError> {
        match statement {
            Statement::Assign(id, expr) => {
                let value = self.evaluate_expression(expr, env)?;
                env.set(id.to_string(), value);
            }
            Statement::FunctionCall { name, args } => {
                self.execute_function_call(name, args, api, env).await?
            }
            Statement::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition_value = self.evaluate_expression(condition, env)?;
                match condition_value {
                    Value::Boolean(true) => {
                        for stmt in then_body {
                            Box::pin(self.execute_statement(stmt, api, env)).await?;
                        }
                    }
                    Value::Boolean(false) => {
                        if let Some(else_stmts) = else_body {
                            for stmt in else_stmts {
                                Box::pin(self.execute_statement(stmt, api, env)).await?;
                            }
                        }
                    }
                    _ => {
                        return Err(InterpreterError::NonBooleanCondition(condition_value));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn evaluate_expression(
        &self,
        expr: &Expr,
        env: &mut Env,
    ) -> Result<Value, InterpreterError> {
        match expr {
            Expr::Integer(i) => Ok(Value::Integer(*i)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Boolean(b) => Ok(Value::Boolean(*b)),
            Expr::Bytes(bytes) => Ok(Value::Bytes(bytes.clone())),
            Expr::String(string) => Ok(Value::String(string.clone())),
            Expr::Identifier(id) => env
                .get(id.as_str())
                .ok_or_else(|| InterpreterError::UndefinedVariable(id.to_string())),
            Expr::QualifiedIdentifier(scope, id) => {
                if let Some(value) = env.get_enum_variant(scope.as_str(), id.as_str()) {
                    Ok(Value::EnumVariant {
                        enum_name: scope.to_string(),
                        variant_name: id.to_string(),
                        value,
                    })
                } else {
                    Err(InterpreterError::UnknownQualifiedIdentifier {
                        scope: scope.to_string(),
                        name: id.to_string(),
                    })
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(left, env)?;
                let right_val = self.evaluate_expression(right, env)?;
                Self::apply_binary_op(&left_val, op, &right_val)
            }
            Expr::StringInterpolation { parts } => {
                self.process_parsed_string_interpolation(parts, env)
            }
            Expr::Cast { expr, target_type } => {
                let value = self.evaluate_expression(expr, env)?;
                self.apply_cast(&value, target_type, env)
            }
        }
    }

    fn apply_binary_op(
        left: &Value,
        op: &BinaryOp,
        right: &Value,
    ) -> Result<Value, InterpreterError> {
        match (left, right) {
            (Value::Integer(a), Value::Integer(b)) => match op {
                BinaryOp::Add => Ok(Value::Integer(a + b)),
                BinaryOp::Subtract => Ok(Value::Integer(a - b)),
                BinaryOp::Multiply => Ok(Value::Integer(a * b)),
                BinaryOp::Divide => {
                    if *b == 0 {
                        Err(InterpreterError::DivisionByZero)
                    } else {
                        Ok(Value::Integer(a / b))
                    }
                }
                BinaryOp::Modulo => {
                    if *b == 0 {
                        Err(InterpreterError::ModuloByZero)
                    } else {
                        Ok(Value::Integer(a % b))
                    }
                }
                BinaryOp::Equal => Ok(Value::Boolean(a == b)),
                BinaryOp::NotEqual => Ok(Value::Boolean(a != b)),
                BinaryOp::Less => Ok(Value::Boolean(a < b)),
                BinaryOp::LessEqual => Ok(Value::Boolean(a <= b)),
                BinaryOp::Greater => Ok(Value::Boolean(a > b)),
                BinaryOp::GreaterEqual => Ok(Value::Boolean(a >= b)),
                BinaryOp::And => Ok(Value::Boolean(*a != 0 && *b != 0)),
                BinaryOp::Or => Ok(Value::Boolean(*a != 0 || *b != 0)),
            },
            (Value::Float(a), Value::Float(b)) => match op {
                BinaryOp::Add => Ok(Value::Float(a + b)),
                BinaryOp::Subtract => Ok(Value::Float(a - b)),
                BinaryOp::Multiply => Ok(Value::Float(a * b)),
                BinaryOp::Divide => Ok(Value::Float(a / b)),
                BinaryOp::Modulo => Ok(Value::Float(a % b)),
                BinaryOp::Equal => Ok(Value::Boolean((a - b).abs() < f64::EPSILON)),
                BinaryOp::NotEqual => Ok(Value::Boolean((a - b).abs() >= f64::EPSILON)),
                BinaryOp::Less => Ok(Value::Boolean(a < b)),
                BinaryOp::LessEqual => Ok(Value::Boolean(a <= b)),
                BinaryOp::Greater => Ok(Value::Boolean(a > b)),
                BinaryOp::GreaterEqual => Ok(Value::Boolean(a >= b)),
                BinaryOp::And | BinaryOp::Or => Err(InterpreterError::InvalidOperation {
                    op: op.clone(),
                    type_name: "floats",
                }),
            },
            (Value::Integer(a), Value::Float(b)) => {
                Self::apply_binary_op(&Value::Float(*a as f64), op, &Value::Float(*b))
            }
            (Value::Float(a), Value::Integer(b)) => {
                Self::apply_binary_op(&Value::Float(*a), op, &Value::Float(*b as f64))
            }
            (Value::Bytes(a), Value::Bytes(b)) => match op {
                BinaryOp::Add => {
                    let mut result = a.clone();
                    result.extend_from_slice(b);
                    Ok(Value::Bytes(result))
                }
                BinaryOp::Equal => Ok(Value::Boolean(a == b)),
                BinaryOp::NotEqual => Ok(Value::Boolean(a != b)),
                _ => Err(InterpreterError::InvalidOperation {
                    op: op.clone(),
                    type_name: "strings",
                }),
            },
            (Value::Boolean(a), Value::Boolean(b)) => match op {
                BinaryOp::Equal => Ok(Value::Boolean(a == b)),
                BinaryOp::NotEqual => Ok(Value::Boolean(a != b)),
                BinaryOp::And => Ok(Value::Boolean(*a && *b)),
                BinaryOp::Or => Ok(Value::Boolean(*a || *b)),
                _ => Err(InterpreterError::InvalidOperation {
                    op: op.clone(),
                    type_name: "booleans",
                }),
            },
            (
                Value::EnumVariant {
                    enum_name: name1,
                    variant_name: _,
                    value: value1,
                },
                Value::EnumVariant {
                    enum_name: name2,
                    variant_name: _,
                    value: value2,
                },
            ) => match op {
                BinaryOp::Equal => {
                    if name1 != name2 {
                        Err(InterpreterError::EnumTypeMismatch(
                            name1.clone(),
                            name2.clone(),
                        ))
                    } else {
                        Ok(Value::Boolean(value1 == value2))
                    }
                }
                _ => Err(InterpreterError::InvalidOperation {
                    op: op.clone(),
                    type_name: "enums",
                }),
            },
            _ => Err(InterpreterError::BinaryTypeMismatch {
                left: left.clone(),
                op: op.clone(),
                right: right.clone(),
            }),
        }
    }

    fn process_parsed_string_interpolation(
        &self,
        parts: &[InterpolationPart],
        env: &mut Env,
    ) -> Result<Value, InterpreterError> {
        let mut result = Vec::new();

        for part in parts {
            match part {
                InterpolationPart::Literal(bytes) => {
                    result.extend_from_slice(bytes);
                }
                InterpolationPart::Variable {
                    name,
                    format,
                    length,
                } => {
                    let interpolated =
                        self.interpolate_parsed_variable(name, format.as_deref(), *length, env)?;
                    result.extend_from_slice(&interpolated);
                }
            }
        }

        Ok(Value::Bytes(result))
    }

    fn interpolate_parsed_variable(
        &self,
        name: &str,
        format: Option<&str>,
        length: usize,
        env: &mut Env,
    ) -> Result<Vec<u8>, InterpreterError> {
        let value = env
            .get(name)
            .ok_or_else(|| InterpreterError::UndefinedVariable(name.to_string()))?;

        match format {
            None => match value {
                Value::Integer(i) => {
                    let format = DataFormat::IntLu;
                    let bytes = format.encode(i as i32, length)?;
                    Ok(bytes)
                }
                Value::EnumVariant { value, .. } => {
                    let format = DataFormat::IntLu;
                    let bytes = format.encode(value as i32, length)?;
                    Ok(bytes)
                }
                Value::Boolean(value) => {
                    let format = DataFormat::IntLu;
                    let bytes = format.encode(value as i32, length)?;
                    Ok(bytes)
                }
                other => Err(InterpreterError::CannotInterpolate(other)),
            },
            Some(format_str) => {
                let format = DataFormat::try_from(format_str)
                    .map_err(|_| InterpreterError::InvalidFormat(format_str.to_string()))?;

                match value {
                    Value::Integer(i) => {
                        let bytes = format.encode(i as i32, length)?;
                        Ok(bytes)
                    }
                    Value::EnumVariant { value, .. } => {
                        let bytes = format.encode(value as i32, length)?;
                        Ok(bytes)
                    }
                    Value::Boolean(value) => {
                        let bytes = format.encode(value as i32, length)?;
                        Ok(bytes)
                    }
                    other => Err(InterpreterError::CannotInterpolate(other)),
                }
            }
        }
    }

    fn apply_cast(
        &self,
        value: &Value,
        target_type: &DataType,
        env: &mut Env,
    ) -> Result<Value, InterpreterError> {
        match (value, target_type) {
            (Value::Integer(i), DataType::Float) => Ok(Value::Float(*i as f64)),
            (Value::Integer(i), DataType::Bool) => Ok(Value::Boolean(*i != 0)),
            (Value::Integer(i), DataType::Enum(enum_name)) => {
                if let Some(variant_name) = env.get_enum_variant_by_value(enum_name, *i as u32) {
                    Ok(Value::EnumVariant {
                        enum_name: enum_name.clone(),
                        variant_name,
                        value: *i as u32,
                    })
                } else {
                    Err(InterpreterError::InvalidEnumValue {
                        value: *i,
                        enum_name: enum_name.clone(),
                    })
                }
            }
            (Value::Float(f), DataType::Int) => Ok(Value::Integer(*f as i64)),
            (Value::Boolean(b), DataType::Int) => Ok(Value::Integer(if *b { 1 } else { 0 })),
            (Value::EnumVariant { value, .. }, DataType::Int) => Ok(Value::Integer(*value as i64)),
            _ => Err(InterpreterError::InvalidCast {
                from: value.clone(),
                to: target_type.clone(),
            }),
        }
    }

    pub fn eval_external_args(
        &self,
        name: &str,
        args: HashMap<String, String>,
        env: &mut Env,
    ) -> Result<Vec<Value>, InterpreterError> {
        let params = &self
            .rig_file
            .impl_block
            .commands
            .get(name)
            .ok_or_else(|| InterpreterError::UnknownCommand(name.to_string()))?
            .parameters;

        let mut evaluated_args = args
            .iter()
            .map(|(key, value)| {
                let param_type = &params
                    .iter()
                    .find(|param| &param.name == key)
                    .ok_or_else(|| InterpreterError::UnknownParameter {
                        key: key.clone(),
                        command: name.to_string(),
                    })?
                    .param_type;

                let parsed = if let DataType::Enum(enum_name) = param_type {
                    Expr::QualifiedIdentifier(Id::new(enum_name), Id::new(value))
                } else {
                    parse_atomic_expr(value)
                        .map_err(|err| InterpreterError::InvalidArguments(err.to_string()))?
                };
                Ok((key.clone(), self.evaluate_expression(&parsed, env)?))
            })
            .collect::<Result<HashMap<_, _>, InterpreterError>>()?;

        let result = params
            .iter()
            .map(|param| {
                let value = evaluated_args.remove(&param.name).ok_or_else(|| {
                    InterpreterError::MissingParameter {
                        param: param.name.clone(),
                        command: name.to_string(),
                    }
                })?;
                Ok(value)
            })
            .collect::<Result<_, InterpreterError>>()?;

        if !evaluated_args.is_empty() {
            return Err(InterpreterError::UnknownParameters(
                evaluated_args.into_keys().collect::<Vec<_>>().join(", "),
            ));
        }
        Ok(result)
    }

    pub async fn execute_init(&self, external: &impl ExternalApi) -> Result<(), InterpreterError> {
        let mut env = self.create_env()?;
        self.execute_init_with_env(external, &mut env).await
    }

    pub async fn execute_command(
        &self,
        command_name: &str,
        params: HashMap<String, String>,
        external: &impl ExternalApi,
    ) -> Result<HashMap<String, Value>, InterpreterError> {
        let mut env = self.create_env()?;

        let args = self.eval_external_args(command_name, params, &mut self.create_env()?)?;
        self.execute_command_with_env(command_name, &args, external, &mut env)
            .await?;

        Ok(HashMap::new())
    }

    pub async fn execute_status(
        &self,
        external: &impl ExternalApi,
    ) -> Result<(), InterpreterError> {
        let mut env = self.create_env()?;
        self.execute_status_with_env(external, &mut env).await
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

fn parse_response_with_template(
    parts: &[InterpolationPart],
    response: &[u8],
    env: &mut Env,
) -> Result<(), InterpreterError> {
    let mut offset = 0;

    for part in parts {
        match part {
            InterpolationPart::Literal(expected_bytes) => {
                if offset + expected_bytes.len() > response.len() {
                    return Err(InterpreterError::ResponseTooShort {
                        expected: expected_bytes.len(),
                        offset,
                    });
                }

                let actual = &response[offset..offset + expected_bytes.len()];
                if actual != expected_bytes {
                    return Err(InterpreterError::ResponseMismatch {
                        offset,
                        expected: expected_bytes.to_vec(),
                        actual: actual.to_vec(),
                    });
                }
                offset += expected_bytes.len();
            }
            InterpolationPart::Variable {
                name,
                format,
                length,
            } => {
                if offset + length > response.len() {
                    return Err(InterpreterError::ResponseTooShort {
                        expected: *length,
                        offset,
                    });
                }

                let bytes = &response[offset..offset + length];
                let format_str = format.as_deref().unwrap_or("int_lu");
                let data_format = DataFormat::try_from(format_str)
                    .map_err(|_| InterpreterError::InvalidFormat(format_str.to_string()))?;
                let value =
                    data_format
                        .decode(bytes)
                        .map_err(|source| InterpreterError::DecodeFailed {
                            length: *length,
                            format: format_str.to_string(),
                            data: bytes.to_vec(),
                            source,
                        })?;

                if name != "_" {
                    env.set(name.clone(), Value::Integer(value as i64));
                }
                offset += length;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use parking_lot::RwLock;

    use super::*;
    use crate::runtime::parser::{Id, parse_rig_file};
    use std::collections::BTreeMap;

    struct DummyExternalApi {
        output: RwLock<Vec<String>>,
    }

    impl DummyExternalApi {
        fn new() -> Self {
            Self {
                output: RwLock::new(vec![]),
            }
        }
    }

    impl ExternalApi for DummyExternalApi {
        async fn write(&self, data: &[u8]) -> Result<()> {
            self.output.write().push(format!("WRITE: {:?}", data));
            Ok(())
        }
        async fn read(&self, size: usize) -> Result<Vec<u8>> {
            self.output.write().push(format!("READ: {size}"));
            Ok(vec![])
        }
        fn set_var(&self, _var: &str, _value: Value) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_basic_expression_evaluation() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::Integer(42);
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Integer(42));

        let expr = Expr::Float(3.5);
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Float(3.5));

        let expr = Expr::String("hello".to_string());
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::String("hello".to_string()));
        Ok(())
    }

    #[test]
    fn test_binary_operations() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Integer(10)),
            op: BinaryOp::Add,
            right: Box::new(Expr::Integer(5)),
        };
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Integer(15));

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Integer(10)),
            op: BinaryOp::Greater,
            right: Box::new(Expr::Integer(5)),
        };
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Boolean(true));
        Ok(())
    }

    #[tokio::test]
    async fn test_variable_assignment_and_lookup() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let statement = Statement::Assign(Id::new("x"), Expr::Integer(42));
        interpreter
            .execute_statement(&statement, &DummyExternalApi::new(), &mut env)
            .await?;

        let expr = Expr::Identifier(Id::new("x"));
        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Integer(42));
        Ok(())
    }

    #[tokio::test]
    async fn test_function_call() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let statement = Statement::FunctionCall {
            name: "write".to_string(),
            args: vec![Expr::Bytes(vec![1, 2, 3, 4])],
        };
        let api = DummyExternalApi::new();
        interpreter
            .execute_statement(&statement, &api, &mut env)
            .await?;

        assert_eq!(api.output.read().len(), 1);
        assert_eq!(api.output.read()[0], "WRITE: [1, 2, 3, 4]");
        Ok(())
    }

    #[test]
    fn test_string_interpolation() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("freq".to_string(), Value::Integer(14500000));
        env.set(
            "vfo".to_string(),
            Value::EnumVariant {
                enum_name: "Vfo".to_string(),
                variant_name: "A".to_string(),
                value: 1,
            },
        );

        let expr = Expr::StringInterpolation {
            parts: vec![
                InterpolationPart::Literal(vec![0xFE, 0xFE, 0x94, 0xE0]),
                InterpolationPart::Literal(vec![0x25]),
                InterpolationPart::Variable {
                    name: "vfo".to_string(),
                    format: None,
                    length: 1,
                },
                InterpolationPart::Variable {
                    name: "freq".to_string(),
                    format: Some("int_lu".to_string()),
                    length: 4,
                },
                InterpolationPart::Literal(vec![0xFD]),
            ],
        };

        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(
            result,
            Value::Bytes(
                [
                    0xFE, 0xFE, 0x94, 0xE0, 0x25, 0x01, 0xA0, 0x40, 0xDD, 0x00, 0xFD
                ]
                .to_vec()
            )
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_if_statement() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("x".to_string(), Value::Integer(10));

        let statement = Statement::If {
            condition: Expr::BinaryOp {
                left: Box::new(Expr::Identifier(Id::new("x"))),
                op: BinaryOp::Greater,
                right: Box::new(Expr::Integer(5)),
            },
            then_body: vec![Statement::FunctionCall {
                name: "write".to_string(),
                args: vec![Expr::Bytes(vec![1])],
            }],
            else_body: Some(vec![Statement::FunctionCall {
                name: "write".to_string(),
                args: vec![Expr::Bytes(vec![0])],
            }]),
        };

        let api = DummyExternalApi::new();
        interpreter
            .execute_statement(&statement, &api, &mut env)
            .await?;

        assert_eq!(api.output.read().len(), 1);
        assert_eq!(api.output.read()[0], "WRITE: [1]");
        Ok(())
    }

    #[test]
    fn test_enum_handling() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let enum_def = Enum {
            name: "Vfo".to_string(),
            variants: BTreeMap::from([("A".to_string(), 0), ("B".to_string(), 1)]),
        };
        env.register_enum(&enum_def);

        let expr = Expr::QualifiedIdentifier(Id::new("Vfo"), Id::new("A"));
        let result = interpreter.evaluate_expression(&expr, &mut env)?;

        match result {
            Value::EnumVariant {
                enum_name,
                variant_name,
                value,
            } => {
                assert_eq!(enum_name, "Vfo");
                assert_eq!(variant_name, "A");
                assert_eq!(value, 0);
            }
            other => panic!("Expected enum variant, got: {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_simple_rig_file_execution() -> Result<()> {
        let dsl_source = r#"
            version = 1;

            impl Transceiver for TestRig {
                enum Vfo {
                    A = 0,
                    B = 1,
                }
                init {
                    write("01020304");
                }
                fn set_freq(int freq, Vfo vfo) {
                    command = "FEFE94E0.25.{vfo:1}.{freq:4}.FD";
                    write(command);
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source).unwrap();
        let interpreter = Interpreter::new(rig_file.clone());
        let mut env = interpreter.create_env().unwrap();
        let api = DummyExternalApi::new();
        interpreter
            .execute_init_with_env(&api, &mut env)
            .await
            .unwrap();

        assert_eq!(env.get("version"), Some(Value::Integer(1)));

        assert_eq!(env.get_enum_variant("Vfo", "A"), Some(0));
        assert_eq!(env.get_enum_variant("Vfo", "B"), Some(1));

        assert!(api.output.read().len() == 1);

        let args = vec![
            Value::Integer(14500000),
            Value::EnumVariant {
                enum_name: "Vfo".to_string(),
                variant_name: "A".to_string(),
                value: 0,
            },
        ];
        interpreter
            .execute_command_with_env("set_freq", &args, &api, &mut env)
            .await?;

        assert_eq!(api.output.read()[0], "WRITE: [1, 2, 3, 4]");

        let last_output = api.output.read().last().unwrap().clone();
        assert!(last_output.contains("WRITE: [254, 254, 148, 224, 37, 0, 160, 64, 221, 0, 253]"));
        Ok(())
    }

    #[test]
    fn test_division_by_zero_error() {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Integer(10)),
            op: BinaryOp::Divide,
            right: Box::new(Expr::Integer(0)),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            error.to_string().to_lowercase().contains("division")
                || error.to_string().to_lowercase().contains("zero")
        );
    }

    #[test]
    fn test_modulo_by_zero_error() {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Integer(10)),
            op: BinaryOp::Modulo,
            right: Box::new(Expr::Integer(0)),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env);
        assert!(result.is_err());
    }

    #[test]
    fn test_undefined_variable_access() {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::Identifier(Id::new("undefined_variable"));
        let result = interpreter.evaluate_expression(&expr, &mut env);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("undefined") || error.to_string().contains("not found"));
    }

    #[test]
    fn test_complex_nested_expressions() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("a".to_string(), Value::Integer(10));
        env.set("b".to_string(), Value::Integer(5));
        env.set("c".to_string(), Value::Integer(3));
        env.set("d".to_string(), Value::Integer(2));

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::BinaryOp {
                    left: Box::new(Expr::Identifier(Id::new("a"))),
                    op: BinaryOp::Add,
                    right: Box::new(Expr::Identifier(Id::new("b"))),
                }),
                op: BinaryOp::Multiply,
                right: Box::new(Expr::BinaryOp {
                    left: Box::new(Expr::Identifier(Id::new("c"))),
                    op: BinaryOp::Subtract,
                    right: Box::new(Expr::Identifier(Id::new("d"))),
                }),
            }),
            op: BinaryOp::Add,
            right: Box::new(Expr::Integer(5)),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Integer(20));
        Ok(())
    }

    #[test]
    fn test_operator_precedence_validation() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("a".to_string(), Value::Integer(2));
        env.set("b".to_string(), Value::Integer(3));
        env.set("c".to_string(), Value::Integer(4));

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Identifier(Id::new("a"))),
            op: BinaryOp::Add,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Identifier(Id::new("b"))),
                op: BinaryOp::Multiply,
                right: Box::new(Expr::Identifier(Id::new("c"))),
            }),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Integer(14));
        Ok(())
    }

    #[test]
    fn test_float_integer_mixed_operations() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Float(3.5)),
            op: BinaryOp::Add,
            right: Box::new(Expr::Integer(2)),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        match result {
            Value::Float(f) => assert!((f - 5.5).abs() < 1e-6),
            _ => panic!("Expected float result"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_nested_if_statements() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let nested_if = Statement::If {
            condition: Expr::BinaryOp {
                left: Box::new(Expr::Integer(5)),
                op: BinaryOp::Greater,
                right: Box::new(Expr::Integer(3)),
            },
            then_body: vec![
                Statement::FunctionCall {
                    name: "write".to_string(),
                    args: vec![Expr::Bytes(vec![1])],
                },
                Statement::If {
                    condition: Expr::BinaryOp {
                        left: Box::new(Expr::Integer(1)),
                        op: BinaryOp::Equal,
                        right: Box::new(Expr::Integer(1)),
                    },
                    then_body: vec![Statement::FunctionCall {
                        name: "write".to_string(),
                        args: vec![Expr::Bytes(vec![2])],
                    }],
                    else_body: None,
                },
            ],
            else_body: Some(vec![Statement::FunctionCall {
                name: "write".to_string(),
                args: vec![Expr::Bytes(vec![3])],
            }]),
        };

        let api = DummyExternalApi::new();
        interpreter
            .execute_statement(&nested_if, &api, &mut env)
            .await?;

        assert_eq!(api.output.read().len(), 2);
        assert_eq!(api.output.read()[0], "WRITE: [1]");
        assert_eq!(api.output.read()[1], "WRITE: [2]");
        Ok(())
    }

    #[test]
    fn test_complex_boolean_expressions() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let expr = Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::BinaryOp {
                    left: Box::new(Expr::Integer(1)),
                    op: BinaryOp::Equal,
                    right: Box::new(Expr::Integer(1)),
                }),
                op: BinaryOp::And,
                right: Box::new(Expr::BinaryOp {
                    left: Box::new(Expr::Integer(1)),
                    op: BinaryOp::Equal,
                    right: Box::new(Expr::Integer(2)),
                }),
            }),
            op: BinaryOp::Or,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Integer(3)),
                op: BinaryOp::Greater,
                right: Box::new(Expr::Integer(2)),
            }),
        };

        let result = interpreter.evaluate_expression(&expr, &mut env)?;
        assert_eq!(result, Value::Boolean(true));
        Ok(())
    }

    #[tokio::test]
    async fn test_if_with_non_boolean_condition_error() {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let if_stmt = Statement::If {
            condition: Expr::Integer(42),
            then_body: vec![Statement::FunctionCall {
                name: "write".to_string(),
                args: vec![Expr::String("should not execute".to_string())],
            }],
            else_body: None,
        };

        let result = interpreter
            .execute_statement(&if_stmt, &DummyExternalApi::new(), &mut env)
            .await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("boolean") || error.to_string().contains("condition"));
    }

    #[tokio::test]
    async fn test_parameter_passing_to_functions() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                fn test_params(int a, bool b) {
                    if b {
                        result = a * 2;
                    } else {
                        result = a;
                    }
                    write("01020304");
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let interpreter = Interpreter::new(rig_file.clone());
        let mut env = Env::new();

        let args = vec![Value::Integer(10), Value::Boolean(true)];
        let api = DummyExternalApi::new();
        interpreter
            .execute_command_with_env("test_params", &args, &api, &mut env)
            .await?;

        assert_eq!(api.output.read().len(), 1);
        assert_eq!(api.output.read()[0], "WRITE: [1, 2, 3, 4]");
        Ok(())
    }

    #[test]
    fn test_all_data_formats_in_interpolation() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        let test_cases = vec![
            ("int_lu", 418),
            ("int_ls", 418),
            ("int_bu", 418),
            ("int_bs", 418),
            ("bcd_lu", 418),
            ("bcd_ls", 418),
            ("bcd_bu", 418),
            ("bcd_bs", 418),
            ("text", 418),
        ];

        for (format, value) in test_cases {
            env.set("test_var".to_string(), Value::Integer(value));

            let parts = vec![
                InterpolationPart::Literal(vec![0xFE, 0xFE]),
                InterpolationPart::Variable {
                    name: "test_var".to_string(),
                    format: Some(format.to_string()),
                    length: 4,
                },
                InterpolationPart::Literal(vec![0xFD]),
            ];

            let expr = Expr::StringInterpolation { parts };
            let result = interpreter.evaluate_expression(&expr, &mut env);

            match result {
                Ok(Value::Bytes(_)) => {}
                Ok(_) => {
                    panic!("Expected string result for format {}", format);
                }
                Err(e) if format == "invalid_format" => {
                    assert!(e.to_string().contains("format") || e.to_string().contains("invalid"));
                }
                Err(e) => {
                    panic!("Unexpected error for format {}: {}", format, e);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_string_interpolation_with_invalid_format() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("test_var".to_string(), Value::Integer(418));

        let parts = vec![InterpolationPart::Variable {
            name: "test_var".to_string(),
            format: Some("invalid_format".to_string()),
            length: 4,
        }];

        let expr = Expr::StringInterpolation { parts };
        let result = interpreter.evaluate_expression(&expr, &mut env);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("format") || error.to_string().contains("invalid"));
        Ok(())
    }

    #[test]
    fn test_string_interpolation_zero_length() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("test_var".to_string(), Value::Integer(1));

        let parts = vec![InterpolationPart::Variable {
            name: "test_var".to_string(),
            format: Some("int_lu".to_string()),
            length: 0,
        }];

        let expr = Expr::StringInterpolation { parts };
        let result = interpreter.evaluate_expression(&expr, &mut env);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("too long") || error.to_string().contains("0 bytes"));
        Ok(())
    }

    #[test]
    fn test_string_interpolation_large_numbers() -> Result<()> {
        let interpreter = Interpreter::default();
        let mut env = Env::new();

        env.set("large_num".to_string(), Value::Integer(0x12345678));

        let parts = vec![InterpolationPart::Variable {
            name: "large_num".to_string(),
            format: Some("int_lu".to_string()),
            length: 4,
        }];

        let expr = Expr::StringInterpolation { parts };
        let result = interpreter.evaluate_expression(&expr, &mut env)?;

        match result {
            Value::Bytes(bytes) => {
                assert!(!bytes.is_empty());
            }
            _ => panic!("Expected string result"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_qualified_identifier_enum_access() -> Result<()> {
        let dsl_source = r#"
            version = 1;
            impl Test for Rig {
                enum TestEnum {
                    A = 10,
                    B = 20,
                }
                fn test() {
                    x = TestEnum::A;
                    y = TestEnum::B;
                    write("00");
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let interpreter = Interpreter::new(rig_file.clone());
        let mut env = interpreter.create_env()?;

        let api = DummyExternalApi::new();
        let result = interpreter
            .execute_command_with_env("test", &[], &api, &mut env)
            .await;
        assert!(
            result.is_ok(),
            "Command with qualified identifiers should execute successfully"
        );

        assert_eq!(api.output.read().len(), 1);
        assert_eq!(api.output.read()[0], "WRITE: [0]");
        Ok(())
    }

    #[tokio::test]
    async fn test_undefined_enum_access() -> Result<()> {
        let dsl_source = r#"
            impl Test for Rig {
                enum TestEnum {
                    A = 10,
                }
                fn test() {
                    x = TestEnum::NonExistent;
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;
        let interpreter = Interpreter::new(rig_file.clone());
        let mut env = interpreter.create_env()?;

        let result = interpreter
            .execute_command_with_env("test", &[], &DummyExternalApi::new(), &mut env)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("variant") || error.to_string().contains("NonExistent"));
        Ok(())
    }

    #[tokio::test]
    async fn test_string_interpolation_invalid_format() -> Result<()> {
        use crate::runtime::interpreter::{Interpreter, Value};

        let dsl_source = r#"
            impl Test for Rig {
                fn test() {
                    command = "FEFE{var:invalid_format:2}FD";
                    write(command);
                }
            }
        "#;

        let rig_file = parse_rig_file(dsl_source)?;

        let interpreter = Interpreter::new(rig_file.clone());
        let mut env = interpreter.create_env()?;

        env.set("var".to_string(), Value::Integer(42));

        let result = interpreter
            .execute_command_with_env("test", &[], &DummyExternalApi::new(), &mut env)
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        println!("Error:\n{error}");
        assert!(
            error.to_string().to_lowercase().contains("format")
                || error.to_string().to_lowercase().contains("invalid")
        );
        Ok(())
    }
}
