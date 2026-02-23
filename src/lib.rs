#[unsafe(no_mangle)]
pub extern "C" fn iris_is_engine_ready(n: i32) -> i32 {
    let mut t: i32 = 0;
    for _i in 0..n {
        t = std::hint::black_box(t + 1);
    }
    t
}

#[unsafe(no_mangle)]
pub extern "C" fn iris_check_status(value: i32) -> bool {
    value > 0
}
