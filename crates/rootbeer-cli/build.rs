fn main() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch");

    let secs = now.as_secs();

    // Format as UTC: YYYY-MM-DD HH:MM
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Days since 1970-01-01 → Y/M/D (civil calendar)
    let (y, m, d) = civil_from_days(days as i64);

    let timestamp = format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02} UTC");
    println!("cargo:rustc-env=RB_BUILD_TIMESTAMP={timestamp}");
}

/// Convert days since 1970-01-01 to (year, month, day).
/// Algorithm from Howard Hinnant's `chrono`-compatible date library.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
