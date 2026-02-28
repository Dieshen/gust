use gust_lang::parse_program_with_errors;
use proptest::prelude::*;

fn machine_with_int_literal(digits: &str) -> String {
    format!(
        "machine M {{
    state A
    transition t: A -> A
    on t() {{
        let n = {digits};
        goto A();
    }}
}}"
    )
}

fn machine_with_string_literal(content: &str) -> String {
    format!(
        "machine M {{
    state A(msg: String)
    transition t: A -> A
    on t() {{
        goto A(\"{content}\");
    }}
}}"
    )
}

proptest! {
    #[test]
    fn parser_never_panics_on_arbitrary_integer_lengths(digits in "[0-9]{1,256}") {
        let source = machine_with_int_literal(&digits);
        let result = std::panic::catch_unwind(|| parse_program_with_errors(&source, "prop.gu"));
        prop_assert!(result.is_ok(), "parser panicked for digits: {digits}");
    }

    #[test]
    fn parser_never_panics_on_string_literals_without_quotes(content in "[^\\\"]{0,128}") {
        let source = machine_with_string_literal(&content);
        let result = std::panic::catch_unwind(|| parse_program_with_errors(&source, "prop.gu"));
        prop_assert!(result.is_ok(), "parser panicked for string content: {content:?}");
    }
}
