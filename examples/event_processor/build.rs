fn main() {
    if let Err(err) = gust_build::compile_gust_files() {
        panic!("gust build failed: {err}");
    }
}
