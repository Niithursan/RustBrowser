//! Read-only numeric fixture reports. No commands or native capabilities are accepted.
use std::fmt;

pub const HTML: &str = include_str!("validation.html");

#[derive(Debug)]
pub struct Report {
    pub scale: f64,
    pub width: f64,
    pub height: f64,
    pub ready_raf_ms: f64,
    pub pointer_raf_ms: f64,
    pub keyboard_raf_ms: f64,
    pub composition_raf_ms: Option<f64>,
}

pub fn parse(title: &str) -> Result<Option<Report>, &'static str> {
    if !title.starts_with("RBVALID1|") {
        return Ok(None);
    }
    if title.len() > 256 {
        return Err("Fixture report exceeds its limit");
    }
    let fields: Vec<_> = title.split('|').collect();
    if fields.len() == 3 && fields[1] == "fail" {
        return Err(match fields[2] {
            "1" => "Fixture readiness failed",
            "2" => "Fixture keyboard event failed",
            "3" => "Fixture composition event failed",
            "4" => "Fixture scaled hit test failed",
            "5" => "Fixture pointer event failed",
            "6" => "Fixture event counts failed",
            "7" => "Fixture IPC boundary failed",
            _ => "Unknown fixture failure",
        });
    }
    if fields.len() != 9 || fields[1] != "pass" {
        return Err("Fixture assertions failed");
    }
    let mut values = [0.0_f64; 6];
    for (destination, source) in values.iter_mut().zip(&fields[2..8]) {
        *destination = source.parse().map_err(|_| "Invalid fixture number")?;
        if !destination.is_finite() || !(0.0..=60_000.0).contains(destination) {
            return Err("Fixture number is outside its range");
        }
    }
    let [
        scale,
        width,
        height,
        ready_raf_ms,
        pointer_raf_ms,
        keyboard_raf_ms,
    ] = values;
    let composition_raf_ms = if fields[8] == "unsupported" {
        None
    } else {
        let value: f64 = fields[8]
            .parse()
            .map_err(|_| "Invalid composition timing")?;
        if !value.is_finite() || !(0.0..=60_000.0).contains(&value) {
            return Err("Invalid composition timing");
        }
        Some(value)
    };
    if !(0.5..=4.0).contains(&scale) || width < 100.0 || height < 100.0 {
        return Err("Invalid fixture viewport");
    }
    Ok(Some(Report {
        scale,
        width,
        height,
        ready_raf_ms,
        pointer_raf_ms,
        keyboard_raf_ms,
        composition_raf_ms,
    }))
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "scale={} viewport={}x{} ready_raf_ms={} pointer_raf_ms={} keyboard_raf_ms={} composition_raf_ms={}",
            self.scale,
            self.width,
            self.height,
            self.ready_raf_ms,
            self.pointer_raf_ms,
            self.keyboard_raf_ms,
            self.composition_raf_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".into())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_accept_only_bounded_finite_numeric_results() {
        let report = parse("RBVALID1|pass|1.25|960|656|30|10|12|14")
            .unwrap()
            .unwrap();
        assert_eq!(report.scale, 1.25);
        assert_eq!(report.composition_raf_ms, Some(14.0));
        assert!(
            parse("RBVALID1|pass|1|960|656|30|10|12|unsupported")
                .unwrap()
                .unwrap()
                .composition_raf_ms
                .is_none()
        );
        assert!(parse("Ordinary page title").unwrap().is_none());
        for invalid in [
            "RBVALID1|fail",
            "RBVALID1|pass|NaN|960|656|30|10|12|14",
            "RBVALID1|pass|1|960|656|inf|10|12|14",
            "RBVALID1|pass|1|960|656|-1|10|12|14",
            "RBVALID1|pass|1|0|656|30|10|12|14",
            "RBVALID1|pass|1|960|656|30|10|12|14|extra",
            "RBVALID1|pass|1|960|656|30|10|12|NaN",
            "RBVALID1|pass|1|960|656|30|10|12|-1",
            "RBVALID1|pass|1|960|656|unsupported|10|12|14",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        assert!(parse(&format!("RBVALID1|{}", "0".repeat(256))).is_err());
    }
}
