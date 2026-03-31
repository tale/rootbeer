fn main() {
    println!("cargo:rerun-if-env-changed=RB_BUILD_TIMESTAMP");
    let timestamp = match std::env::var("RB_BUILD_TIMESTAMP") {
        Ok(val) => val,
        Err(_) => {
            let now = time::OffsetDateTime::now_utc();
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02} UTC",
                now.year(),
                now.month() as u8,
                now.day(),
                now.hour(),
                now.minute(),
            )
        }
    };

    println!("cargo:rustc-env=RB_BUILD_TIMESTAMP={timestamp}");
}
