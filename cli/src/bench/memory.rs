//! Resident memory of one benchmark row, `docs/spec/evals.md` section 5.
//!
//! "Resident memory" means three different things across the rows, so the
//! report names the source of every number it prints:
//!
//! - A llama.cpp row on a graphics processor holds the weights on the device,
//!   not in the process's RSS. RSS alone is wrong for such a row (HUF-209), so
//!   the number is the device memory its server process holds.
//! - A server engine on the CPU reports the RSS of its server process, found
//!   through the transient unit that spec section 4 names.
//! - The in-process engine reports how far this process's own peak RSS grew
//!   over the run, because it has no process of its own. Harper builds its
//!   dictionary and rule set on the first Check, which is the cost worth
//!   naming.
//!
//! The device number comes from the DRM fdinfo of the server process, which is
//! what the kernel reports for the Vulkan allocation of one client. The
//! llama.cpp `/metrics` endpoint is not the source: it is off unless the server
//! runs with `--metrics`, and it carries token and slot counters only, with no
//! memory gauge.
//!
//! Everything here reads `/proc`, so a machine without it reports nothing
//! rather than a wrong number.

use std::collections::HashMap;
use std::process::Command;

/// What one row prints when the number could not be read.
pub const UNKNOWN: &str = "not measured";

/// The fdinfo keys that report device memory one DRM client holds right now.
///
/// `amdgpu` writes `drm-resident-vram`, and newer kernels number the region as
/// `drm-resident-vram0`. The Intel and Xe drivers name the same region
/// `drm-resident-local<n>` on a card with memory of its own.
const DEVICE_KEYS: [&str; 2] = ["drm-resident-vram", "drm-resident-local"];

/// The fdinfo key that tells two open files of one DRM client apart.
const CLIENT_KEY: &str = "drm-client-id";

/// Where the number in the Resident memory column came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The growth of this process's own peak RSS, for an in-process engine.
    Growth,
    /// The resident set size of the engine's server process.
    ServerRss,
    /// The device memory the engine's server process holds.
    Device,
    /// Nothing to measure, because the model runs off this machine.
    Provider,
}

impl Source {
    /// The sentence the report prints under the table.
    pub fn line(self) -> &'static str {
        match self {
            Source::Growth => {
                "the growth of this process's own peak RSS, because it runs in process"
            }
            Source::ServerRss => "the RSS of its server process",
            Source::Device => "the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS",
            Source::Provider => "not measured, because the model runs on the provider's machine",
        }
    }
}

/// What one row measured, and where the number came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub bytes: Option<u64>,
    pub source: Source,
}

impl Reading {
    /// A row whose number came from the named source.
    pub fn new(bytes: Option<u64>, source: Source) -> Reading {
        Reading { bytes, source }
    }

    /// The row's cell in the Resident memory column.
    pub fn cell(&self) -> String {
        cell(self.bytes)
    }
}

/// The reading for one server process: the device first, its RSS failing that.
///
/// A llama.cpp server built against the CPU backend opens no DRM client at
/// all, so it reports no device memory and keeps RSS. A server on a graphics
/// processor holds the weights on the device, where RSS cannot see them.
pub fn server_reading(pid: Option<u32>) -> Reading {
    let Some(pid) = pid else {
        return Reading::new(None, Source::ServerRss);
    };
    match device_resident_bytes(pid) {
        Some(bytes) => Reading::new(Some(bytes), Source::Device),
        None => Reading::new(resident_bytes(pid), Source::ServerRss),
    }
}

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

/// The device memory one process holds, or `None` when it holds none.
pub fn device_resident_bytes(pid: u32) -> Option<u64> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/fdinfo")).ok()?;
    let files: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect();
    device_bytes(&files)
}

/// The device memory of one process, summed over the DRM clients it opened.
///
/// A process opens the render node more than once, and every open file reports
/// the whole client rather than its own share. So the sum is taken over
/// `drm-client-id` and never over files, which would count one allocation as
/// many. A process with no DRM client, or one holding nothing, answers `None`,
/// which is what sends its row back to RSS.
pub fn device_bytes(fdinfo: &[String]) -> Option<u64> {
    let mut per_client: HashMap<String, u64> = HashMap::new();
    for file in fdinfo {
        let Some(client) = value_of(file, CLIENT_KEY) else {
            continue;
        };
        let held: u64 = file
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(key, _)| {
                DEVICE_KEYS
                    .iter()
                    .any(|wanted| key.trim().starts_with(wanted))
            })
            .filter_map(|(_, value)| size_bytes(value))
            .sum();
        let entry = per_client.entry(client).or_default();
        *entry = (*entry).max(held);
    }
    let total: u64 = per_client.values().sum();
    (total > 0).then_some(total)
}

/// The value of one fdinfo key, trimmed.
fn value_of(file: &str, key: &str) -> Option<String> {
    file.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_string())
}

/// One fdinfo size, such as `91656 KiB` or a bare byte count.
fn size_bytes(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let amount: u64 = parts.next()?.parse().ok()?;
    let scale = match parts.next() {
        None => 1,
        Some("KiB") => 1_024,
        Some("MiB") => 1_024 * 1_024,
        Some("GiB") => 1_024 * 1_024 * 1_024,
        Some(_) => return None,
    };
    Some(amount * scale)
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

    /// One open file of a llama.cpp server on an `amdgpu` card, as the kernel
    /// of a real machine writes it, trimmed to the keys that matter.
    fn amdgpu_fdinfo(client: &str, vram_kib: u64) -> String {
        format!(
            "pos:\t0\n\
             drm-driver:\tamdgpu\n\
             drm-client-id:\t{client}\n\
             drm-pdev:\t0000:65:00.0\n\
             drm-total-vram:\t{vram_kib} KiB\n\
             drm-shared-vram:\t0\n\
             drm-resident-vram:\t{vram_kib} KiB\n\
             drm-purgeable-vram:\t0\n\
             drm-resident-gtt:\t14352 KiB\n\
             drm-memory-vram:\t{vram_kib} KiB\n\
             drm-engine-compute:\t26891966 ns\n"
        )
    }

    /// One open file of a process that holds no graphics device at all.
    const NO_DEVICE: &str = "pos:\t0\nflags:\t02100002\nmnt_id:\t28\nino:\t525\n";

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

    #[test]
    fn the_device_number_counts_one_client_once_however_many_files_report_it() {
        // The same client, reported by two open files, is one allocation.
        let one_client = [amdgpu_fdinfo("13", 91_656), amdgpu_fdinfo("13", 91_656)];

        assert_eq!(device_bytes(&one_client), Some(91_656 * 1_024));

        let two_clients = [amdgpu_fdinfo("13", 91_656), amdgpu_fdinfo("14", 8_000)];

        assert_eq!(device_bytes(&two_clients), Some((91_656 + 8_000) * 1_024));
    }

    #[test]
    fn a_process_with_no_device_memory_reports_none_so_its_row_keeps_rss() {
        assert_eq!(device_bytes(&[]), None);
        assert_eq!(device_bytes(&[NO_DEVICE.to_string()]), None);
        assert_eq!(device_bytes(&[amdgpu_fdinfo("13", 0)]), None);
    }

    #[test]
    fn the_intel_and_xe_drivers_report_the_same_region_under_another_name() {
        let xe = "drm-driver:\txe\n\
                  drm-client-id:\t7\n\
                  drm-total-local0:\t524288 KiB\n\
                  drm-resident-local0:\t524288 KiB\n";

        assert_eq!(device_bytes(&[xe.to_string()]), Some(524_288 * 1_024));
    }

    #[test]
    fn a_server_with_no_device_falls_back_to_the_rss_of_its_process() {
        assert_eq!(server_reading(None), Reading::new(None, Source::ServerRss));

        // This process opens no render node, so it falls back to its own RSS.
        let own = server_reading(Some(std::process::id()));
        assert_eq!(own.source, Source::ServerRss);
        assert!(own.bytes.is_some(), "this process reports an RSS");
    }

    #[test]
    fn every_source_names_itself_in_one_sentence() {
        for source in [
            Source::Growth,
            Source::ServerRss,
            Source::Device,
            Source::Provider,
        ] {
            assert!(!source.line().is_empty());
            assert!(!source.line().ends_with('.'), "the report adds the stop");
        }
    }
}
