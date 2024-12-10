////////////////////////////////////////////////////////////////////
// test data module
////////////////////////////////////////////////////////////////////

use std::fs::File;

use crate::compiler::Compiler;
use crate::data_types::DataType;
use crate::data_types::DataType::{NumberType, UnionType};
use crate::dataframe::Dataframe;
use crate::dataframe::Dataframe::Disk;
use crate::dataframe_config::DataFrameConfig;
use crate::file_row_collection::FileRowCollection;
use crate::inferences::Inferences;
use crate::interpreter::Interpreter;
use crate::namespaces::Namespace;
use crate::number_kind::NumberKind::{F64Kind, I64Kind};
use crate::numbers::Numbers::{F64Value, U64Value};
use crate::parameter::Parameter;
use crate::rows::Row;
use crate::table_columns::Column;
use crate::table_renderer::TableRenderer;
use crate::typed_values::TypedValue;
use crate::typed_values::TypedValue::*;
use chrono::Utc;
use rand::distributions::Uniform;
use rand::prelude::ThreadRng;
use rand::{thread_rng, Rng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn make_dataframe(database: &str, schema: &str, name: &str, columns: Vec<Parameter>) -> std::io::Result<Dataframe> {
    make_dataframe_ns(Namespace::new(database, schema, name), columns)
}

pub fn make_dataframe_ns(ns: Namespace, columns: Vec<Parameter>) -> std::io::Result<Dataframe> {
    Ok(Disk(FileRowCollection::create_table(&ns, &columns)?))
}

pub fn make_dataframe_config(columns: Vec<Parameter>) -> DataFrameConfig {
    DataFrameConfig::new(columns, Vec::new(), Vec::new())
}

pub fn make_quote(id: usize,
                  symbol: &str,
                  exchange: &str,
                  last_sale: f64) -> Row {
    Row::new(id, vec![
        StringValue(symbol.into()),
        StringValue(exchange.into()),
        Number(F64Value(last_sale))
    ])
}

pub fn make_quote_columns() -> Vec<Column> {
    Column::from_parameters(&make_quote_parameters()).unwrap()
}

pub fn make_quote_parameters() -> Vec<Parameter> {
    vec![
        Parameter::new("symbol", Some("String(8)".into()), None),
        Parameter::new("exchange", Some("String(8)".into()), None),
        Parameter::new("last_sale", Some("f64".into()), None),
    ]
}

pub fn make_scan_quote(
    id: usize,
    symbol: &str,
    exchange: &str,
    last_sale: f64,
    _active: bool,
) -> Row {
    Row::new(id, vec![
        StringValue(symbol.into()),
        StringValue(exchange.into()),
        Number(F64Value(last_sale)),
        Number(U64Value(id as u64)),
        Boolean(_active)
    ])
}

pub fn make_table_file(
    database: &str,
    schema: &str,
    name: &str,
    columns: Vec<Parameter>,
) -> (String, File, Vec<Column>, usize) {
    let table_columns = Column::from_parameters(&columns).unwrap();
    let record_size = Row::compute_record_size(&table_columns);
    let ns = Namespace::new(database, schema, name);
    let file = FileRowCollection::table_file_create(&ns).unwrap();
    (ns.get_table_file_path(), file, table_columns, record_size)
}

pub fn verify_data_type(code: &str, expected: DataType) {
    let model = Compiler::build(code).unwrap();
    assert_eq!(Inferences::infer(&model), expected);
}

pub fn verify_bit_operator(op: &str) {
    verify_data_type(format!("5 {} 9", op).as_str(), NumberType(I64Kind));
    verify_data_type(format!("a {} b", op).as_str(), UnionType(vec![]));
}

pub fn verify_math_operator(op: &str) {
    verify_data_type(format!("5 {} 9", op).as_str(), NumberType(I64Kind));
    verify_data_type(format!("9.4 {} 3.7", op).as_str(), NumberType(F64Kind));
    verify_data_type(format!("a {} b", op).as_str(), UnionType(vec![]));
}

pub fn verify_exact(code: &str, expected: TypedValue) {
    let mut interpreter = Interpreter::new();
    let actual = interpreter.evaluate(code).unwrap();
    assert_eq!(actual, expected);
}

pub fn verify_exact_text(code: &str, expected: &str) {
    let mut interpreter = Interpreter::new();
    let actual = interpreter.evaluate(code).unwrap();
    assert_eq!(actual.to_code(), expected);
}

pub fn verify_exact_json(code: &str, expected: Value) {
    let mut interpreter = Interpreter::new();
    let actual = interpreter.evaluate(code).unwrap();
    assert_eq!(actual.to_json(), expected);
}

pub fn verify_exact_table_with_ids(code: &str, expected: Vec<&str>) {
    let mut interpreter = Interpreter::new();
    let result = interpreter.evaluate(code)
        .unwrap().to_table().unwrap();
    let actual = TableRenderer::from_table_with_ids(&result).unwrap();
    for s in &actual { println!("{}", s) }
    assert_eq!(actual, expected);
}

pub fn verify_exact_table_where(
    mut interpreter: Interpreter,
    code: &str,
    expected: Vec<&str>,
) -> Interpreter {
    let result = interpreter.evaluate(code)
        .unwrap().to_table().unwrap();
    let actual = TableRenderer::from_table_with_ids(&result).unwrap();
    for s in &actual { println!("{}", s) }
    assert_eq!(actual, expected);
    interpreter
}

pub fn verify_outcome(
    mut interpreter: Interpreter,
    code: &str,
    f: fn(std::io::Result<TypedValue>) -> bool,
) -> Interpreter {
    assert!(f(interpreter.evaluate(code)));
    interpreter
}

pub fn verify_when(code: &str, f: fn(TypedValue) -> bool) {
    let mut interpreter = Interpreter::new();
    let actual = interpreter.evaluate(code).unwrap();
    assert!(f(actual));
}

pub fn verify_whence(
    interpreter: Interpreter,
    code: &str,
    f: fn(TypedValue) -> bool,
) -> Interpreter {
    let mut my_interpreter = interpreter;
    let actual = my_interpreter.evaluate(code).unwrap();
    assert!(f(actual));
    my_interpreter
}

pub fn verify_where(mut interpreter: Interpreter, code: &str, expected: TypedValue) -> Interpreter {
    let actual = interpreter.evaluate(code).unwrap();
    assert_eq!(actual, expected);
    interpreter
}

/////////////////////////////////////////////////////////////
//      STOCK QUOTE GENERATION
/////////////////////////////////////////////////////////////

#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct StockQuote {
    pub symbol: String,
    pub exchange: String,
    pub last_sale: f64,
    pub transaction_time: i64,
}

impl StockQuote {
    pub fn generate_quote() -> StockQuote {
        Self::generate_quote_from_symbol_and_exchange(
            Self::generate_random_symbol(),
            Self::generate_random_exchange())
    }

    fn generate_quote_from_symbol_and_exchange(symbol: String, exchange: String) -> StockQuote {
        StockQuote {
            symbol,
            exchange,
            last_sale: Self::generate_random_last_sale(),
            transaction_time: Self::generate_random_transaction_time(),
        }
    }

    fn get_exchange_from_index(exchange_index: usize) -> String {
        let exchanges = ["AMEX", "NASDAQ", "NYSE", "OTCBB", "OTHEROTC"];
        exchanges[exchange_index % exchanges.len()].parse().unwrap()
    }

    fn generate_random_exchange() -> String {
        let mut rng: ThreadRng = thread_rng();
        let exchange_index = rng.next_u32();
        Self::get_exchange_from_index(exchange_index as usize)
    }

    fn generate_random_last_sale() -> f64 {
        let mut rng: ThreadRng = thread_rng();
        400.0 * rng.sample(Uniform::new(0.0, 1.0))
    }

    fn generate_random_symbol() -> String {
        let mut rng: ThreadRng = thread_rng();
        let size: usize = rng.sample(Uniform::new(3, 5));
        (0..size)
            .map(|_| rng.gen_range(b'A'..=b'Z') as char)
            .collect()
    }

    fn generate_random_transaction_time() -> i64 {
        let mut rng: ThreadRng = thread_rng();
        Utc::now().timestamp_millis() - (rng.sample(Uniform::new(0, 10000)) as i64)
    }
}

#[cfg(test)]
mod tests {}