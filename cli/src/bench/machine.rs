//! The machine a benchmark file was measured on, `docs/spec/evals.md` section 5.
//!
//! The recommended model must fit a named memory tier, so a benchmark file that
//! does not say which machine produced it cannot be compared with the next one.
//! The tier is the total RAM rounded down to the usual sizes, because that is
//! how a model card states its requirement.

/// The RAM sizes a tier is named after.
const TIERS: [u64; 6] = [4, 8, 16, 32, 64, 128];

/// What the benchmark file says about the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub cpus: usize,
    /// Total RAM in gigabytes, rounded to the nearest whole one.
    pub ram_gb: u64,
}

impl Machine {
    /// Read this machine.
    pub fn here() -> Machine {
        Machine {
            cpus: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(0),
            ram_gb: total_ram_gb().unwrap_or(0),
        }
    }

    /// The memory tier the machine fills, such as `16 GB`.
    pub fn tier(&self) -> String {
        let tier = TIERS
            .iter()
            .rev()
            .find(|size| self.ram_gb >= **size)
            .copied()
            .unwrap_or(0);
        format!("{tier} GB")
    }

    /// The one line the report prints.
    pub fn line(&self) -> String {
        format!(
            "{} tier, {} CPUs, {} GB RAM",
            self.tier(),
            self.cpus,
            self.ram_gb
        )
    }
}

/// Total RAM in gigabytes from `/proc/meminfo`.
fn total_ram_gb() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    // Round to the nearest gigabyte, because the kernel reserves a little.
    Some((kilobytes as f64 / 1_048_576.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tier_is_the_ram_size_rounded_down_to_a_usual_one() {
        assert_eq!(
            Machine {
                cpus: 8,
                ram_gb: 27
            }
            .tier(),
            "16 GB"
        );
        assert_eq!(
            Machine {
                cpus: 8,
                ram_gb: 16
            }
            .tier(),
            "16 GB"
        );
        assert_eq!(Machine { cpus: 4, ram_gb: 7 }.tier(), "4 GB");
        assert_eq!(
            Machine {
                cpus: 64,
                ram_gb: 250
            }
            .tier(),
            "128 GB"
        );
    }

    #[test]
    fn a_machine_smaller_than_every_tier_names_no_tier_rather_than_the_wrong_one() {
        assert_eq!(Machine { cpus: 1, ram_gb: 2 }.tier(), "0 GB");
    }

    #[test]
    fn this_machine_reads_its_own_size() {
        let machine = Machine::here();

        assert!(machine.cpus > 0, "the CPU count is readable");
        assert!(machine.ram_gb > 0, "the RAM size is readable");
    }
}
