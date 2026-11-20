use jiff::ZonedDateTime as Zoned;

#[allow(dead_code, reason = "public clock helper consumed by later milestones")]
pub fn format_clock(zoned: &Zoned, pattern: &str) -> String {
    match jiff::fmt::strtime::format(pattern, zoned) {
        Ok(s) => s,
        Err(_) => "--".to_string(),
    }
}

#[allow(dead_code, reason = "public clock helper consumed by later milestones")]
pub fn now_zoned() -> Zoned {
    Zoned::now()
}

#[cfg(test)]
mod tests {
    use jiff::civil;

    use super::{format_clock, Zoned};

    fn fixed_zoned() -> Zoned {
        civil::date(2024, 7, 15)
            .at(16, 24, 59, 0)
            .in_tz("UTC")
            .expect("fixed tz")
    }

    #[test]
    fn formats_time_pattern() {
        assert_eq!(format_clock(&fixed_zoned(), "%H:%M:%S"), "16:24:59");
    }

    #[test]
    fn formats_date_pattern() {
        assert_eq!(format_clock(&fixed_zoned(), "%Y-%m-%d"), "2024-07-15");
    }

    #[test]
    fn invalid_pattern_yields_placeholder() {
        assert_eq!(format_clock(&fixed_zoned(), "%J"), "--");
    }
}
