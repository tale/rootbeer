fn main() {
    println!("cargo:rerun-if-env-changed=ROOTBEER_PDR_URL");
    println!("cargo:rerun-if-env-changed=ROOTBEER_PDR_PUBLIC_KEY");
}
