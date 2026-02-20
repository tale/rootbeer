fn main() {
    #[cfg(not(unix))]
    compile_error!("rootbeer only supports unix-like systems (macOS, Linux)");
}
