//! Resident memory of one benchmark row, spec section 13.1.
//!
//! Two engines are servers and one runs in process, so "resident memory" means
//! two different things and the report says which:
//!
//! - A server engine reports the RSS of its server process, found through the
//!   transient unit that spec section 4 names. That is what the user pays while
//!   the engine is warm.
//! - The in-process engine reports how far this process's own peak RSS grew
//!   over the run, because it has no process of its own. Harper builds its
//!   dictionary and rule set on the first Check, which is the cost worth naming.
//!
//! Everything here reads `/proc`, so a machine without it reports nothing
//! rather than a wrong number.

use std::process::Command;

/// What one row prints when the number could not be read.
pub const UNKNOWN: &str = "not measured";

/// Current resident set size of one process, in bytes.
pub fn resident_bytes(pid: u32) -> Option<u64> {
    field_bytes(&format!("/proc/{pid}/status"), "VmRSS:")
}

/// Peak resident set size of this process, in bytes.
///
/// `VmHWM` is a high-water mark, so it never falls. Two readings around a run
/// give the growth that run caused.
pub fn peak_resident_bytes() -> Option<u64> {
    field_bytes("/proc/self/status", "VmHWM:")
}

/// The main process of one systemd user unit, or `None` when it does not run.
pub fn unit_main_pid(unit: &str) -> Option<u32> {
    let output = Command::new("systemctl")
        .args(["--user", "show", "--property=MainPID", "--value", unit])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
    {
        Ok(0) | Err(_) => None,
        Ok(pid) => Some(pid),
    }
}

/// One `/proc` status field, whose value is a number and a unit such as `kB`.
fn field_bytes(path: &str, field: &str) -> Option<u64> {
    let status = std::fs::read_to_string(path).ok()?;
    let line = status.lines().find(|line| line.starts_with(field))?;
    let mut parts = line.split_whitespace().skip(1);
    let value: u64 = parts.next()?.parse().ok()?;
    let scale = match parts.next() {
        Some("kB") => 1_024,
        Some("mB") => 1_024 * 1_024,
        None => 1,
        Some(_) => return None,
    };
    Some(value * scale)
}

/// A byte count as one table cell.
///
/// Megabytes below a gigabyte and one decimal above it, because a row that
/// reads `7.3 GB` says more about the machine tier than `7311 MB` does.
pub fn cell(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return UNKNOWN.to_string();
    };
    if bytes == 0 {
        return "under 1 MB".to_string();
    }
    if bytes >= 1_000_000_000 {
        return format!("{:.1} GB", bytes as f64 / 1_000_000_000.0);
    }
    let megabytes = bytes as f64 / 1_000_000.0;
    if megabytes < 1.0 {
        return "under 1 MB".to_string();
    }
    format!("{megabytes:.0} MB")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_count_reads_as_megabytes_below_a_gigabyte() {
        assert_eq!(cell(Some(141_000_000)), "141 MB");
        assert_eq!(cell(Some(731_000_000)), "731 MB");
        assert_eq!(cell(Some(999_000_000)), "999 MB");
    }

    #[test]
    fn a_byte_count_reads_as_gigabytes_above_one() {
        assert_eq!(cell(Some(7_300_000_000)), "7.3 GB");
        assert_eq!(cell(Some(1_000_000_000)), "1.0 GB");
    }

    #[test]
    fn a_number_that_could_not_be_read_says_so_rather_than_printing_zero() {
        assert_eq!(cell(None), UNKNOWN);
        assert_eq!(cell(Some(0)), "under 1 MB");
        assert_eq!(cell(Some(400_000)), "under 1 MB");
    }

    #[test]
    fn this_process_reports_its_own_peak_resident_memory() {
        let peak = peak_resident_bytes().expect("this platform has /proc/self/status");

        assert!(peak > 0, "a running process holds resident memory");
    }

    #[test]
    fn a_unit_that_does_not_exist_has_no_main_process() {
        assert_eq!(unit_main_pid("grammachy-no-such-unit-for-a-test"), None);
    }
}
