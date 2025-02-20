#![warn(dead_code)]
////////////////////////////////////////////////////////////////////
// DataType class
////////////////////////////////////////////////////////////////////

use crate::byte_code_compiler::ByteCodeCompiler;
use crate::compiler::Compiler;
use crate::data_types::DataType::*;
use crate::dataframe::Dataframe::Model;
use crate::errors::Errors::{Exact, Syntax, TypeMismatch};
use crate::errors::TypeMismatchErrors::{ArgumentsMismatched, UnrecognizedTypeName, UnsupportedType};
use crate::errors::{throw, Errors};
use crate::expression::Expression;
use crate::expression::Expression::{ArrayExpression, AsValue, FnExpression, FunctionCall, Literal, SetVariable, TupleExpression, Variable};
use crate::field::FieldMetadata;
use crate::model_row_collection::ModelRowCollection;
use crate::number_kind::NumberKind;
use crate::number_kind::NumberKind::*;
use crate::numbers::Numbers;
use crate::numbers::Numbers::I32Value;
use crate::parameter::Parameter;
use crate::platform::PlatformOps;
use crate::row_collection::RowCollection;
use crate::sequences::Array;
use crate::structures::Structures::Hard;
use crate::structures::{HardStructure, Structure};
use crate::typed_values::TypedValue;
use crate::typed_values::TypedValue::{ArrayValue, Binary, Boolean, ErrorValue, Function, Null, Number, PlatformOp, StringValue, Structured, TableValue, TupleValue, Undefined, ASCII};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::ops::Deref;

const PTR_LEN: usize = 8;

/// Represents an Oxide-native datatype
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DataType {
    ArrayType(usize),
    ASCIIType(usize),
    BinaryType(usize),
    BooleanType,
    EnumType(Vec<Parameter>),
    ErrorType,
    FunctionType(Vec<Parameter>, Box<DataType>),
    Indeterminate,
    NumberType(NumberKind),
    PlatformOpsType(PlatformOps),
    StringType(usize),
    StructureType(Vec<Parameter>),
    TableType(Vec<Parameter>, usize),
    TupleType(Vec<DataType>),
    VaryingType(Vec<DataType>), // Polymorphic
}

impl DataType {
    ////////////////////////////////////////////////////////////////////
    //  STATIC METHODS
    ////////////////////////////////////////////////////////////////////

    /// deciphers a datatype from an expression (e.g. "String" | "String(20)")
    pub fn decipher_type(model: &Expression) -> std::io::Result<DataType> {
        fn expect_size(args: &Vec<Expression>, f: fn(usize) -> DataType) -> std::io::Result<DataType> {
            match args.as_slice() {
                [] => Ok(f(0)),
                [Literal(Number(n))] => Ok(f(n.to_usize())),
                [other] => throw(Syntax(other.to_code())),
                other => throw(TypeMismatch(ArgumentsMismatched(1, other.len())))
            }
        }
        fn expect_type(args: &Vec<Expression>, f: fn(DataType) -> DataType) -> std::io::Result<DataType> {
            match args.as_slice() {
                [item] => Ok(f(decode_model(item)?)),
                other => throw(TypeMismatch(ArgumentsMismatched(1, other.len())))
            }
        }
        fn expect_params(args: &Vec<Expression>, f: fn(Vec<Parameter>) -> DataType) -> std::io::Result<DataType> {
            let mut params: Vec<Parameter> = vec![];
            for arg in args {
                let param = match arg {
                    AsValue(name, model) => Parameter::new(name, decode_model(model)?),
                    SetVariable(name, expr) => Parameter::from_tuple(name, expr.to_pure()?),
                    Variable(name) => Parameter::build(name),
                    other => return throw(Syntax(other.to_code()))
                };
                params.push(param);
            }
            Ok(f(params))
        }
        fn decode_model_array(items: &Vec<Expression>) -> std::io::Result<DataType> {
            let mut kinds = vec![];
            for item in items {
                match decode_model(item) {
                    Ok(kind) => kinds.push(kind),
                    Err(err) => return throw(Exact(err.to_string()))
                }
            }
            Ok(ArrayType(kinds.len()))
        }
        fn decode_model_function_call(fx: &Expression, args: &Vec<Expression>) -> std::io::Result<DataType> {
            match fx {
                Variable(name) =>
                    match name.as_str() {
                        "Array" => expect_size(args, |size| ArrayType(size)),
                        "ASCII" => expect_size(args, |size| ASCIIType(size)),
                        "Binary" => expect_size(args, |size| BinaryType(size)),
                        "Enum" => expect_params(args, |params| EnumType(params)),
                        "fn" => expect_params(args, |params| FunctionType(params, Box::from(Indeterminate))),
                        "String" => expect_size(args, |size| StringType(size)),
                        "Struct" => expect_params(args, |params| StructureType(params)),
                        "Table" => expect_params(args, |params| TableType(params, 0)),
                        type_name => throw(Syntax(type_name.into()))
                    }
                other => throw(Syntax(other.to_code()))
            }
        }
        fn decode_model_tuple(items: &Vec<Expression>) -> std::io::Result<DataType> {
            let mut kinds = vec![];
            for item in items {
                match decode_model(item) {
                    Ok(kind) => kinds.push(kind),
                    Err(err) => return throw(Exact(err.to_string()))
                }
            }
            Ok(TupleType(kinds))
        }
        fn decode_model_variable(name: &str) -> std::io::Result<DataType> {
            match name {
                "Ack" => Ok(NumberType(AckKind)),
                "Boolean" => Ok(BooleanType),
                "Date" => Ok(NumberType(DateKind)),
                "Enum" => Ok(EnumType(vec![])),
                "Error" => Ok(ErrorType),
                "f32" => Ok(NumberType(F32Kind)),
                "f64" => Ok(NumberType(F64Kind)),
                "Fn" => Ok(FunctionType(vec![], Box::new(Indeterminate))),
                "i8" => Ok(NumberType(I8Kind)),
                "i16" => Ok(NumberType(I16Kind)),
                "i32" => Ok(NumberType(I32Kind)),
                "i64" => Ok(NumberType(I64Kind)),
                "i128" => Ok(NumberType(I128Kind)),
                "RowId" => Ok(NumberType(RowIdKind)),
                "RowsAffected" => Ok(NumberType(RowsAffectedKind)),
                "String" => Ok(StringType(0)),
                "Struct" => Ok(StructureType(vec![])),
                "Table" => Ok(TableType(vec![], 0)),
                "u8" => Ok(NumberType(U8Kind)),
                "u16" => Ok(NumberType(U16Kind)),
                "u32" => Ok(NumberType(U32Kind)),
                "u64" => Ok(NumberType(U64Kind)),
                "u128" => Ok(NumberType(U128Kind)),
                type_name => throw(TypeMismatch(UnrecognizedTypeName(type_name.to_string())))
            }
        }
        fn decode_model(model: &Expression) -> std::io::Result<DataType> {
            match model {
                // e.g. [String, String, f64]
                ArrayExpression(params) => decode_model_array(params),
                // e.g. fn(a, b, c)
                FnExpression { params, returns, .. } =>
                    Ok(FunctionType(params.clone(), Box::from(returns.clone()))),
                // e.g. String(80)
                FunctionCall { fx, args } =>
                    decode_model_function_call(fx, args),
                // e.g. Ack
                Literal(Number(Numbers::Ack)) => Ok(NumberType(AckKind)),
                // e.g. Structure(symbol: String, exchange: String, last_sale: f64)
                Literal(Structured(s)) => Ok(StructureType(s.get_parameters())),
                // e.g. (f64, f64, f64)
                TupleExpression(params) => decode_model_tuple(params),
                // e.g. i64
                Variable(name) => decode_model_variable(name),
                other => throw(Syntax(format!("{:?}", other)))
            }
        }
        decode_model(model)
    }

    /// decodes the typed value based on the supplied data type and buffer
    pub fn decode(&self, buffer: &Vec<u8>, offset: usize) -> TypedValue {
        match self {
            ArrayType(..) => ArrayValue(Array::new()),
            BinaryType(..) => Binary(Vec::new()),
            BooleanType => ByteCodeCompiler::decode_u8(buffer, offset, |b| Boolean(b == 1)),
            ErrorType => ErrorValue(Exact(ByteCodeCompiler::decode_string(buffer, offset, 255).to_string())),
            NumberType(kind) => Number(kind.decode(buffer, offset)),
            PlatformOpsType(pf) => PlatformOp(pf.to_owned()),
            StringType(size) => StringValue(ByteCodeCompiler::decode_string(buffer, offset, *size).to_string()),
            StructureType(params) => Structured(Hard(HardStructure::from_parameters(params.to_vec()))),
            TableType(columns, ..) => TableValue(Model(ModelRowCollection::from_parameters(columns))),
            _ => ByteCodeCompiler::decode_value(&buffer[offset..].to_vec())
        }
    }

    /// decodes the typed value based on the supplied data type and buffer
    pub fn decode_bcc(&self, bcc: &mut ByteCodeCompiler) -> std::io::Result<TypedValue> {
        let tv = match self {
            ArrayType(..) => ArrayValue(Array::from(bcc.next_array()?)),
            ASCIIType(..) => ASCII(bcc.next_clob()),
            BinaryType(..) => Binary(bcc.next_blob()),
            BooleanType => Boolean(bcc.next_bool()),
            EnumType(labels) => {
                let index = bcc.next_u32() as usize;
                StringValue(labels[index].get_name().to_string())
            }
            ErrorType => ErrorValue(Exact(bcc.next_string())),
            FunctionType(columns, returns) => Function {
                params: columns.to_owned(),
                body: Box::new(ByteCodeCompiler::disassemble(bcc)?),
                returns: returns.deref().clone(),
            },
            NumberType(kind) => Number(kind.decode_buffer(bcc)?),
            PlatformOpsType(pf) => PlatformOp(pf.to_owned()),
            StringType(..) => StringValue(bcc.next_string()),
            StructureType(params) => Structured(Hard(bcc.next_struct_with_parameters(params.to_vec())?)),
            TableType(columns, ..) => TableValue(Model(bcc.next_table_with_columns(columns)?)),
            TupleType(..) => TupleValue(bcc.next_array()?),
            VaryingType(..) => bcc.next_value()?,
            DataType::Indeterminate => Undefined,
        };
        Ok(tv)
    }

    pub fn decode_field_value(&self, buffer: &Vec<u8>, offset: usize) -> TypedValue {
        let metadata = FieldMetadata::decode(buffer[offset]);
        if metadata.is_active {
            self.decode(buffer, offset + 1)
        } else { Null }
    }

    pub fn decode_field_value_bcc(&self, bcc: &mut ByteCodeCompiler, offset: usize) -> std::io::Result<TypedValue> {
        let metadata: FieldMetadata = FieldMetadata::decode(bcc[offset]);
        let value: TypedValue = if metadata.is_active {
            self.decode_bcc(bcc)?
        } else { Null };
        Ok(value)
    }

    pub fn encode(&self, value: &TypedValue) -> std::io::Result<Vec<u8>> {
        match self {
            DataType::ArrayType(_) => match value {
                ArrayValue(_) => Ok(value.encode()),
                z => throw(TypeMismatch(UnsupportedType(self.clone(), z.get_type())))
            }
            DataType::BinaryType(_) => Ok(value.encode()),
            DataType::ASCIIType(_) => Ok(value.encode()),
            DataType::BooleanType => Ok(value.encode()),
            DataType::EnumType(_) => Ok(value.encode()),
            DataType::ErrorType => Ok(value.encode()),
            DataType::FunctionType(..) => Ok(value.encode()),
            DataType::NumberType(_) => Ok(value.encode()),
            DataType::PlatformOpsType(_) => Ok(value.encode()),
            DataType::StringType(_) => Ok(value.encode()),
            DataType::StructureType(_) => Ok(value.encode()),
            DataType::TableType(..) =>
                match value.to_table_value() {
                    TableValue(df) => Ok(ByteCodeCompiler::encode_df(&df)),
                    z => throw(TypeMismatch(UnsupportedType(self.clone(), z.get_type())))
                },
            _ => Ok(value.encode()),
        }
    }

    pub fn encode_field(
        &self,
        value: &TypedValue,
        metadata: &FieldMetadata,
        capacity: usize,
    ) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::with_capacity(capacity);
        buf.push(metadata.encode());
        buf.extend(self.encode(value).unwrap_or_else(|_| vec![]));
        buf.resize(capacity, 0u8);
        buf
    }

    /// parses a datatype expression (e.g. "String(20)")
    pub fn from_str(param_type: &str) -> std::io::Result<DataType> {
        let model = Compiler::build(param_type)?;
        Self::decipher_type(&model)
    }

    pub fn get_type_names() -> Vec<String> {
        vec![
            "Array", "ASCII", "Binary", "Boolean", "Date", "Enum", "Error", "Fn",
            "String", "Struct", "Table", //"Ack", "RowId", "RowsAffected",
            "f32", "f64", "i8", "i16", "i32", "i64", "i128",
            "u8", "u16", "u32", "u64", "u128", "UUID",
        ].iter().map(|s| s.to_string()).collect()
    }

    ////////////////////////////////////////////////////////////////////
    //  INSTANCE METHODS
    ////////////////////////////////////////////////////////////////////

    /// computes and returns the maximum physical size of the value of this datatype
    pub fn compute_fixed_size(&self) -> usize {
        use crate::data_types::DataType::*;
        let width: usize = match self {
            ArrayType(size) => *size,
            ASCIIType(size) => match size {
                size => *size + size.to_be_bytes().len(),
                0 => PTR_LEN
            },
            BinaryType(size) => *size,
            BooleanType => 1,
            EnumType(..) => 2,
            ErrorType => 256,
            FunctionType(columns, ..) => columns.len() * 8,
            Indeterminate => 8,
            NumberType(nk) => nk.compute_fixed_size(),
            PlatformOpsType(..) => 4,
            StringType(size) => match size {
                size => *size + size.to_be_bytes().len(),
                0 => PTR_LEN
            },
            StructureType(columns) => columns.len() * 8,
            TableType(columns, ..) => columns.len() * 8,
            TupleType(types) => types.iter().map(|t| t.compute_fixed_size()).sum(),
            VaryingType(dts) => dts.iter()
                .map(|t| t.compute_fixed_size())
                .max().unwrap_or(0),
        };
        width + 1 // +1 for field metadata
    }

    pub fn get_default_value(&self) -> TypedValue {
        match self {
            ArrayType(..) => ArrayValue(Array::new()),
            ASCIIType(..) => ASCII(vec![]),
            BinaryType(..) => Binary(vec![]),
            BooleanType => Boolean(false),
            EnumType(..) => Number(I32Value(0)),
            ErrorType => ErrorValue(Errors::Empty),
            FunctionType(params, returns) => Function {
                params: params.to_vec(),
                body: Box::new(Literal(returns.get_default_value())),
                returns: returns.deref().clone(),
            },
            Indeterminate => TypedValue::Null,
            NumberType(kind) => Number(kind.get_default_value()),
            PlatformOpsType(kind) => PlatformOp(kind.clone()),
            StringType(..) => StringValue(String::new()),
            StructureType(params) =>
                Structured(Hard(HardStructure::from_parameters(params.to_vec()))),
            TableType(params, _) =>
                TableValue(Model(ModelRowCollection::from_parameters(params))),
            TupleType(dts) => TupleValue(dts.iter()
                .map(|dt| dt.get_default_value()).collect()),
            VaryingType(dts) => dts.first()
                .map(|dt| dt.get_default_value())
                .unwrap_or(TypedValue::Null),
        }
    }

    pub fn render(types: &Vec<DataType>) -> String {
        Self::render_f(types, |t| t.to_code())
    }

    pub fn render_f(types: &Vec<DataType>, f: fn(&DataType) -> String) -> String {
        types.iter().map(|dt| f(dt))
            .collect::<Vec<String>>()
            .join(", ")
    }

    pub fn to_code(&self) -> String {
        fn parameterized(name: &str, params: &Vec<Parameter>, is_enum: bool) -> String {
            match params.len() {
                0 => name.to_string(),
                _ => format!("{name}({})", if is_enum {
                    Parameter::render_f(params, |p| p.to_code_enum())
                } else {
                    Parameter::render(params)
                })
            }
        }
        fn sized(name: &str, size: usize) -> String {
            match size {
                0 => name.to_string(),
                n => format!("{name}({n})"),
            }
        }
        fn typed(name: &str, params: &Vec<DataType>) -> String {
            match params.len() {
                0 => name.to_string(),
                _ => format!("{name}({})", DataType::render(params))
            }
        }
        match self {
            ArrayType(size) => sized("Array", *size),
            ASCIIType(size) => sized("ASCII", *size),
            BinaryType(size) => sized("Binary", *size), //UTF8
            BooleanType => "Boolean".into(),
            EnumType(labels) => parameterized("Enum", labels, true),
            ErrorType => "Error".into(),
            FunctionType(params, returns) =>
                format!("fn({}){}", Parameter::render(params),
                        match returns.to_code() {
                            s if !s.is_empty() => format!(": {}", s),
                            _ => String::new()
                        }),
            Indeterminate => String::new(),
            NumberType(nk) => nk.get_type_name(),
            PlatformOpsType(pf) => pf.to_code(),
            StringType(size) => sized("String", *size),
            StructureType(params) => parameterized("Struct", params, false),
            TableType(params, ..) => parameterized("Table", params, false),
            TupleType(types) => typed("", types),
            VaryingType(dts) => dts.iter()
                .map(|dt| dt.to_code())
                .collect::<Vec<_>>().join("|"),
        }
    }

    pub fn to_type_declaration(&self) -> Option<String> {
        match self.to_code() {
            s if s.is_empty() => None,
            s => Some(s)
        }
    }
}

impl Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_code())
    }
}

/// Unit tests
#[cfg(test)]
mod tests {
    /// Experimental Unit tests
    mod defaults_tests {
        use crate::data_types::DataType::*;
        use crate::dataframe::Dataframe::Model;
        use crate::model_row_collection::ModelRowCollection;
        use crate::number_kind::NumberKind::*;
        use crate::numbers::Numbers::*;
        use crate::testdata::{make_quote_columns, make_quote_parameters};
        use crate::typed_values::TypedValue::*;

        #[test]
        fn test_get_default_value_ascii() {
            assert!(matches!(
                ASCIIType(128).get_default_value(),
                ASCII(..)
            ));
        }

        #[test]
        fn test_get_default_value_binary() {
            assert!(matches!(
                BinaryType(128).get_default_value(),
                Binary(..)
            ));
        }

        #[test]
        fn test_get_default_value_date() {
            assert!(matches!(
                NumberType(DateKind).get_default_value(),
                Number(DateValue(..))
            ));
        }

        #[test]
        fn test_get_default_value_enum() {
            assert!(matches!(
                EnumType(make_quote_parameters()).get_default_value(),
                Number(I32Value(0))
            ));
        }

        #[test]
        fn test_get_default_value_i8() {
            assert_eq!(NumberType(I8Kind).get_default_value(), Number(I8Value(0)));
        }

        #[test]
        fn test_get_default_value_i16() {
            assert_eq!(NumberType(I16Kind).get_default_value(), Number(I16Value(0)));
        }

        #[test]
        fn test_get_default_value_i32() {
            assert_eq!(NumberType(I32Kind).get_default_value(), Number(I32Value(0)));
        }

        #[test]
        fn test_get_default_value_i64() {
            assert_eq!(NumberType(I64Kind).get_default_value(), Number(I64Value(0)));
        }

        #[test]
        fn test_get_default_value_i128() {
            assert_eq!(NumberType(I128Kind).get_default_value(), Number(I128Value(0)));
        }

        #[test]
        fn test_get_default_value_u8() {
            assert_eq!(NumberType(U8Kind).get_default_value(), Number(U8Value(0)));
        }

        #[test]
        fn test_get_default_value_u16() {
            assert_eq!(NumberType(U16Kind).get_default_value(), Number(U16Value(0)));
        }

        #[test]
        fn test_get_default_value_u32() {
            assert_eq!(NumberType(U32Kind).get_default_value(), Number(U32Value(0)));
        }

        #[test]
        fn test_get_default_value_u64() {
            assert_eq!(NumberType(U64Kind).get_default_value(), Number(U64Value(0)));
        }

        #[test]
        fn test_get_default_value_u128() {
            assert_eq!(NumberType(U128Kind).get_default_value(), Number(U128Value(0)));
        }

        #[test]
        fn test_get_default_value_uuid() {
            assert!(matches!(
                NumberType(UUIDKind).get_default_value(),
                Number(UUIDValue(..))
            ));
        }

        #[test]
        fn test_get_default_value_string() {
            assert_eq!(StringType(0).get_default_value(), StringValue(String::new()));
        }

        #[test]
        fn test_get_default_value_table() {
            assert_eq!(
                TableType(make_quote_parameters(), 0).get_default_value(),
                TableValue(Model(ModelRowCollection::new(make_quote_columns()))));
        }
    }

    /// Core Unit tests
    mod core_tests {
        use crate::data_types::DataType;
        use crate::data_types::DataType::*;
        use crate::number_kind::NumberKind::*;
        use crate::numbers::Numbers::I64Value;
        use crate::parameter::Parameter;
        use crate::testdata::make_quote_parameters;
        use crate::typed_values::TypedValue::Number;

        #[test]
        fn test_array() {
            verify_type_construction("Array(12)", ArrayType(12));
        }

        #[test]
        fn test_binary() {
            verify_type_construction("Binary(5566)", BinaryType(5566));
        }

        #[test]
        fn test_ascii() {
            verify_type_construction("ASCII(1000)", ASCIIType(1000));
        }

        #[test]
        fn test_boolean() {
            verify_type_construction("Boolean", BooleanType);
        }

        #[test]
        fn test_date() {
            verify_type_construction("Date", NumberType(DateKind));
        }

        #[test]
        fn test_enums_0() {
            verify_type_construction(
                "Enum(A, B, C)",
                EnumType(vec![
                    Parameter::build("A"),
                    Parameter::build("B"),
                    Parameter::build("C"),
                ]));
        }

        #[test]
        fn test_enums_1() {
            verify_type_construction(
                "Enum(AMEX := 1, NASDAQ := 2, NYSE := 3, OTCBB := 4)",
                EnumType(vec![
                    Parameter::with_default("AMEX", NumberType(I64Kind), Number(I64Value(1))),
                    Parameter::with_default("NASDAQ", NumberType(I64Kind), Number(I64Value(2))),
                    Parameter::with_default("NYSE", NumberType(I64Kind), Number(I64Value(3))),
                    Parameter::with_default("OTCBB", NumberType(I64Kind), Number(I64Value(4))),
                ]));
        }

        #[test]
        fn test_f32() {
            verify_type_construction("f32", NumberType(F32Kind));
        }

        #[test]
        fn test_f64() {
            verify_type_construction("f64", NumberType(F64Kind));
        }

        #[test]
        fn test_fn() {
            verify_type_construction(
                "fn(symbol: String(8), exchange: String(8), last_sale: f64)",
                FunctionType(make_quote_parameters(), Box::from(Indeterminate)));
        }

        #[test]
        fn test_i8() {
            verify_type_construction("i8", NumberType(I8Kind));
        }

        #[test]
        fn test_i16() {
            verify_type_construction("i16", NumberType(I16Kind));
        }

        #[test]
        fn test_i32() {
            verify_type_construction("i32", NumberType(I32Kind));
        }

        #[test]
        fn test_i64() {
            verify_type_construction("i64", NumberType(I64Kind));
        }

        #[test]
        fn test_i128() {
            verify_type_construction("i128", NumberType(I128Kind));
        }

        #[test]
        fn test_tuple_3() {
            verify_type_construction("(i64, i64, i64)", TupleType(vec![
                NumberType(I64Kind),
                NumberType(I64Kind),
                NumberType(I64Kind),
            ]));
        }

        #[test]
        fn test_outcome() {
            verify_type_construction("RowId", NumberType(RowIdKind));
            verify_type_construction("RowsAffected", NumberType(RowsAffectedKind));
            verify_type_construction("Ack", NumberType(AckKind));
        }

        #[test]
        fn test_string() {
            verify_type_construction("String", StringType(0));
            verify_type_construction("String(10)", StringType(10));
        }

        #[test]
        fn test_struct() {
            verify_type_construction(
                "Struct(symbol: String(8), exchange: String(8), last_sale: f64)",
                StructureType(make_quote_parameters()));
        }

        #[test]
        fn test_table() {
            verify_type_construction(
                "Table(symbol: String(8), exchange: String(8), last_sale: f64)",
                TableType(make_quote_parameters(), 0));
        }

        #[test]
        fn test_u8() {
            verify_type_construction("u8", NumberType(U8Kind));
        }

        #[test]
        fn test_u16() {
            verify_type_construction("u16", NumberType(U16Kind));
        }

        #[test]
        fn test_u32() {
            verify_type_construction("u32", NumberType(U32Kind));
        }

        #[test]
        fn test_u64() {
            verify_type_construction("u64", NumberType(U64Kind));
        }

        #[test]
        fn test_u128() {
            verify_type_construction("u128", NumberType(U128Kind));
        }

        fn verify_type_construction(type_decl: &str, data_type: DataType) {
            let dt: DataType = DataType::from_str(type_decl)
                .expect(format!("Failed to parse type {}", data_type).as_str());
            assert_eq!(dt, data_type);
            assert_eq!(data_type.to_code(), type_decl);
            assert_eq!(format!("{}", data_type), type_decl.to_string())
        }
    }
}