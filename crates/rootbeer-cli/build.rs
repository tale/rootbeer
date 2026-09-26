fn main() {
    println!("cargo:rerun-if-env-changed=RB_SOURCE_REVISION");

    // Builds that name no source commit say so rather than embedding the clock,
    // which would make every build of the same source differ.
    let revision = std::env::var("RB_SOURCE_REVISION")
        .map(|revision| revision.chars().take(12).collect::<String>())
        .unwrap_or_else(|_| "dev".into());
    println!("cargo:rustc-env=RB_SOURCE_REVISION={revision}");
}
