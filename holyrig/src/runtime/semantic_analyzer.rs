use std::collections::{HashMap, HashSet};
use std::fmt;

use super::SchemaFile;
use super::parser::{BinaryOp, DataType, Expr, InterpolationPart, RigFile, Statement};
use super::parser_errors::{ErrorLevel, ParseError, ParseErrorType, SourcePosition};

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub position: Option<SourcePosition>,
    pub error_type: SemanticErrorType,
}

#[derive(Debug, Clone)]
pub enum SemanticErrorType {
    UndefinedVariable {
        name: String,
    },
    UndefinedFunction {
        name: String,
    },
    UndefinedEnumVariant {
        enum_name: String,
        variant_name: String,
    },
    UndefinedEnum {
        name: String,
    },
    TypeMismatch {
        expected: DataType,
        found: DataType,
        context: String,
    },
    InvalidFunctionArguments {
        function_name: String,
        expected: usize,
        found: usize,
    },
    InvalidFunctionArgumentType {
        function_name: String,
        arg_index: usize,
        expected: String,
        found: String,
    },
    CommandNotInSchema {
        command_name: String,
    },
    ParameterTypeMismatch {
        command_name: String,
        param_name: String,
        expected: DataType,
        found: DataType,
    },
    MissingRequiredParameter {
        command_name: String,
        param_name: String,
    },
    UnknownParameter {
        command_name: String,
        param_name: String,
    },
    DuplicateEnumVariant {
        enum_name: String,
        variant_name: String,
    },
    InvalidBinaryOperation {
        left_type: DataType,
        op: BinaryOp,
        right_type: DataType,
    },
    SchemaVersionMismatch {
        rig_version: String,
        schema_version: u32,
    },
    SchemaTypeMismatch {
        rig_type: String,
        schema_type: String,
    },
    InvalidInterpolationVariable {
        variable_name: String,
        context: String,
    },
    EmptyCommand {
        command_name: String,
    },
    InvalidEnumVariantValue {
        enum_name: String,
        variant_name: String,
        value: u32,
    },
    DivisionByZero {
        context: String,
    },
    InvalidDataFormat {
        format: String,
        context: String,
    },
    InvalidStatusVariable {
        name: String,
        context: String,
    },
    StatusVariableTypeMismatch {
        name: String,
        expected: DataType,
        found: DataType,
    },
    InvalidCast {
        from_type: DataType,
        to_type: DataType,
    },
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error_type {
            SemanticErrorType::UndefinedVariable { name } => {
                write!(f, "Undefined variable '{name}'")
            }
            SemanticErrorType::UndefinedFunction { name } => {
                write!(f, "Undefined function '{name}'")
            }
            SemanticErrorType::UndefinedEnumVariant {
                enum_name,
                variant_name,
            } => {
                write!(
                    f,
                    "Undefined variant '{variant_name}' for enum '{enum_name}'"
                )
            }
            SemanticErrorType::UndefinedEnum { name } => {
                write!(f, "Undefined enum '{name}'")
            }
            SemanticErrorType::TypeMismatch {
                expected,
                found,
                context,
            } => {
                write!(
                    f,
                    "Type mismatch in {context}: expected {expected:?}, found {found:?}"
                )
            }
            SemanticErrorType::InvalidFunctionArguments {
                function_name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Function '{function_name}' expects {expected} arguments, got {found}"
                )
            }
            SemanticErrorType::InvalidFunctionArgumentType {
                function_name,
                arg_index,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Function '{function_name}' argument {arg_index}: expected {expected}, found {found}"
                )
            }
            SemanticErrorType::CommandNotInSchema { command_name } => {
                write!(f, "Command '{command_name}' is not defined in the schema")
            }
            SemanticErrorType::ParameterTypeMismatch {
                command_name,
                param_name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Parameter '{param_name}' in command '{command_name}': expected {expected:?}, found {found:?}"
                )
            }
            SemanticErrorType::MissingRequiredParameter {
                command_name,
                param_name,
            } => {
                write!(
                    f,
                    "Missing required parameter '{param_name}' for command '{command_name}'"
                )
            }
            SemanticErrorType::UnknownParameter {
                command_name,
                param_name,
            } => {
                write!(
                    f,
                    "Unknown parameter '{param_name}' for command '{command_name}'"
                )
            }
            SemanticErrorType::DuplicateEnumVariant {
                enum_name,
                variant_name,
            } => {
                write!(
                    f,
                    "Duplicate variant '{variant_name}' in enum '{enum_name}'"
                )
            }
            SemanticErrorType::InvalidBinaryOperation {
                left_type,
                op,
                right_type,
            } => {
                write!(
                    f,
                    "Invalid binary operation: {left_type:?} {op:?} {right_type:?}"
                )
            }
            SemanticErrorType::SchemaVersionMismatch {
                rig_version,
                schema_version,
            } => {
                write!(
                    f,
                    "Schema version mismatch: rig expects '{rig_version}', schema has {schema_version}"
                )
            }
            SemanticErrorType::SchemaTypeMismatch {
                rig_type,
                schema_type,
            } => {
                write!(
                    f,
                    "Schema type mismatch: rig has '{rig_type}', schema expects '{schema_type}'"
                )
            }
            SemanticErrorType::InvalidInterpolationVariable {
                variable_name,
                context,
            } => {
                write!(
                    f,
                    "Invalid interpolation variable '{variable_name}' in {context}"
                )
            }
            SemanticErrorType::EmptyCommand { command_name } => {
                write!(f, "Command '{command_name}' is empty")
            }
            SemanticErrorType::InvalidEnumVariantValue {
                enum_name,
                variant_name,
                value,
            } => {
                write!(
                    f,
                    "Invalid value '{value}' for enum variant '{variant_name}' in enum '{enum_name}'"
                )
            }
            SemanticErrorType::DivisionByZero { context } => {
                write!(f, "Division by zero in {context}")
            }
            SemanticErrorType::InvalidDataFormat { format, context } => {
                write!(f, "Invalid data format '{format}' in {context}")
            }
            SemanticErrorType::InvalidStatusVariable { name, context } => {
                write!(f, "Invalid status variable '{name}' in {context}")
            }
            SemanticErrorType::StatusVariableTypeMismatch {
                name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Status variable '{name}' type mismatch: expected {expected:?}, found {found:?}"
                )
            }
            SemanticErrorType::InvalidCast { from_type, to_type } => {
                write!(f, "Invalid cast from {from_type:?} to {to_type:?}")
            }
        }
    }
}

impl std::error::Error for SemanticError {}

#[derive(Debug)]
pub struct SemanticAnalyzer {
    schema: SchemaFile,
    builtin_functions: HashSet<String>,
}

impl SemanticAnalyzer {
    pub fn new(schema: SchemaFile) -> Self {
        let mut builtin_functions = HashSet::new();
        builtin_functions.insert("write".to_string());
        builtin_functions.insert("read".to_string());
        builtin_functions.insert("set_var".to_string());
        builtin_functions.insert("error".to_string());

        Self {
            schema,
            builtin_functions,
        }
    }

    pub fn analyze(&self, rig_file: &RigFile) -> Result<(), Vec<SemanticError>> {
        let mut errors = Vec::new();
        let mut context = AnalysisContext::new(&self.schema, rig_file);

        self.validate_schema_compatibility(rig_file, &mut errors);
        self.validate_enums(rig_file, &mut errors, &mut context);
        self.validate_settings(rig_file, &mut errors, &mut context);
        self.validate_impl_block(rig_file, &mut errors, &mut context);
        self.validate_status_block(rig_file, &mut errors, &mut context);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_schema_compatibility(&self, rig_file: &RigFile, errors: &mut Vec<SemanticError>) {
        if rig_file.impl_block.schema != self.schema.name {
            errors.push(SemanticError {
                position: None,
                error_type: SemanticErrorType::SchemaTypeMismatch {
                    rig_type: rig_file.impl_block.schema.clone(),
                    schema_type: self.schema.name.clone(),
                },
            });
        }
    }

    fn validate_enums(
        &self,
        rig_file: &RigFile,
        errors: &mut Vec<SemanticError>,
        context: &mut AnalysisContext,
    ) {
        for enum_def in &rig_file.impl_block.enums {
            let mut seen_variants = HashSet::new();

            for variant_name in enum_def.variants.keys() {
                if !seen_variants.insert(variant_name) {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::DuplicateEnumVariant {
                            enum_name: enum_def.name.clone(),
                            variant_name: variant_name.clone(),
                        },
                    });
                }
            }

            if !self.schema.enums.contains_key(&enum_def.name) {
                errors.push(SemanticError {
                    position: None,
                    error_type: SemanticErrorType::UndefinedEnum {
                        name: enum_def.name.clone(),
                    },
                });
            }

            context.register_enum(
                &enum_def.name,
                &enum_def.variants.keys().cloned().collect::<Vec<_>>(),
            );
        }
    }

    fn validate_settings(
        &self,
        rig_file: &RigFile,
        errors: &mut Vec<SemanticError>,
        context: &mut AnalysisContext,
    ) {
        for (id, expr) in &rig_file.settings.settings {
            match self.infer_expression_type(expr, context) {
                Ok(expr_type) => {
                    context.register_variable(id.as_str(), expr_type);
                }
                Err(expr_errors) => {
                    errors.extend(expr_errors);
                }
            }
        }
    }

    fn validate_impl_block(
        &self,
        rig_file: &RigFile,
        errors: &mut Vec<SemanticError>,
        context: &mut AnalysisContext,
    ) {
        if let Some(init) = &rig_file.impl_block.init {
            for statement in &init.statements {
                if let Err(stmt_errors) = self.validate_statement(statement, context) {
                    errors.extend(stmt_errors);
                }
            }
        }

        if let Some(status) = &rig_file.impl_block.status {
            for statement in &status.statements {
                if let Err(stmt_errors) = self.validate_statement(statement, context) {
                    errors.extend(stmt_errors);
                }
            }
        }

        for (command_name, command) in &rig_file.impl_block.commands {
            self.validate_command(command_name, command, errors, context);
        }
    }

    fn validate_command(
        &self,
        command_name: &str,
        command: &super::parser::Command,
        errors: &mut Vec<SemanticError>,
        context: &mut AnalysisContext,
    ) {
        let schema_params = match self.schema.commands.get(command_name) {
            Some(cmd) => cmd,
            None => {
                errors.push(SemanticError {
                    position: None,
                    error_type: SemanticErrorType::CommandNotInSchema {
                        command_name: command_name.to_string(),
                    },
                });
                return;
            }
        };

        let mut local_context = context.clone();

        for rig_param in &command.parameters {
            let schema_param_type = schema_params
                .iter()
                .find(|schema_param| schema_param.name == rig_param.name)
                .map(|schema_param| &schema_param.param_type);

            match schema_param_type {
                Some(expected_type) => {
                    if &rig_param.param_type != expected_type {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::ParameterTypeMismatch {
                                command_name: command_name.to_string(),
                                param_name: rig_param.name.clone(),
                                expected: expected_type.clone(),
                                found: rig_param.param_type.clone(),
                            },
                        });
                    }
                }
                None => {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::UnknownParameter {
                            command_name: command_name.to_string(),
                            param_name: rig_param.name.clone(),
                        },
                    });
                }
            }

            local_context.register_variable(&rig_param.name, rig_param.param_type.clone());
        }

        for schema_param in schema_params {
            if !command
                .parameters
                .iter()
                .any(|p| p.name == schema_param.name)
            {
                errors.push(SemanticError {
                    position: None,
                    error_type: SemanticErrorType::MissingRequiredParameter {
                        command_name: command_name.to_string(),
                        param_name: schema_param.name.clone(),
                    },
                });
            }
        }

        if command.statements.is_empty() {
            errors.push(SemanticError {
                position: None,
                error_type: SemanticErrorType::EmptyCommand {
                    command_name: command_name.to_string(),
                },
            });
        }

        for statement in &command.statements {
            if let Err(stmt_errors) = self.validate_statement(statement, &mut local_context) {
                errors.extend(stmt_errors);
            }
        }
    }

    fn validate_statement(
        &self,
        statement: &Statement,
        context: &mut AnalysisContext,
    ) -> Result<(), Vec<SemanticError>> {
        let mut errors = Vec::new();

        match statement {
            Statement::Assign(_id, expr) => {
                match self.infer_expression_type(expr, context) {
                    Ok(_expr_type) => {
                        // Assignment is valid - in a real implementation we might want to
                        // track the variable type for future use
                    }
                    Err(expr_errors) => {
                        errors.extend(expr_errors);
                    }
                }
            }
            Statement::FunctionCall { name, args } => {
                self.validate_function_call(name, args, context, &mut errors);
            }
            Statement::If {
                condition,
                then_body,
                else_body,
            } => {
                match self.infer_expression_type(condition, context) {
                    Ok(condition_type) => {
                        if condition_type != DataType::Bool {
                            errors.push(SemanticError {
                                position: None,
                                error_type: SemanticErrorType::TypeMismatch {
                                    expected: DataType::Bool,
                                    found: condition_type,
                                    context: "if condition".to_string(),
                                },
                            });
                        }
                    }
                    Err(expr_errors) => {
                        errors.extend(expr_errors);
                    }
                }

                for stmt in then_body {
                    if let Err(stmt_errors) = self.validate_statement(stmt, context) {
                        errors.extend(stmt_errors);
                    }
                }

                if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        if let Err(stmt_errors) = self.validate_statement(stmt, context) {
                            errors.extend(stmt_errors);
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_function_call(
        &self,
        name: &str,
        args: &[Expr],
        context: &mut AnalysisContext,
        errors: &mut Vec<SemanticError>,
    ) {
        if !self.builtin_functions.contains(name) {
            errors.push(SemanticError {
                position: None,
                error_type: SemanticErrorType::UndefinedFunction {
                    name: name.to_string(),
                },
            });
            return;
        }

        match name {
            "write" => {
                if args.len() != 1 {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidFunctionArguments {
                            function_name: name.to_string(),
                            expected: 1,
                            found: args.len(),
                        },
                    });
                    return;
                }

                match self.infer_expression_type(&args[0], context) {
                    Ok(_) => {
                        if let Expr::StringInterpolation { parts } = &args[0] {
                            self.validate_string_interpolation(parts, context, errors);
                        }
                    }
                    Err(expr_errors) => {
                        errors.extend(expr_errors);
                    }
                }
            }
            "read" => {
                if args.len() != 1 {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidFunctionArguments {
                            function_name: name.to_string(),
                            expected: 1,
                            found: args.len(),
                        },
                    });
                    return;
                }

                match &args[0] {
                    Expr::Bytes(_) => {}
                    Expr::StringInterpolation { parts } => {
                        for part in parts {
                            if let InterpolationPart::Variable { name, .. } = part {
                                context.register_variable(name, DataType::Int);
                            }
                        }
                    }
                    _ => {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::InvalidFunctionArgumentType {
                                function_name: "read".into(),
                                arg_index: 0,
                                expected: "Bytes or StringInterpolation".into(),
                                found: format!("{:?}", args[0]),
                            },
                        });
                    }
                }
            }
            "set_var" => {
                if args.len() != 2 {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidFunctionArguments {
                            function_name: name.to_string(),
                            expected: 2,
                            found: args.len(),
                        },
                    });
                    return;
                }

                if let Err(expr_errors) = self.infer_expression_type(&args[0], context) {
                    errors.extend(expr_errors);
                }

                if let Err(expr_errors) = self.infer_expression_type(&args[1], context) {
                    errors.extend(expr_errors);
                }
            }
            "error" => {
                if args.len() != 1 {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidFunctionArguments {
                            function_name: name.to_string(),
                            expected: 1,
                            found: args.len(),
                        },
                    });
                    return;
                }

                if let Err(expr_errors) = self.infer_expression_type(&args[0], context) {
                    errors.extend(expr_errors);
                }
            }
            _ => {
                // Unknown function, already handled above
            }
        }
    }

    fn validate_string_interpolation(
        &self,
        parts: &[InterpolationPart],
        context: &AnalysisContext,
        errors: &mut Vec<SemanticError>,
    ) {
        for part in parts {
            if let InterpolationPart::Variable {
                name,
                format: _,
                length: _,
            } = part
                && name != "_"
                && !context.has_variable(name)
            {
                errors.push(SemanticError {
                    position: None,
                    error_type: SemanticErrorType::InvalidInterpolationVariable {
                        variable_name: name.clone(),
                        context: "string interpolation".to_string(),
                    },
                });
            }
        }
    }

    fn infer_expression_type(
        &self,
        expr: &Expr,
        context: &AnalysisContext,
    ) -> Result<DataType, Vec<SemanticError>> {
        let mut errors = Vec::new();

        let expr_type = match expr {
            Expr::Integer(_) => DataType::Int,
            Expr::Float(_) => DataType::Float,
            Expr::String(_) => DataType::String,
            Expr::Bytes(_) => DataType::Bytes,
            Expr::Identifier(id) => {
                match context.get_variable_type(id.as_str()) {
                    Some(var_type) => var_type,
                    None => {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::UndefinedVariable {
                                name: id.as_str().to_string(),
                            },
                        });
                        // Default fallback
                        DataType::Int
                    }
                }
            }
            Expr::QualifiedIdentifier(enum_name, variant_name) => {
                if let Some(variants) = context.get_enum_variants(enum_name.as_str()) {
                    if !variants.contains(variant_name.as_str()) {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::UndefinedEnumVariant {
                                enum_name: enum_name.as_str().to_string(),
                                variant_name: variant_name.as_str().to_string(),
                            },
                        });
                    }
                    DataType::Enum(enum_name.as_str().to_string())
                } else {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::UndefinedEnum {
                            name: enum_name.as_str().to_string(),
                        },
                    });
                    // Default fallback
                    DataType::Int
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let left_type = match self.infer_expression_type(left, context) {
                    Ok(t) => t,
                    Err(expr_errors) => {
                        errors.extend(expr_errors);
                        // Default fallback
                        DataType::Int
                    }
                };

                let right_type = match self.infer_expression_type(right, context) {
                    Ok(t) => t,
                    Err(expr_errors) => {
                        errors.extend(expr_errors);
                        // Default fallback
                        DataType::Int
                    }
                };

                if matches!(op, BinaryOp::Divide | BinaryOp::Modulo)
                    && let Expr::Integer(0) = right.as_ref()
                {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::DivisionByZero {
                            context: "binary operation".to_string(),
                        },
                    });
                }

                match self.validate_binary_op(&left_type, op, &right_type) {
                    Ok(result_type) => result_type,
                    Err(error) => {
                        errors.push(*error);
                        DataType::Bool
                    }
                }
            }
            Expr::StringInterpolation { parts } => {
                self.validate_string_interpolation(parts, context, &mut errors);
                DataType::Bytes
            }
            Expr::Cast { expr, target_type } => {
                let expr_type = self.infer_expression_type(expr, context)?;
                if !self.validate_cast(&expr_type, target_type) {
                    errors.push(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidCast {
                            from_type: expr_type,
                            to_type: target_type.clone(),
                        },
                    });
                }
                target_type.clone()
            }
        };

        if errors.is_empty() {
            Ok(expr_type)
        } else {
            Err(errors)
        }
    }

    fn validate_binary_op(
        &self,
        left_type: &DataType,
        op: &BinaryOp,
        right_type: &DataType,
    ) -> Result<DataType, Box<SemanticError>> {
        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Ok(DataType::Int)
                } else {
                    Err(Box::new(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidBinaryOperation {
                            left_type: left_type.clone(),
                            op: op.clone(),
                            right_type: right_type.clone(),
                        },
                    }))
                }
            }

            BinaryOp::Equal | BinaryOp::NotEqual => {
                if left_type == right_type {
                    Ok(DataType::Bool)
                } else {
                    Err(Box::new(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidBinaryOperation {
                            left_type: left_type.clone(),
                            op: op.clone(),
                            right_type: right_type.clone(),
                        },
                    }))
                }
            }

            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Ok(DataType::Bool)
                } else {
                    Err(Box::new(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidBinaryOperation {
                            left_type: left_type.clone(),
                            op: op.clone(),
                            right_type: right_type.clone(),
                        },
                    }))
                }
            }

            BinaryOp::And | BinaryOp::Or => {
                if *left_type == DataType::Bool && *right_type == DataType::Bool {
                    Ok(DataType::Bool)
                } else {
                    Err(Box::new(SemanticError {
                        position: None,
                        error_type: SemanticErrorType::InvalidBinaryOperation {
                            left_type: left_type.clone(),
                            op: op.clone(),
                            right_type: right_type.clone(),
                        },
                    }))
                }
            }
        }
    }

    pub fn analyze_with_advanced_checks(
        &self,
        rig_file: &RigFile,
    ) -> Result<(), Vec<SemanticError>> {
        let mut errors = Vec::new();
        let context = AnalysisContext::new(&self.schema, rig_file);

        if let Err(basic_errors) = self.analyze(rig_file) {
            errors.extend(basic_errors);
        }

        self.validate_interpolation_formats(rig_file, &mut errors, &context);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_interpolation_formats(
        &self,
        rig_file: &RigFile,
        errors: &mut Vec<SemanticError>,
        context: &AnalysisContext,
    ) {
        // Additional validation for string interpolation format specifiers
        // This could check if format specifiers like "bcd_lu:5" are valid
        let _ = (rig_file, errors, context); // Placeholder
    }

    fn validate_status_block(
        &self,
        rig_file: &RigFile,
        errors: &mut Vec<SemanticError>,
        context: &mut AnalysisContext,
    ) {
        if let Some(status) = &rig_file.impl_block.status {
            for statement in &status.statements {
                if let Statement::FunctionCall { name, args } = statement
                    && name == "set_var"
                {
                    if args.len() != 2 {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::InvalidFunctionArguments {
                                function_name: "set_var".to_string(),
                                expected: 2,
                                found: args.len(),
                            },
                        });
                        continue;
                    }

                    let var_name = match &args[0] {
                        Expr::String(name) => name,
                        _ => {
                            errors.push(SemanticError {
                                position: None,
                                error_type: SemanticErrorType::InvalidFunctionArgumentType {
                                    function_name: "set_var".to_string(),
                                    arg_index: 0,
                                    expected: "string literal".to_string(),
                                    found: "non-string expression".to_string(),
                                },
                            });
                            continue;
                        }
                    };

                    if let Some(expected_type) = self.schema.status.get(var_name) {
                        if let Ok(found_type) = self.infer_expression_type(&args[1], context)
                            && &found_type != expected_type
                        {
                            errors.push(SemanticError {
                                position: None,
                                error_type: SemanticErrorType::StatusVariableTypeMismatch {
                                    name: var_name.clone(),
                                    expected: expected_type.clone(),
                                    found: found_type,
                                },
                            });
                        }
                    } else {
                        errors.push(SemanticError {
                            position: None,
                            error_type: SemanticErrorType::InvalidStatusVariable {
                                name: var_name.clone(),
                                context: "status block".to_string(),
                            },
                        });
                    }
                }
            }
        }
    }

    fn validate_cast(&self, from_type: &DataType, to_type: &DataType) -> bool {
        matches!(
            (from_type, to_type),
            (DataType::Int, DataType::Float)
                | (DataType::Float, DataType::Int)
                | (DataType::Bool, DataType::Int)
                | (DataType::Int, DataType::Bool)
                | (DataType::Int, DataType::Enum(_))
                | (DataType::Enum(_), DataType::Int)
        ) || from_type == to_type
    }
}

#[derive(Debug, Clone)]
struct AnalysisContext {
    variables: HashMap<String, DataType>,
    enums: HashMap<String, HashSet<String>>,
}

impl AnalysisContext {
    fn new(_schema: &SchemaFile, rig_file: &RigFile) -> Self {
        let mut context = Self {
            variables: HashMap::new(),
            enums: HashMap::new(),
        };

        // for (enum_name, enum_def) in &schema.enums {
        //     let variants = enum_def.iter().cloned().collect();
        //     context.enums.insert(enum_name.clone(), variants);
        // }

        for enum_def in &rig_file.impl_block.enums {
            let variants = enum_def.variants.keys().cloned().collect();
            context.enums.insert(enum_def.name.clone(), variants);
        }

        context
    }

    fn register_variable(&mut self, name: &str, var_type: DataType) {
        self.variables.insert(name.to_string(), var_type);
    }

    fn register_enum(&mut self, enum_name: &str, variants: &[String]) {
        self.enums
            .insert(enum_name.to_string(), variants.iter().cloned().collect());
    }

    fn get_variable_type(&self, name: &str) -> Option<DataType> {
        self.variables.get(name).cloned()
    }

    fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    fn get_enum_variants(&self, enum_name: &str) -> Option<&HashSet<String>> {
        self.enums.get(enum_name)
    }
}

pub fn semantic_errors_to_parse_errors(
    errors: Vec<SemanticError>,
    source: &str,
) -> Vec<ParseError> {
    errors
        .into_iter()
        .map(|error| ParseError {
            position: error
                .position
                .clone()
                .unwrap_or_else(|| SourcePosition::new(1, 1, 0)),
            error_type: Box::new(ParseErrorType::Semantic {
                message: error.to_string(),
                suggestion: None,
            }),
            source: source.to_string(),
            level: ErrorLevel::Normal,
        })
        .collect()
}

pub fn parse_and_validate_with_schema(
    rig_source: &str,
    schemas: &HashMap<String, SchemaFile>,
) -> Result<RigFile, Vec<ParseError>> {
    let rig_file = super::parser::parse_rig_file(rig_source).map_err(|x| vec![x])?;
    let schema_name = &rig_file.impl_block.schema;
    let schema = schemas.get(schema_name).ok_or_else(|| {
        vec![ParseError {
            position: SourcePosition::new(1, 1, 0),
            error_type: Box::new(ParseErrorType::Semantic {
                message: format!("Unknown schema: {schema_name}"),
                suggestion: None,
            }),
            source: rig_source.to_string(),
            level: ErrorLevel::Normal,
        }]
    })?;

    let analyzer = SemanticAnalyzer::new(schema.clone());
    analyzer
        .analyze(&rig_file)
        .map(|_| rig_file)
        .map_err(|err| semantic_errors_to_parse_errors(err, rig_source))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::runtime::{parser::parse_rig_file, schema_parser::SchemaParameter};

    fn create_test_schema() -> SchemaFile {
        let mut schema = SchemaFile {
            version: 1,
            name: "Transceiver".to_string(),
            enums: BTreeMap::new(),
            commands: BTreeMap::new(),
            status: BTreeMap::new(),
        };

        schema
            .enums
            .insert("Vfo".to_string(), vec!["A".to_string(), "B".to_string()]);

        schema.commands.insert(
            "set_freq".to_string(),
            vec![
                SchemaParameter {
                    param_type: DataType::Int,
                    name: "freq".to_string(),
                },
                SchemaParameter {
                    param_type: DataType::Enum("Vfo".to_string()),
                    name: "target".to_string(),
                },
            ],
        );

        schema
    }

    #[test]
    fn test_undefined_variable() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                fn set_freq(int freq, Vfo target) {
                    a = undefined_var + 1;
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        let result = analyzer.analyze(&rig_file);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            &e.error_type,
            SemanticErrorType::UndefinedVariable { name } if name == "undefined_var"
        )));
    }

    #[test]
    fn test_undefined_function() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                fn set_freq(int freq, Vfo target) {
                    unknown_function();
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        let result = analyzer.analyze(&rig_file);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            &e.error_type,
            SemanticErrorType::UndefinedFunction { name } if name == "unknown_function"
        )));
    }

    #[test]
    fn test_command_not_in_schema() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                fn unknown_command(int param) {
                    write(s"test");
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        let result = analyzer.analyze(&rig_file);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            &e.error_type,
            SemanticErrorType::CommandNotInSchema { command_name } if command_name == "unknown_command"
        )));
    }

    #[test]
    fn test_valid_rig_file() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                enum Vfo {
                    A = 0,
                    B = 1,
                }

                fn set_freq(int freq, Vfo target) {
                    write("test{freq:4}{target:1}");
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        let result = analyzer.analyze(&rig_file);

        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_casts() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                enum Vfo {
                    A = 0,
                    B = 1,
                }

                fn set_freq(int freq, Vfo target) {
                    write("AABBCCDD");
                    x = freq as float;
                    w = target as int;
                    w = (1 == 1) as int;
                    w = 0 as Vfo;
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        analyzer.analyze(&rig_file).unwrap();
    }

    #[test]
    fn test_invalid_casts() {
        let schema = create_test_schema();
        let analyzer = SemanticAnalyzer::new(schema);

        let rig_file_source = r#"
            impl Transceiver for TestRig {
                enum Vfo {
                    A = 0,
                    B = 1,
                }

                fn set_freq(int freq, Vfo target) {
                    x = "AABBCC" as float;
                    w = target as float;
                    w = s"invalid" as Vfo;
                }
            }
        "#;

        let rig_file = parse_rig_file(rig_file_source).unwrap();
        let result = analyzer.analyze(&rig_file);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 3);
        assert!(
            errors
                .iter()
                .all(|e| matches!(&e.error_type, SemanticErrorType::InvalidCast { .. }))
        );
    }
}
