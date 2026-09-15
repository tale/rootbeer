fn main() {
    println!("cargo:rerun-if-env-changed=ROOTBEER_INDEX_URL");
    println!("cargo:rerun-if-env-changed=ROOTBEER_INDEX_PUBLIC_KEY");
}
