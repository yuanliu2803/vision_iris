mod build;

#[unsafe(no_mangle)]
pub extern "C" fn iris_is_engine_ready() -> bool {
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn iris_check_status(value: i32) -> bool {
    value > 0
}
