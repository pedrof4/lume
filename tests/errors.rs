use std::{fs, path::Path};
fn invalid(id: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/errors");
    let source = root.join(format!("{id}.lume"));
    let expected = fs::read_to_string(root.join(format!("{id}.stderr"))).unwrap();
    let messages: Vec<_> = lumec::compile_path(&source, 0)
        .err()
        .expect("programa inválido foi aceito")
        .into_iter()
        .map(|d| d.to_string())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains(expected.trim())),
        "esperado {expected}, recebido {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|m| m.contains(".lume:") && m.contains('^')),
        "{messages:?}"
    );
}
macro_rules! cases { ($($name:ident),* $(,)?) => { $( #[test] fn $name() { invalid(stringify!($name)); } )* }; }
cases! {
    call_nonfunction,
    char_empty,
    char_too_long,
    closure_bad_capture,
    duplicate_field,
    duplicate_function,
    duplicate_local,
    duplicate_match_arm,
    duplicate_parameter,
    empty_untyped_array,
    foreign_variant,
    heterogeneous_array,
    immutable_assignment,
    import_missing,
    incomplete_impl,
    inexhaustive_bool,
    inexhaustive_enum,
    int_overflow,
    invalid_impl,
    invalid_index_type,
    local_outside_scope,
    loop_control,
    missing_block_end,
    missing_delimiter,
    missing_trait_bound,
    mixed_arithmetic,
    narrow_literal_overflow,
    no_main,
    nonboolean_if,
    nonboolean_loop,
    propagate_wrong_error,
    propagate_wrong_return,
    return_mismatch,
    return_path,
    shadow_intrinsic,
    string_mutation,
    unclosed_string,
    unexpected_item,
    uninferred_type,
    unknown_char,
    unknown_escape,
    unknown_field,
    unknown_name,
    unknown_type,
    unreachable_match_arm,
    widening_requires_cast,
    wrong_argument_count,
    wrong_argument_type,
    wrong_generic_arity,
    wrong_main,
}
