//! Resident memory of one benchmark row, `docs/spec/evals.md` section 5.
//!
//! "Resident memory" means three different things across the rows, so the
//! report names the source of every number it prints:
//!
//! - A llama.cpp row on a graphics card holds the weights in the card's own
//!   memory, not in the process's RSS. RSS alone is wrong for such a row
//!   (HUF-209), so the number is the device memory its server process holds.
//! - A llama.cpp row on an integrated graphics processor holds the weights in
//!   system memory the device maps instead, which the kernel reports as a
//!   separate pool. That row names that pool rather than the card memory it
//!   does not have, so the printed label always matches the pool measured.
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

/// The fdinfo keys that report the card memory one DRM client holds right now.
///
/// `amdgpu` writes `drm-resident-vram`, and newer kernels number the region as
/// `drm-resident-vram0`. The Intel and Xe drivers name the same region
/// `drm-resident-local<n>` on a card with memory of its own. A driver that
/// names no region of its own reports through the generic helper of the
/// kernel, which calls the card's memory `drm-resident-memory`. `nouveau` is
/// the driver of that kind this project supports. The proprietary NVIDIA
/// driver is another matter, because it may expose no DRM fdinfo at all.
const CARD_KEYS: [&str; 3] = [
    "drm-resident-vram",
    "drm-resident-local",
    "drm-resident-memory",
];

/// The fdinfo keys that report the system memory one DRM client maps.
///
/// This is where an integrated graphics processor holds the weights, because
/// it owns no memory of its own. `amdgpu` calls that pool `drm-resident-gtt`
/// and the Intel and Xe drivers call it `drm-resident-system<n>`.
const SHARED_KEYS: [&str; 2] = ["drm-resident-gtt", "drm-resident-system"];

/// The fdinfo key that tells two open files of one DRM client apart.
const CLIENT_KEY: &str = "drm-client-id";

/// Where the number in the Resident memory column came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The growth of this process's own peak RSS, for an in-process engine.
    Growth,
    /// The resident set size of the engine's server process.
    ServerRss,
    /// The card memory the engine's server process holds.
    Device,
    /// The system memory the engine's server process maps onto the device.
    DeviceShared,
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
            Source::DeviceShared => "the system memory its server process maps onto an integrated graphics processor, read from the DRM fdinfo of that process rather than from its RSS",
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
    match device_resident_reading(pid) {
        Some(reading) => reading,
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
pub fn device_resident_reading(pid: u32) -> Option<Reading> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/fdinfo")).ok()?;
    let files: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect();
    device_reading(&files)
}

/// The device memory of one process, and which pool it was read from.
///
/// The card memory is asked for first, because a card with memory of its own
/// is where a discrete graphics processor holds the weights. The system memory
/// the device maps is the answer only when the process holds no card memory at
/// all, which is what an integrated graphics processor reports. The two pools
/// are never summed into one number, because one cell may name one pool only.
///
/// A process with no DRM client, or one holding nothing in either pool,
/// answers `None`, which is what sends its row back to RSS.
pub fn device_reading(fdinfo: &[String]) -> Option<Reading> {
    let card = pool_bytes(fdinfo, &CARD_KEYS);
    if card > 0 {
        return Some(Reading::new(Some(card), Source::Device));
    }
    let shared = pool_bytes(fdinfo, &SHARED_KEYS);
    (shared > 0).then(|| Reading::new(Some(shared), Source::DeviceShared))
}

/// One memory pool of a process, summed over the DRM clients it opened.
///
/// A process opens the render node more than once, and every open file reports
/// the whole client rather than its own share. So the sum is taken over
/// `drm-client-id` and never over files, which would count one allocation as
/// many.
fn pool_bytes(fdinfo: &[String], keys: &[&str]) -> u64 {
    let mut per_client: HashMap<String, u64> = HashMap::new();
    for file in fdinfo {
        let Some(client) = value_of(file, CLIENT_KEY) else {
            continue;
        };
        let held: u64 = file
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(key, _)| keys.iter().any(|wanted| key.trim().starts_with(wanted)))
            .filter_map(|(_, value)| size_bytes(value))
            .sum();
        let entry = per_client.entry(client).or_default();
        *entry = (*entry).max(held);
    }
    per_client.values().sum()
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

/// The seam that fixes what a server row measured, for tests.
///
/// A server reading comes from a live process on this machine, so a case that
/// wants a measured row would otherwise need LanguageTool or llama.cpp
/// installed and running. A test sets this variable to a byte count, and every
/// server row reports that number instead of reading the machine. The value is
/// what the row measured, so it also decides the tier bar of
/// `bench::weights::tier_objection`.
///
/// Only a test may set it. The in-process engine reads its own peak RSS, which
/// every machine answers, so this seam leaves that row alone. A cloud row
/// measures nothing here and keeps `None`.
pub const RESIDENT_SEAM: &str = "GRAMMACHY_BENCH_RESIDENT_BYTES";

/// The byte count [`RESIDENT_SEAM`] names, or `None` when no test set one.
pub fn seam_bytes() -> Option<u64> {
    parse_seam(std::env::var(RESIDENT_SEAM).ok().as_deref())
}

/// A seam value is a plain byte count. Anything else reads as no seam at all,
/// so a stray empty variable never turns a real reading into a wrong number.
fn parse_seam(value: Option<&str>) -> Option<u64> {
    value?.trim().parse().ok()
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
        amdgpu_pools(client, vram_kib, 14_352)
    }

    /// The same file, with both memory pools named.
    ///
    /// An integrated graphics processor owns no memory of its own, so it
    /// reports the weights under `drm-resident-gtt` and no card memory at all.
    fn amdgpu_pools(client: &str, vram_kib: u64, gtt_kib: u64) -> String {
        format!(
            "pos:\t0\n\
             drm-driver:\tamdgpu\n\
             drm-client-id:\t{client}\n\
             drm-pdev:\t0000:65:00.0\n\
             drm-total-vram:\t{vram_kib} KiB\n\
             drm-shared-vram:\t0\n\
             drm-resident-vram:\t{vram_kib} KiB\n\
             drm-purgeable-vram:\t0\n\
             drm-total-gtt:\t{gtt_kib} KiB\n\
             drm-resident-gtt:\t{gtt_kib} KiB\n\
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
    fn only_a_plain_byte_count_seams_a_server_reading() {
        assert_eq!(parse_seam(Some("2200000000")), Some(2_200_000_000));
        assert_eq!(parse_seam(Some(" 8000000000 ")), Some(8_000_000_000));
        assert_eq!(parse_seam(None), None);
        assert_eq!(parse_seam(Some("")), None);
        assert_eq!(parse_seam(Some("2.2 GB")), None);
    }

    #[test]
    fn a_unit_that_does_not_exist_has_no_main_process() {
        assert_eq!(unit_main_pid("grammachy-no-such-unit-for-a-test"), None);
    }

    #[test]
    fn the_device_number_counts_one_client_once_however_many_files_report_it() {
        // The same client, reported by two open files, is one allocation.
        let one_client = [amdgpu_fdinfo("13", 91_656), amdgpu_fdinfo("13", 91_656)];

        assert_eq!(
            device_reading(&one_client),
            Some(Reading::new(Some(91_656 * 1_024), Source::Device))
        );

        let two_clients = [amdgpu_fdinfo("13", 91_656), amdgpu_fdinfo("14", 8_000)];

        assert_eq!(
            device_reading(&two_clients),
            Some(Reading::new(Some((91_656 + 8_000) * 1_024), Source::Device))
        );
    }

    #[test]
    fn a_card_row_names_its_card_memory_alone_and_never_adds_the_shared_pool() {
        let card = [amdgpu_pools("13", 91_656, 14_352)];

        assert_eq!(
            device_reading(&card),
            Some(Reading::new(Some(91_656 * 1_024), Source::Device)),
            "two pools are two numbers, so one cell names one of them"
        );
    }

    #[test]
    fn an_integrated_processor_holds_the_weights_in_the_shared_pool_not_in_rss() {
        // An APU carves out no card memory for the weights, so the whole
        // footprint lands in the system memory the device maps.
        let integrated = [amdgpu_pools("13", 0, 1_800_000)];

        assert_eq!(
            device_reading(&integrated),
            Some(Reading::new(Some(1_800_000 * 1_024), Source::DeviceShared)),
            "an iGPU row reads the shared pool rather than falling back to RSS"
        );

        // The same client, reported by two open files, is still one allocation.
        let two_files = [
            amdgpu_pools("13", 0, 1_800_000),
            amdgpu_pools("13", 0, 1_800_000),
        ];

        assert_eq!(
            device_reading(&two_files),
            Some(Reading::new(Some(1_800_000 * 1_024), Source::DeviceShared))
        );
    }

    #[test]
    fn a_driver_on_the_generic_helper_still_reports_its_card_memory() {
        // `nouveau` names no region of its own, so the kernel writes the
        // generic `memory` region for it rather than an `amdgpu` or Xe name.
        let nouveau = "pos:\t0\n\
                       drm-driver:\tnouveau\n\
                       drm-client-id:\t42\n\
                       drm-pdev:\t0000:01:00.0\n\
                       drm-total-memory:\t1835008 KiB\n\
                       drm-shared-memory:\t0\n\
                       drm-resident-memory:\t1835008 KiB\n";

        assert_eq!(
            device_reading(&[nouveau.to_string()]),
            Some(Reading::new(Some(1_835_008 * 1_024), Source::Device)),
            "a nouveau row reads the card rather than falling back to RSS"
        );
    }

    #[test]
    fn a_process_with_no_device_memory_reports_none_so_its_row_keeps_rss() {
        assert_eq!(device_reading(&[]), None);
        assert_eq!(device_reading(&[NO_DEVICE.to_string()]), None);
        assert_eq!(device_reading(&[amdgpu_pools("13", 0, 0)]), None);
    }

    #[test]
    fn the_intel_and_xe_drivers_report_the_same_regions_under_another_name() {
        let xe = "drm-driver:\txe\n\
                  drm-client-id:\t7\n\
                  drm-total-local0:\t524288 KiB\n\
                  drm-resident-local0:\t524288 KiB\n";

        assert_eq!(
            device_reading(&[xe.to_string()]),
            Some(Reading::new(Some(524_288 * 1_024), Source::Device))
        );

        let integrated = "drm-driver:\txe\n\
                          drm-client-id:\t7\n\
                          drm-total-system0:\t262144 KiB\n\
                          drm-resident-system0:\t262144 KiB\n";

        assert_eq!(
            device_reading(&[integrated.to_string()]),
            Some(Reading::new(Some(262_144 * 1_024), Source::DeviceShared))
        );
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
        let sources = [
            Source::Growth,
            Source::ServerRss,
            Source::Device,
            Source::DeviceShared,
            Source::Provider,
        ];
        for source in sources {
            assert!(!source.line().is_empty());
            assert!(!source.line().ends_with('.'), "the report adds the stop");
        }

        let lines: std::collections::HashSet<&str> =
            sources.iter().map(|source| source.line()).collect();
        assert_eq!(
            lines.len(),
            sources.len(),
            "two sources that print one sentence tell the reader nothing apart"
        );
    }
}
