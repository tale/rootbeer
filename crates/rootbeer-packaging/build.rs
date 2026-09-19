#[path = "../../scripts/cache_inputs.rs"]
mod cache_inputs;

fn main() {
    cache_inputs::emit(&["rootbeer-packaging"]);
}
