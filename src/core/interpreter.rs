////////////////////////////////////////////////////////////////////
// Interpreter class
////////////////////////////////////////////////////////////////////

use serde::{Deserialize, Serialize};

use crate::compiler::Compiler;
use crate::machine::Machine;
use crate::typed_values::TypedValue;

/// Represents the Oxide language interpreter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interpreter {
    machine: Machine,
}

impl Interpreter {

    ////////////////////////////////////////////////////////////////
    // static methods
    ////////////////////////////////////////////////////////////////

    /// Constructs a new Interpreter
    pub fn new() -> Self {
        Self { machine: Machine::new_platform_full() }
    }

    ////////////////////////////////////////////////////////////////
    // instance methods
    ////////////////////////////////////////////////////////////////

    /// Executes the supplied source code returning the result of the evaluation
    pub(crate) fn evaluate(&mut self, source_code: &str) -> std::io::Result<TypedValue> {
        let opcode = Compiler::compile_script(source_code)?;
        let (machine, result) = self.machine.evaluate(&opcode)?;
        self.machine = machine;
        Ok(result)
    }

    /// Executes the supplied source code returning the result of the evaluation
    pub async fn evaluate_async(&mut self, source_code: &str) -> std::io::Result<TypedValue> {
        let opcode = Compiler::compile_script(source_code)?;
        let (machine, result) = self.machine.evaluate_async(&opcode).await?;
        self.machine = machine;
        Ok(result)
    }

    /// Sets the value of a variable
    pub fn with_variable(&mut self, name: &str, value: TypedValue) {
        self.machine = self.machine.with_variable(name, value);
    }
}

/// Unit tests
#[cfg(test)]
mod tests {
    use crate::backdoor::BackDoorKey;
    use crate::errors::Errors::{Exact, StringExpected, TypeMismatch};
    use crate::expression::Expression::*;
    use crate::interpreter::Interpreter;
    use crate::machine::{MAJOR_VERSION, MINOR_VERSION};
    use crate::model_row_collection::ModelRowCollection;
    use crate::numbers::NumberValue::{F64Value, I32Value, I64Value, NaNValue, U128Value};
    use crate::outcomes::Outcomes::{Ack, RowsAffected};
    use crate::row_collection::RowCollection;
    use crate::rows::Row;
    use crate::structures::*;
    use crate::table_columns::Column;
    use crate::table_renderer::TableRenderer;
    use crate::testdata::*;
    use crate::typed_values::TypedValue;
    use crate::typed_values::TypedValue::*;

    #[test]
    fn test_arrays() {
        use crate::numbers::NumberValue::*;
        verify_exact("[0, 1, 3, 5]", Array(vec![
            Number(I64Value(0)), Number(I64Value(1)),
            Number(I64Value(3)), Number(I64Value(5)),
        ]));
        verify_exact("[0, 1, 3, 5][2]", Number(I64Value(3)));
    }

    #[test]
    fn test_basic_state_manipulation() {
        use crate::numbers::NumberValue::*;
        let mut interpreter = Interpreter::new();
        assert_eq!(interpreter.evaluate("x := 5").unwrap(), Outcome(Ack));
        assert_eq!(interpreter.evaluate("$x").unwrap(), Number(I64Value(5)));
        assert_eq!(interpreter.evaluate("-x").unwrap(), Number(I64Value(-5)));
        assert_eq!(interpreter.evaluate("x¡").unwrap(), Number(U128Value(120)));
        assert_eq!(interpreter.evaluate("x := x + 1").unwrap(), Outcome(Ack));
        assert_eq!(interpreter.evaluate("x").unwrap(), Number(I64Value(6)));
        assert_eq!(interpreter.evaluate("x < 7").unwrap(), Boolean(true));
        assert_eq!(interpreter.evaluate("x := x ** 2").unwrap(), Outcome(Ack));
        assert_eq!(interpreter.evaluate("x").unwrap(), Number(F64Value(36.)));
        assert_eq!(interpreter.evaluate("x / 0").unwrap(), Number(NaNValue));
        assert_eq!(interpreter.evaluate("x := x - 1").unwrap(), Outcome(Ack));
        assert_eq!(interpreter.evaluate("x % 5").unwrap(), Number(F64Value(0.)));
        assert_eq!(interpreter.evaluate("x < 35").unwrap(), Boolean(false));
        assert_eq!(interpreter.evaluate("x >= 35").unwrap(), Boolean(true));
    }

    #[test]
    fn test_platform_functions_io() {
        verify_exact(r#"io::stderr("Goodbye Cruel World")"#, Outcome(Ack));
        verify_exact(r#"io::stdout("Hello World")"#, Outcome(Ack));
    }

    #[test]
    fn test_platform_functions_os_system_call() {
        verify_exact(r#"
            create table ns("interpreter.os.call") (
                symbol: String(8),
                exchange: String(8),
                last_sale: f64
            )
            os::call("chmod", "777", "oxide_db")
        "#, StringValue(String::new()))
    }

    #[test]
    fn test_platform_functions_str() {
        verify_exact(r#"
            str::format("This {} the {}", "is", "way")
        "#, StringValue("This is the way".into()));

        verify_exact(r#"
            str::to_string(125.75)
        "#, StringValue("125.75".into()));
    }

    #[test]
    fn test_platform_functions_str_left() {
        // test valid case 1 (positive)
        verify_exact(r#"
            str::left('Hello World', 5)
        "#, StringValue("Hello".into()));

        // test valid case 2 (negative)
        verify_exact(r#"
            str::left('Hello World', -5)
        "#, StringValue("World".into()));

        // test the invalid case
        verify_exact(r#"
            str::left(12345, 5)
        "#, Undefined)
    }

    #[test]
    fn test_platform_functions_str_right() {
        let mut interpreter = Interpreter::new();

        // test valid case 1 (positive)
        let result = interpreter.evaluate(r#"
            str::right('Hello World', 5)
        "#).unwrap();
        assert_eq!(result, StringValue("World".into()));

        // test valid case 2 (negative)
        let result = interpreter.evaluate(r#"
            str::right('Hello World', -5)
        "#).unwrap();
        assert_eq!(result, StringValue("Hello".into()));

        // test the invalid case
        let result = interpreter.evaluate(r#"
            str::right(7779311, 5)
        "#).unwrap();
        assert_eq!(result, Undefined)
    }

    #[test]
    fn test_platform_functions_str_substring() {
        // test the valid case
        verify_exact(r#"
            str::substring('Hello World', 0, 5)
        "#, StringValue("Hello".into()));

        // test the invalid case
        verify_exact(r#"
            str::substring(8888, 0, 5)
        "#, Undefined)
    }

    #[test]
    fn test_platform_functions_util() {
        verify_when("util::timestamp()", |r| matches!(r, DateValue(..)));
        verify_when("util::uuid()", |r| matches!(r, Number(U128Value(..))));

        let mut interpreter = Interpreter::new();
        interpreter.evaluate(r#"
            ts := util::timestamp()
        "#).unwrap();
        interpreter = verify_where(interpreter, "util::day_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::hour_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::hour12_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::minute_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::month_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::second_of(ts)", |n| matches!(n, Number(..)));
        interpreter = verify_where(interpreter, "util::year_of(ts)", |n| matches!(n, Number(..)));
    }

    #[test]
    fn test_platform_functions_util_to_csv() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.csv.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 11.11 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                 { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 }] ~> stocks
            util::to_csv(from stocks)
        "#).unwrap();
        assert_eq!(result, Array(vec![
            StringValue(r#""ABC","AMEX",11.11"#.into()),
            StringValue(r#""UNO","OTC",0.2456"#.into()),
            StringValue(r#""BIZ","NYSE",23.66"#.into()),
            StringValue(r#""GOTO","OTC",0.1428"#.into()),
            StringValue(r#""BOOM","NASDAQ",0.0872"#.into()),
        ]));
    }

    #[test]
    fn test_platform_functions_util_to_json() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.json.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 11.11 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                 { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 }] ~> stocks
            util::to_json(from stocks)
        "#).unwrap();
        assert_eq!(result, Array(vec![
            StringValue(r#"{"symbol":"ABC","exchange":"AMEX","last_sale":11.11}"#.into()),
            StringValue(r#"{"symbol":"UNO","exchange":"OTC","last_sale":0.2456}"#.into()),
            StringValue(r#"{"symbol":"BIZ","exchange":"NYSE","last_sale":23.66}"#.into()),
            StringValue(r#"{"symbol":"GOTO","exchange":"OTC","last_sale":0.1428}"#.into()),
            StringValue(r#"{"symbol":"BOOM","exchange":"NASDAQ","last_sale":0.0872}"#.into()),
        ]));
    }

    #[test]
    fn test_platform_functions_util_numeric() {
        use crate::numbers::NumberValue::*;
        // floating-point kinds
        verify_exact("util::to_f32(1015)", Number(F32Value(1015.)));
        verify_exact("util::to_f64(7779311)", Number(F64Value(7779311.)));

        // signed-integer kinds
        verify_exact("util::to_i128(12345678987.43)", Number(I128Value(12345678987)));
        verify_exact("util::to_i64(123456789.42)", Number(I64Value(123456789)));
        verify_exact("util::to_i32(-765.65)", Number(I32Value(-765)));
        verify_exact("util::to_i16(567.311)", Number(I16Value(567)));
        verify_exact("util::to_i8(-125.089)", Number(I8Value(-125)));

        // unsigned-integer kinds
        verify_exact("util::to_u128(12789.43)", Number(U128Value(12789)));
        verify_exact("util::to_u64(12.3)", Number(U64Value(12)));
        verify_exact("util::to_u32(765.65)", Number(U32Value(765)));
        verify_exact("util::to_u16(567.311)", Number(U16Value(567)));
        verify_exact("util::to_u8(125.089)", Number(U8Value(125)));
    }

    #[test]
    fn test_platform_functions_vm_eval() {
        // valid case
        verify_exact(r#"vm::eval("2 ** 4")"#, Number(F64Value(16.)));
        // invalid case
        verify_exact("vm::eval(123)", ErrorValue(StringExpected("i64".into())))
    }

    #[actix::test]
    async fn test_platform_functions_vm_http() {
        let mut interpreter = Interpreter::new();

        // set up a listener on port 8833
        let result = interpreter.evaluate_async(r#"
            vm::serve(8833)
        "#).await.unwrap();
        assert_eq!(result, Outcome(Ack));

        // create the table
        let result = interpreter.evaluate(r#"
            create table ns("interpreter.www.stocks") (
                symbol: String(8),
                exchange: String(8),
                last_sale: f64
            )"#).unwrap();
        assert_eq!(result, Outcome(Ack));

        // append a new row
        let row_id = interpreter.evaluate_async(r#"
            POST "http://localhost:8833/interpreter/www/stocks/0" FROM {
                fields:[
                    { name: "symbol", value: "ABC" },
                    { name: "exchange", value: "AMEX" },
                    { name: "last_sale", value: 11.77 }
                ]
            }"#).await.unwrap();
        assert!(matches!(row_id, Number(I64Value(..))));

        // fetch the previously created row
        let row = interpreter.evaluate_async(format!(r#"
            GET "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        assert_eq!(row, StructureSoft(SoftStructure::new(&vec![
            ("id".into(), Number(I64Value(0))),
            ("fields".into(), Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("symbol".into())),
                    ("value", StringValue("ABC".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("exchange".into())),
                    ("value", StringValue("AMEX".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("last_sale".into())),
                    ("value", Number(F64Value(11.77))),
                ])),
            ])),
        ])));

        // replace the previously created row
        let result = interpreter.evaluate_async(r#"
            PUT "http://localhost:8833/interpreter/www/stocks/:id" FROM {
                fields:[
                    { name: "symbol", value: "ABC" },
                    { name: "exchange", value: "AMEX" },
                    { name: "last_sale", value: 11.79 }
                ]
            }"#
            .replace("/:id", format!("/{}", row_id.unwrap_value()).as_str())
            .as_str()).await.unwrap();
        assert_eq!(result, Number(I64Value(1)));

        // re-fetch the previously updated row
        let row = interpreter.evaluate_async(format!(r#"
            GET "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        assert_eq!(row, StructureSoft(SoftStructure::new(&vec![
            ("id".into(), Number(I64Value(0))),
            ("fields".into(), Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("symbol".into())),
                    ("value", StringValue("ABC".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("exchange".into())),
                    ("value", StringValue("AMEX".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("last_sale".into())),
                    ("value", Number(F64Value(11.79))),
                ])),
            ])),
        ])));

        // update the previously created row
        let result = interpreter.evaluate_async(r#"
            PATCH "http://localhost:8833/interpreter/www/stocks/:id" FROM {
                fields:[
                    { name: "last_sale", value: 11.81 }
                ]
            }"#
            .replace("/:id", format!("/{}", row_id.unwrap_value()).as_str())
            .as_str()).await.unwrap();
        assert_eq!(result, Number(I64Value(1)));

        // re-fetch the previously updated row
        let row = interpreter.evaluate_async(format!(r#"
            GET "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        assert_eq!(row, StructureSoft(SoftStructure::new(&vec![
            ("id".into(), Number(I64Value(0))),
            ("fields".into(), Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name".into(), StringValue("symbol".into())),
                    ("value".into(), StringValue("ABC".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name".into(), StringValue("exchange".into())),
                    ("value".into(), StringValue("AMEX".into())),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name".into(), StringValue("last_sale".into())),
                    ("value".into(), Number(F64Value(11.81))),
                ])),
            ])),
        ])));

        // fetch the headers for the previously updated row
        let result = interpreter.evaluate_async(format!(r#"
            HEAD "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        println!("HEAD: {}", result.to_string());
        assert!(matches!(result, StructureSoft(..)));

        // delete the previously updated row
        let result = interpreter.evaluate_async(format!(r#"
            DELETE "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        assert_eq!(result, Number(I64Value(1)));

        // verify the deleted row is empty
        let row = interpreter.evaluate_async(format!(r#"
            GET "http://localhost:8833/interpreter/www/stocks/{row_id}"
        "#).as_str()).await.unwrap();
        assert_eq!(row, StructureSoft(SoftStructure::empty()));
    }

    #[test]
    fn test_platform_functions_vm_version() {
        verify_exact(
            "vm::version()",
            StringValue(format!("{MAJOR_VERSION}.{MINOR_VERSION}")))
    }

    #[test]
    fn test_compact_from_namespace() {
        let phys_columns = make_quote_columns();
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.compact.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "DMX", exchange: "NYSE", last_sale: 99.99 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                 { symbol: "ABC", exchange: "AMEX", last_sale: 11.11 },
                 { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 },
                 { symbol: "JET", exchange: "NASDAQ", last_sale: 32.12 }] ~> stocks
            [+] delete from stocks where last_sale > 1.0
            [+] from stocks
        "#).unwrap();

        let rc = result.to_table().unwrap();
        assert_eq!(rc.read_active_rows().unwrap(), vec![
            make_quote(1, "UNO", "OTC", 0.2456),
            make_quote(3, "GOTO", "OTC", 0.1428),
            make_quote(5, "BOOM", "NASDAQ", 0.0872),
        ]);

        let result = interpreter.evaluate(r#"
            [+] compact stocks
            [+] from stocks
        "#).unwrap();
        let rc = result.to_table().unwrap();
        let rows = rc.read_active_rows().unwrap();
        for s in TableRenderer::from_rows(rc.get_columns().clone(), rows.to_owned()) { println!("{}", s); }
        assert_eq!(rows, vec![
            make_quote(0, "BOOM", "NASDAQ", 0.0872),
            make_quote(1, "UNO", "OTC", 0.2456),
            make_quote(2, "GOTO", "OTC", 0.1428),
        ]);
    }

    #[test]
    fn test_feature_with_scenarios() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            feature "Matches function" {
                scenario "Compare Array contents: Equal" {
                    assert(matches(
                        [ 1 "a" "b" "c" ],
                        [ 1 "a" "b" "c" ]
                    ))
                }
                scenario "Compare Array contents: Not Equal" {
                    assert(!matches(
                        [ 1 "a" "b" "c" ],
                        [ 0 "x" "y" "z" ]
                    ))
                }
                scenario "Compare JSON contents (in sequence)" {
                    assert(matches(
                            { first: "Tom" last: "Lane" },
                            { first: "Tom" last: "Lane" }))
                }
                scenario "Compare JSON contents (out of sequence)" {
                    assert(matches(
                            { scores: [82 78 99], id: "A1537" },
                            { id: "A1537", scores: [82 78 99] }))
                }
            } "#).unwrap();
        let table = result.to_table().unwrap();
        let phys_columns = table.get_columns().clone();
        let rows = table.read_active_rows().unwrap();
        let output = TableRenderer::from_rows(phys_columns, rows.to_owned()).join("\n");
        println!("{}", output);
        assert_eq!(output, r#"|---------------------------------------------------------------------------------------------------------------------|
| level | item                                                                                      | passed | result |
|---------------------------------------------------------------------------------------------------------------------|
| 0     | Matches function                                                                          | true   | ack    |
| 1     | Compare Array contents: Equal                                                             | true   | ack    |
| 2     | assert(matches([1, "a", "b", "c"], [1, "a", "b", "c"]))                                   | true   | true   |
| 1     | Compare Array contents: Not Equal                                                         | true   | ack    |
| 2     | assert(!matches([1, "a", "b", "c"], [0, "x", "y", "z"]))                                  | true   | true   |
| 1     | Compare JSON contents (in sequence)                                                       | true   | ack    |
| 2     | assert(matches({first: "Tom", last: "Lane"}, {first: "Tom", last: "Lane"}))               | true   | true   |
| 1     | Compare JSON contents (out of sequence)                                                   | true   | ack    |
| 2     | assert(matches({scores: [82, 78, 99], id: "A1537"}, {id: "A1537", scores: [82, 78, 99]})) | true   | true   |
|---------------------------------------------------------------------------------------------------------------------|"#);
    }

    #[test]
    fn test_describe_table_structure() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.struct.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            describe stocks
        "#).unwrap();

        // process the table result
        // |-----------------------------------------------------|
        // | name      | type      | default_value | is_nullable |
        // |-----------------------------------------------------|
        // | symbol    | String(8) | null          | true        |
        // | exchange  | String(8) | null          | true        |
        // | last_sale | f64       | null          | true        |
        // |-----------------------------------------------------|
        let mrc = result.to_table().unwrap();
        let mrc_rows = mrc.read_active_rows().unwrap();
        let mrc_columns = mrc.get_columns();
        for s in TableRenderer::from_rows(mrc_columns.clone(), mrc_rows.to_owned()) { println!("{}", s); }
        assert_eq!(mrc_rows, vec![
            Row::new(0, vec![
                StringValue("symbol".into()),
                StringValue("String(8)".into()),
                StringValue("null".into()),
                Boolean(true),
            ]),
            Row::new(1, vec![
                StringValue("exchange".into()),
                StringValue("String(8)".into()),
                StringValue("null".into()),
                Boolean(true),
            ]),
            Row::new(2, vec![
                StringValue("last_sale".into()),
                StringValue("f64".into()),
                StringValue("null".into()),
                Boolean(true),
            ]),
        ]);
    }

    #[test]
    fn test_directive_die() {
        verify_exact(r#"
            [!] "Kaboom!!!"
        "#, ErrorValue(Exact("Kaboom!!!".into())));
    }

    #[test]
    fn test_directive_ignore_failure() {
        verify_exact(r#"[~] vm::eval("7 / 0")"#, Outcome(Ack));
    }

    #[test]
    fn test_directive_must_be_true() {
        verify_exact("[+] x := 67", Outcome(Ack));
    }

    #[test]
    fn test_directive_must_be_false() {
        verify_exact(r#"
            [+] x := 67
            [-] x < 67
        "#, Boolean(false));
    }

    #[test]
    fn test_directives_pipeline() {
        verify_exact(r#"
            [+] stocks := ns("interpreter.pipeline.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 12.49 },
                 { symbol: "BOOM", exchange: "NYSE", last_sale: 56.88 },
                 { symbol: "JET", exchange: "NASDAQ", last_sale: 32.12 }] ~> stocks
            [+] delete from stocks where last_sale < 30.0
            [+] from stocks
        "#, TableValue(ModelRowCollection::from_rows(make_quote_columns(), vec![
            make_quote(1, "BOOM", "NYSE", 56.88),
            make_quote(2, "JET", "NASDAQ", 32.12),
        ])));
    }

    #[test]
    fn test_hard_structure() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            struct(symbol: String(8), exchange: String(8), last_sale: f64)
        "#).unwrap();
        let phys_columns = make_quote_columns();
        assert_eq!(result, StructureHard(HardStructure::new(phys_columns.clone(), Vec::new())));
    }

    fn test_hard_structure_with_default_values() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            struct(
                symbol: String(8) = "ABC",
                exchange: String(8) = "NYSE",
                last_sale: f64 = 23.67
            )
        "#).unwrap();
        match result {
            StructureHard(s) =>
                assert_eq!(s.get_values(), vec![
                    StringValue("ABC".into()),
                    StringValue("NYSE".into()),
                    Number(F64Value(23.67)),
                ]),
            x => assert_eq!(x, Undefined)
        }
    }

    #[test]
    fn test_hard_structure_field() {
        verify_exact(r#"
            stock := struct(
                symbol: String(8) = "ABC",
                exchange: String(8) = "NYSE",
                last_sale: f64 = 23.67
            )
            stock::symbol
        "#, StringValue("ABC".into()));
    }

    #[test]
    fn test_hard_structure_impl_method() {
        verify_exact(r#"
            stock := struct(
                symbol: String(8) = "ABC",
                exchange: String(8) = "NYSE",
                last_sale: f64 = 23.67
            )

            impl stock {
                fn is_this_you(symbol) => symbol == self::symbol
            }

            stock::is_this_you('ABC')
        "#, Boolean(true));
    }

    #[test]
    fn test_hard_structure_import() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            stock := struct(
                symbol: String(8) = "ABC",
                exchange: String(8) = "NYSE",
                last_sale: f64 = 23.67
            )
            import stock
        "#).unwrap();
        assert_eq!(interpreter.machine.get("symbol"), Some(StringValue("ABC".into())));
        assert_eq!(interpreter.machine.get("exchange"), Some(StringValue("NYSE".into())));
    }

    #[test]
    fn test_hard_structure_from_table() {
        verify_exact(r#"
            [+] stocks := ns("interpreter.struct.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 11.11 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                 { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 }] ~> stocks
            stocks[0]
        "#, StructureHard(HardStructure::from_parameters_and_values(
            &make_quote_parameters(), vec![
                StringValue("ABC".into()),
                StringValue("AMEX".into()),
                Number(F64Value(11.11)),
            ],
        ).unwrap()));
    }

    #[test]
    fn test_hard_structure_to_table() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            util::to_table(struct(
                symbol: String(8) = "ABC",
                exchange: String(8) = "NYSE",
                last_sale: f64 = 45.67
            ))
        "#).unwrap();
        let rows = result.to_table().unwrap()
            .read_active_rows().unwrap();
        assert_eq!(rows, vec![
            make_quote(0, "ABC", "NYSE", 45.67),
        ])
    }

    #[test]
    fn test_soft_structure() {
        let code = r#"
            {
                fields:[
                    { name: "symbol", value: "ABC" },
                    { name: "exchange", value: "AMEX" },
                    { name: "last_sale", value: 11.77 }
                ]
            }"#;
        let model = StructureSoft(SoftStructure::new(&vec![
            ("fields", Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("symbol".into())),
                    ("value", StringValue("ABC".into()))
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("exchange".into())),
                    ("value", StringValue("AMEX".into()))
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("last_sale".into())),
                    ("value", Number(F64Value(11.77)))
                ])),
            ]))
        ]));
        verify_exact(code, model.clone());
        assert_eq!(
            model.to_code().chars().filter(|c| !c.is_whitespace())
                .collect::<String>(),
            code.chars().filter(|c| !c.is_whitespace())
                .collect::<String>())
    }

    #[test]
    fn test_soft_structure_field() {
        verify_exact(r#"
            stock := { symbol:"AAA", price:123.45 }
            stock::symbol
        "#, StringValue("AAA".into()));
    }

    #[test]
    fn test_soft_structure_method() {
        verify_exact(r#"
            stock := {
                symbol:"ABC",
                price:123.45,
                last_sale: 23.67,
                is_this_you: fn(symbol) => symbol == self::symbol
            }
            stock::is_this_you('ABC')
        "#, Boolean(true));
    }

    #[test]
    fn test_soft_structure_impl_method() {
        verify_exact(r#"
            stock := {
                symbol: "ABC",
                exchange: "NYSE",
                last_sale: 23.67
            }

            impl stock {
                fn is_this_you(symbol) => symbol == self::symbol
            }

            stock::is_this_you('ABC')
        "#, Boolean(true));
    }

    #[test]
    fn test_soft_structure_import() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            quote := { symbol: "ABC", exchange: "AMEX" }
            import quote
        "#).unwrap();
        assert_eq!(interpreter.machine.get("symbol"), Some(StringValue("ABC".into())));
        assert_eq!(interpreter.machine.get("exchange"), Some(StringValue("AMEX".into())));
    }

    #[test]
    fn test_soft_structure_json_literal_1() {
        verify_exact(r#"
            {
              "columns": [{
                  "name": "symbol",
                  "param_type": "String(8)",
                  "default_value": null
                }, {
                  "name": "exchange",
                  "param_type": "String(8)",
                  "default_value": null
                }, {
                  "name": "last_sale",
                  "param_type": "f64",
                  "default_value": null
                }],
              "indices": [],
              "partitions": []
            }
        "#, StructureSoft(SoftStructure::new(&vec![
            ("columns", Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("symbol".into())),
                    ("param_type", StringValue("String(8)".into())),
                    ("default_value", Null),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("exchange".into())),
                    ("param_type", StringValue("String(8)".into())),
                    ("default_value", Null)
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("last_sale".into())),
                    ("param_type", StringValue("f64".into())),
                    ("default_value", Null),
                ])),
            ])),
            ("indices", Array(Vec::new())),
            ("partitions", Array(Vec::new())),
        ])));
    }

    #[test]
    fn test_soft_structure_json_literal_2() {
        verify_exact(r#"
            {
              columns: [{
                  name: "symbol",
                  param_type: "String(8)",
                  default_value: null
                }, {
                  name: "exchange",
                  param_type: "String(8)",
                  default_value: null
                }, {
                  name: "last_sale",
                  param_type: "f64",
                  default_value: null
                }]
            }
        "#, StructureSoft(SoftStructure::new(&vec![
            ("columns", Array(vec![
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("symbol".into())),
                    ("param_type", StringValue("String(8)".into())),
                    ("default_value", Null),
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("exchange".into())),
                    ("param_type", StringValue("String(8)".into())),
                    ("default_value", Null)
                ])),
                StructureSoft(SoftStructure::new(&vec![
                    ("name", StringValue("last_sale".into())),
                    ("param_type", StringValue("f64".into())),
                    ("default_value", Null),
                ])),
            ]))
        ])));
    }

    #[test]
    fn test_soft_structure_to_table() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            util::to_table({
                symbol: "ABC",
                exchange: "NYSE",
                last_sale: 23.67
            })
        "#).unwrap();
        let rows = result.to_table().unwrap()
            .read_active_rows().unwrap();
        assert_eq!(rows, vec![
            make_quote(0, "ABC", "NYSE", 23.67),
        ])
    }

    #[test]
    fn test_soft_structure_array_to_table() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            util::to_table([{
                symbol: "TRX",
                exchange: "AMEX",
                last_sale: 29.88
            }, {
                symbol: "BMX",
                exchange: "NYSE",
                last_sale: 46.11
            }])
        "#).unwrap();
        let rows = result.to_table().unwrap()
            .read_active_rows().unwrap();
        assert_eq!(rows, vec![
            make_quote(0, "TRX", "AMEX", 29.88),
            make_quote(1, "BMX", "NYSE", 46.11),
        ])
    }

    #[test]
    fn test_structure_mixed_array_to_table() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
             stocks := util::to_table([
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "DMX", exchange: "OTC_BB", last_sale: 1.17 }
             ])

            util::to_table([
                stocks,
                struct(
                    symbol: String(8) = "ABC",
                    exchange: String(8) = "OTHER_OTC",
                    last_sale: f64 = 0.67),
                { symbol: "TRX", exchange: "AMEX", last_sale: 29.88 },
                { symbol: "BMX", exchange: "NASDAQ", last_sale: 46.11 }
            ])
        "#).unwrap();
        let rows = result.to_table().unwrap()
            .read_active_rows().unwrap();
        assert_eq!(rows, vec![
            make_quote(0, "BIZ", "NYSE", 23.66),
            make_quote(1, "DMX", "OTC_BB", 1.17),
            make_quote(2, "ABC", "OTHER_OTC", 0.67),
            make_quote(3, "TRX", "AMEX", 29.88),
            make_quote(4, "BMX", "NASDAQ", 46.11),
        ])
    }

    #[test]
    fn test_module() {
        verify_exact(r#"
            mod abc {
                fn hello(name) => str::format("hello {}", name)
            }
            abc::hello('world')
        "#, StringValue("hello world".into()));
    }

    #[test]
    fn test_function_lambda() {
        let mut interpreter = Interpreter::new();
        assert_eq!(Outcome(Ack), interpreter.evaluate(r#"
            product := fn (a, b) => a * b
        "#).unwrap());

        assert_eq!(Number(I64Value(10)), interpreter.evaluate(r#"
            product(2, 5)
        "#).unwrap())
    }

    #[test]
    fn test_function_named() {
        let mut interpreter = Interpreter::new();
        assert_eq!(Outcome(Ack), interpreter.evaluate(r#"
            fn product(a, b) => a * b
        "#).unwrap());

        assert_eq!(Number(I64Value(10)), interpreter.evaluate(r#"
            product(2, 5)
        "#).unwrap())
    }

    #[test]
    fn test_function_recursion_1() {
        verify_exact(r#"
            f := fn(n: i64) => if(n <= 1) 1 else n * f(n - 1)
            f(5)
        "#, Number(I64Value(120)))
    }

    #[test]
    fn test_function_recursion_2() {
        verify_exact(r#"
            f := fn(n) => iff(n <= 1, 1, n * f(n - 1))
            f(6)
        "#, Number(I64Value(720)))
    }

    #[test]
    fn test_if_when_result_is_defined() {
        verify_exact(r#"
            x := 7
            if(x > 5) "Yes"
        "#, StringValue("Yes".into()));
    }

    #[test]
    fn test_if_when_result_is_undefined() {
        verify_exact(r#"
            x := 4
            if(x > 5) "Yes"
        "#, Undefined);
    }

    #[test]
    fn test_if_else_expression() {
        verify_exact(r#"
            x := 4
            if(x > 5) "Yes"
            else if(x < 5) "Maybe"
            else "No"
        "#, StringValue("Maybe".into()));
    }

    #[test]
    fn test_if_function() {
        verify_exact(r#"
            x := 4
            iff(x > 5, "Yes", iff(x < 5, "Maybe", "No"))
        "#, StringValue("Maybe".into()));
    }

    #[test]
    fn test_import_function_from_package() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            import str::format
            format("This {} the {}", "is", "way")
        "#).unwrap();
        assert_eq!(result, StringValue("This is the way".into()));

        // attempt a non-import function from the same package
        let result = interpreter.evaluate(r#"
            left('Hello World', -5)
        "#);
        assert!(matches!(result, Err(..)));
    }

    #[test]
    fn test_import_all_from_package() {
        let mut interpreter = Interpreter::new();

        // initially 'to_u8' should not be in scope
        assert_eq!(interpreter.machine.get("to_u8"), None);
        assert_eq!(interpreter.machine.get("to_u16"), None);
        assert_eq!(interpreter.machine.get("to_u32"), None);
        assert_eq!(interpreter.machine.get("to_u64"), None);

        // import all conversion members
        interpreter.evaluate("import util").unwrap();

        // after the import, 'to_u8' should be in scope
        assert_eq!(interpreter.machine.get("to_u8"), Some(BackDoor(BackDoorKey::UtilToU8)));
        assert_eq!(interpreter.machine.get("to_u16"), Some(BackDoor(BackDoorKey::UtilToU16)));
        assert_eq!(interpreter.machine.get("to_u32"), Some(BackDoor(BackDoorKey::UtilToU32)));
        assert_eq!(interpreter.machine.get("to_u64"), Some(BackDoor(BackDoorKey::UtilToU64)));
    }

    #[test]
    fn test_postfix_methods() {
        verify_exact(r#"
            import str::substring
            "Hello":::substring(1, 4)
        "#, StringValue("ell".into()));

        verify_exact(r#"
            import str::to_string
            123:::to_string()
        "#, StringValue("123".into()));
    }

    #[test]
    fn test_include_file_valid() {
        let phys_columns = make_quote_columns();
        verify_exact(r#"
            include "./demoes/language/include_file.oxide"
        "#, TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
            make_quote(0, "ABC", "AMEX", 12.49),
            make_quote(1, "BOOM", "NYSE", 56.88),
            make_quote(2, "JET", "NASDAQ", 32.12),
        ])));

        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            include 123
        "#).unwrap();
        assert_eq!(result, ErrorValue(TypeMismatch("String".into(), "i64".into())))
    }

    #[test]
    fn test_matches_1() {
        // test a perfect match
        verify_exact(r#"
            a := { first: "Tom", last: "Lane", scores: [82, 78, 99] }
            b := { first: "Tom", last: "Lane", scores: [82, 78, 99] }
            matches(a, b)
        "#, Boolean(true));
    }

    #[test]
    fn test_matches_2() {
        // test an unordered match
        verify_exact(r#"
            a := { scores: [82, 78, 99], first: "Tom", last: "Lane" }
            b := { last: "Lane", first: "Tom", scores: [82, 78, 99] }
            matches(a, b)
        "#, Boolean(true));
    }

    #[test]
    fn test_match_not_1() {
        // test when things do not match 1
        verify_exact(r#"
            a := { first: "Tom", last: "Lane" }
            b := { first: "Jerry", last: "Lane" }
            matches(a, b)
        "#, Boolean(false));
    }

    #[test]
    fn test_match_not_2() {
        // test when things do not match 2
        verify_exact(r#"
            a := { key: "123", values: [1, 74, 88] }
            b := { key: "123", values: [1, 74, 88, 0] }
            matches(a, b)
        "#, Boolean(false));
    }

    #[test]
    fn test_table_create_ephemeral() {
        verify_exact(r#"
        table(
            symbol: String(8),
            exchange: String(8),
            last_sale: f64
        )"#, TableValue(ModelRowCollection::with_rows(
            make_quote_columns(), Vec::new(),
        )))
    }

    #[test]
    fn test_table_create_durable() {
        verify_exact(r#"
            create table ns("interpreter.create.stocks") (
                symbol: String(8),
                exchange: String(8),
                last_sale: f64
            )"#, Outcome(Ack))
    }

    #[test]
    fn test_table_crud_in_namespace() {
        let mut interpreter = Interpreter::new();
        let columns = make_quote_parameters();
        let phys_columns = Column::from_parameters(&columns).unwrap();

        // set up the interpreter
        assert_eq!(Outcome(Ack), interpreter.evaluate(r#"
            stocks := ns("interpreter.crud.stocks")
        "#).unwrap());

        // create the table
        assert_eq!(Outcome(RowsAffected(0)), interpreter.evaluate(r#"
            table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
        "#).unwrap());

        // append a row
        assert_eq!(Outcome(RowsAffected(1)), interpreter.evaluate(r#"
            append stocks from { symbol: "ABC", exchange: "AMEX", last_sale: 11.77 }
        "#).unwrap());

        // write another row
        assert_eq!(Outcome(RowsAffected(1)), interpreter.evaluate(r#"
            { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 } ~> stocks
        "#).unwrap());

        // write some more rows
        assert_eq!(Outcome(RowsAffected(2)), interpreter.evaluate(r#"
            [{ symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
             { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 }] ~> stocks
        "#).unwrap());

        // write even more rows
        assert_eq!(Outcome(RowsAffected(2)), interpreter.evaluate(r#"
            append stocks from [
                { symbol: "BOOM", exchange: "NASDAQ", last_sale: 56.87 },
                { symbol: "TRX", exchange: "NASDAQ", last_sale: 7.9311 }
            ]
        "#).unwrap());

        // remove some rows
        assert_eq!(Outcome(RowsAffected(4)), interpreter.evaluate(r#"
            delete from stocks where last_sale > 1.0
        "#).unwrap());

        // overwrite a row
        assert_eq!(Outcome(RowsAffected(1)), interpreter.evaluate(r#"
            overwrite stocks
            via {symbol: "GOTO", exchange: "OTC", last_sale: 0.1421}
            where symbol == "GOTO"
        "#).unwrap());

        // verify the remaining rows
        assert_eq!(
            interpreter.evaluate("from stocks").unwrap(),
            TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
                make_quote(1, "UNO", "OTC", 0.2456),
                make_quote(3, "GOTO", "OTC", 0.1421),
            ]))
        );

        // restore the previously deleted rows
        assert_eq!(Outcome(RowsAffected(4)), interpreter.evaluate(r#"
            undelete from stocks where last_sale > 1.0
        "#).unwrap());

        // verify the existing rows
        assert_eq!(
            interpreter.evaluate("from stocks").unwrap(),
            TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
                make_quote(0, "ABC", "AMEX", 11.77),
                make_quote(1, "UNO", "OTC", 0.2456),
                make_quote(2, "BIZ", "NYSE", 23.66),
                make_quote(3, "GOTO", "OTC", 0.1421),
                make_quote(4, "BOOM", "NASDAQ", 56.87),
                make_quote(5, "TRX", "NASDAQ", 7.9311),
            ]))
        );
    }

    #[test]
    fn test_table_scan_from_namespace() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.scan.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 12.33 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 9.775 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1442 },
                 { symbol: "XYZ", exchange: "NYSE", last_sale: 0.0289 }] ~> stocks
            [+] delete from stocks where last_sale > 1.0
            [+] scan stocks
        "#).unwrap();

        // |-----------------------------------------------|
        // | symbol | exchange | last_sale | _id | _active |
        // |-----------------------------------------------|
        // | ABC    | AMEX     | 12.33     | 0   | false   |
        // | UNO    | OTC      | 0.2456    | 1   | true    |
        // | BIZ    | NYSE     | 9.775     | 2   | false   |
        // | GOTO   | OTC      | 0.1442    | 3   | true    |
        // | XYZ    | NYSE     | 0.0289    | 4   | true    |
        // |-----------------------------------------------|
        let mrc = result.to_table().unwrap();
        let mrc_rows = mrc.read_active_rows().unwrap();
        let scan_columns = mrc.get_columns();
        for s in TableRenderer::from_rows(scan_columns.clone(), mrc_rows.to_owned()) { println!("{}", s); }
        assert_eq!(mrc_rows, vec![
            make_scan_quote(0, "ABC", "AMEX", 12.33, false),
            make_scan_quote(1, "UNO", "OTC", 0.2456, true),
            make_scan_quote(2, "BIZ", "NYSE", 9.775, false),
            make_scan_quote(3, "GOTO", "OTC", 0.1442, true),
            make_scan_quote(4, "XYZ", "NYSE", 0.0289, true),
        ])
    }

    #[test]
    fn test_table_select_from_namespace() {
        // create a table with test data
        let columns = make_quote_parameters();
        let phys_columns = Column::from_parameters(&columns).unwrap();

        // append some rows
        let mut interpreter = Interpreter::new();
        assert_eq!(Outcome(RowsAffected(5)), interpreter.evaluate(r#"
            stocks := ns("interpreter.select1.stocks")
            table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            append stocks from [
                { symbol: "ABC", exchange: "AMEX", last_sale: 11.77 },
                { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 }
            ]
        "#).unwrap());

        // compile and execute the code
        assert_eq!(interpreter.evaluate(r#"
            select symbol, exchange, last_sale from stocks
            where last_sale > 1.0
            order by symbol
            limit 5
        "#).unwrap(), TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
            make_quote(0, "ABC", "AMEX", 11.77),
            make_quote(2, "BIZ", "NYSE", 23.66),
        ])));
    }

    #[test]
    fn test_table_select_from_variable() {
        // create a table with test data
        let columns = make_quote_parameters();
        let phys_columns = Column::from_parameters(&columns).unwrap();

        // set up the interpreter
        let mut interpreter = Interpreter::new();
        assert_eq!(interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.select.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 11.77 },
                 { symbol: "UNO", exchange: "OTC", last_sale: 0.2456 },
                 { symbol: "BIZ", exchange: "NYSE", last_sale: 23.66 },
                 { symbol: "GOTO", exchange: "OTC", last_sale: 0.1428 },
                 { symbol: "BOOM", exchange: "NASDAQ", last_sale: 0.0872 }] ~> stocks
            [+] select symbol, exchange, last_sale
                from stocks
                where last_sale > 1.0
                order by symbol
                limit 5
        "#).unwrap(), TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
            make_quote(0, "ABC", "AMEX", 11.77),
            make_quote(2, "BIZ", "NYSE", 23.66),
        ])));
    }

    #[test]
    fn test_table_reverse_from_namespace() {
        let mut interpreter = Interpreter::new();
        let result = interpreter.evaluate(r#"
            [+] stocks := ns("interpreter.reverse.stocks")
            [+] table(symbol: String(8), exchange: String(8), last_sale: f64) ~> stocks
            [+] [{ symbol: "ABC", exchange: "AMEX", last_sale: 12.49 },
                 { symbol: "BOOM", exchange: "NYSE", last_sale: 56.88 },
                 { symbol: "JET", exchange: "NASDAQ", last_sale: 32.12 }] ~> stocks
            [+] reverse from stocks
        "#).unwrap();
        let phys_columns = make_quote_columns();
        assert_eq!(result, TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
            make_quote(2, "JET", "NASDAQ", 32.12),
            make_quote(1, "BOOM", "NYSE", 56.88),
            make_quote(0, "ABC", "AMEX", 12.49),
        ])));
    }

    #[test]
    fn test_table_reverse_from_variable() {
        let phys_columns = make_quote_columns();
        let mut interpreter = Interpreter::new();
        interpreter.with_variable("stocks", TableValue(
            ModelRowCollection::from_rows(phys_columns.clone(), vec![
                make_quote(0, "ABC", "AMEX", 11.88),
                make_quote(1, "UNO", "OTC", 0.2456),
                make_quote(2, "BIZ", "NYSE", 23.66),
                make_quote(3, "GOTO", "OTC", 0.1428),
                make_quote(4, "BOOM", "NASDAQ", 56.87),
            ])));

        let result = interpreter.evaluate(r#"
            reverse from stocks
        "#).unwrap();
        assert_eq!(result, TableValue(ModelRowCollection::from_rows(phys_columns.clone(), vec![
            make_quote(4, "BOOM", "NASDAQ", 56.87),
            make_quote(3, "GOTO", "OTC", 0.1428),
            make_quote(2, "BIZ", "NYSE", 23.66),
            make_quote(1, "UNO", "OTC", 0.2456),
            make_quote(0, "ABC", "AMEX", 11.88),
        ])));
    }

    #[test]
    fn test_type_of() {
        verify_exact("type_of([true, false])", StringValue("Array<Boolean>".into()));
        verify_exact("type_of([12, 76, 444])", StringValue("Array<i64>".into()));
        verify_exact("type_of(['ciao', 'hello', 'world'])", StringValue("Array<String(5)>".into()));
        verify_exact("type_of(false)", StringValue("Boolean".into()));
        verify_exact("type_of(true)", StringValue("Boolean".into()));
        verify_exact("type_of(util::timestamp())", StringValue("Date".into()));
        verify_exact("type_of(fn(a, b) => a + b)", StringValue("fn(a, b)".into()));
        verify_exact("type_of(1234)", StringValue("i64".into()));
        verify_exact("type_of(12.394)", StringValue("f64".into()));
        verify_exact("type_of('1234')", StringValue("String(4)".into()));
        verify_exact(r#"type_of("abcde")"#, StringValue("String(5)".into()));
        verify_exact(r#"type_of({symbol:"ABC"})"#,
                     StringValue(r#"struct(symbol: String(3) = "ABC")"#.into()));
        verify_exact(r#"type_of(ns("a.b.c"))"#, StringValue("Table()".into()));
        verify_exact(r#"type_of(table(
            symbol: String(8),
            exchange: String(8),
            last_sale: f64
        ))"#, StringValue("Table(symbol: String(8), exchange: String(8), last_sale: f64)".into()));
        verify_exact("type_of(util::uuid())", StringValue("u128".into()));
        verify_exact("type_of(my_var)", StringValue("".into()));
        verify_exact("type_of(null)", StringValue("".into()));
        verify_exact("type_of(undefined)", StringValue("".into()));
    }

    #[test]
    fn test_while_loop() {
        let mut interpreter = Interpreter::new();
        assert_eq!(Outcome(Ack), interpreter.evaluate("x := 0").unwrap());
        assert_eq!(Outcome(Ack), interpreter.evaluate(r#"
            while (x < 5)
                x := x + 1
        "#).unwrap());
        assert_eq!(Number(I64Value(5)), interpreter.evaluate("x").unwrap());
    }

    fn verify_whence(mut interpreter: Interpreter, code: &str, expected: TypedValue) -> Interpreter {
        let actual = interpreter.evaluate(code).unwrap();
        assert_eq!(actual, expected);
        interpreter
    }

    fn verify_exact(code: &str, expected: TypedValue) {
        let mut interpreter = Interpreter::new();
        let actual = interpreter.evaluate(code).unwrap();
        assert_eq!(actual, expected);
    }

    fn verify_when(code: &str, f: fn(TypedValue) -> bool) {
        let mut interpreter = Interpreter::new();
        let actual = interpreter.evaluate(code).unwrap();
        assert!(f(actual));
    }

    fn verify_where(
        interpreter: Interpreter,
        code: &str,
        f: fn(TypedValue) -> bool,
    ) -> Interpreter {
        let mut my_interpreter = interpreter;
        let actual = my_interpreter.evaluate(code).unwrap();
        assert!(f(actual));
        my_interpreter
    }
}