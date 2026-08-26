//! What `doctor` sees of this machine, and the hardware tier it reads from it.
//!
//! Every field here is a recorded fact rather than a call, so the report is a
//! pure function of [`Facts`] and every test runs against facts it wrote
//! itself. [`Facts::collect`] is the one place that touches the real machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::args::CheckOptions;
use crate::engines::languagetool;
use crate::engines::openai::{self, endpoint};

/// Where `/sys` exposes the graphics devices of this machine.
const DRM_CLASS: &str = "/sys/class/drm";

/// Where the `ggml-cpu` and `ggml-vulkan` packages drop their backend
/// libraries. `llama-cpp` carries no compute backend of its own, so a
/// `llama-server` beside an empty directory here starts and then answers
/// nothing (spec section 4).
const GGML_BACKENDS: &str = "/usr/lib/ggml";

/// DRM drivers that drive no real graphics processor.
///
/// `simpledrm` is the framebuffer the kernel sets up before a driver loads,
/// and the rest are virtual devices. A machine that has only these runs
/// llama.cpp on the CPU, because no Vulkan device is there to use.
const SOFTWARE_DRIVERS: [&str; 8] = [
    "simpledrm",
    "offb",
    "vkms",
    "vgem",
    "bochs-drm",
    "qxl",
    "vmwgfx",
    "virtio_gpu",
];

/// Whether a transient unit runs right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Running,
    Stopped,
    /// `systemctl --user` could not be asked, so the CLI cannot start the unit.
    Unknown,
}

/// One graphics device, as `/sys/class/drm` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmCard {
    /// Basename of `card<N>/device/driver`, such as `amdgpu`.
    pub driver: String,
    /// Basename of the `card<N>/device` link, such as `0000:65:00.0`.
    /// `None` for a card that hangs off no PCI address.
    pub pci_address: Option<String>,
}

impl DrmCard {
    /// Whether the device is a graphics processor at all.
    fn is_gpu(&self) -> bool {
        !self.driver.is_empty() && !SOFTWARE_DRIVERS.contains(&self.driver.as_str())
    }

    /// Whether the device sits on its own PCIe bus rather than on the CPU
    /// package. Every integrated processor answers bus `00`, so a card that
    /// answers anything else came in through a slot.
    fn is_discrete(&self) -> bool {
        match self.pci_address.as_deref().and_then(pci_bus) {
            Some(bus) => bus != "00",
            None => false,
        }
    }
}

/// The bus part of a PCI address: `0000:65:00.0` answers `65`.
fn pci_bus(address: &str) -> Option<&str> {
    address.split(':').nth(1)
}

/// What llama.cpp runs on, which is the one thing hardware decides.
///
/// Spec section 4: hardware tiers affect only the install step, where `doctor`
/// names the Vulkan or the CPU backend package for the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareTier {
    /// A graphics card on its own PCIe bus.
    DiscreteGpu,
    /// A graphics processor on the CPU package.
    IntegratedGpu,
    /// No graphics processor, so llama.cpp runs on the CPU alone.
    Cpu,
}

impl HardwareTier {
    /// The ggml backend package that makes `llama-cpp` run on this tier.
    pub fn backend_package(self) -> &'static str {
        match self {
            HardwareTier::DiscreteGpu | HardwareTier::IntegratedGpu => "ggml-vulkan",
            HardwareTier::Cpu => "ggml-cpu",
        }
    }

    /// The value the JSON envelope carries.
    pub fn as_str(self) -> &'static str {
        match self {
            HardwareTier::DiscreteGpu => "discrete-gpu",
            HardwareTier::IntegratedGpu => "integrated-gpu",
            HardwareTier::Cpu => "cpu",
        }
    }
}

/// Read the tier from the graphics devices of one machine.
pub fn tier_of(cards: &[DrmCard]) -> HardwareTier {
    let gpus: Vec<&DrmCard> = cards.iter().filter(|card| card.is_gpu()).collect();
    if gpus.is_empty() {
        return HardwareTier::Cpu;
    }
    if gpus.iter().any(|card| card.is_discrete()) {
        return HardwareTier::DiscreteGpu;
    }
    HardwareTier::IntegratedGpu
}

/// Everything one `doctor` run knows about this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The companion binary that is running, when its path is knowable.
    pub binary: Option<PathBuf>,
    /// The version of that binary.
    pub version: String,
    /// The LanguageTool launcher the pacman package installs.
    pub languagetool_launcher: Option<PathBuf>,
    /// The `bin/java` the launcher runs, through `JAVA_HOME` or the default JVM.
    pub java: Option<PathBuf>,
    /// The address the `languagetool` adapter talks to.
    pub languagetool_address: String,
    /// The llama.cpp server the `llama-cpp` package installs.
    pub llama_server: Option<PathBuf>,
    /// Where `grammachy setup` keeps the weights.
    pub models_directory: Option<PathBuf>,
    /// The model name the Settings ask for.
    pub model: String,
    /// The weights file that model name stands for.
    pub model_file: Option<PathBuf>,
    /// The chat endpoint address, or why the base URL is not usable at all
    /// (spec section 4: the host must stay on this machine).
    pub openai_endpoint: Result<String, String>,
    pub languagetool_unit: UnitState,
    pub llama_unit: UnitState,
    /// The graphics devices, which decide the tier.
    pub cards: Vec<DrmCard>,
    /// The backend library file names under [`GGML_BACKENDS`], such as
    /// `libggml-cpu-zen4.so` and `libggml-vulkan.so`.
    pub ggml_backends: Vec<String>,
}

/// One compute backend `llama-server` can load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Vulkan,
}

impl Backend {
    /// The package that installs it.
    pub fn package(self) -> &'static str {
        match self {
            Backend::Cpu => "ggml-cpu",
            Backend::Vulkan => "ggml-vulkan",
        }
    }

    /// Whether one library file name belongs to this backend.
    fn owns(self, library: &str) -> bool {
        match self {
            Backend::Cpu => library.starts_with("libggml-cpu"),
            Backend::Vulkan => library.starts_with("libggml-vulkan"),
        }
    }
}

/// Whether one backend is installed, from the library file names alone.
pub fn has_backend(libraries: &[String], backend: Backend) -> bool {
    libraries.iter().any(|library| backend.owns(library))
}

impl Facts {
    /// Read this machine. The one function here that is not a pure value.
    pub fn collect(options: &CheckOptions) -> Self {
        let models_directory = openai::unit::models_directory();
        let model_file = models_directory
            .as_deref()
            .and_then(|directory| openai::unit::model_file(directory, &options.openai_model).ok());

        Facts {
            binary: std::env::current_exe().ok(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            languagetool_launcher: existing_file(languagetool::unit::PACKAGE_LAUNCHER),
            java: languagetool::unit::java_home()
                .ok()
                .map(|home| PathBuf::from(home).join("bin/java")),
            languagetool_address: languagetool::Config::from_env().address,
            llama_server: existing_file(openai::unit::PACKAGE_SERVER),
            models_directory,
            model: options.openai_model.clone(),
            model_file,
            openai_endpoint: endpoint::parse(&options.openai_base_url)
                .map(|endpoint| endpoint.address()),
            languagetool_unit: unit_state(languagetool::unit::UNIT_NAME),
            llama_unit: unit_state(openai::unit::UNIT_NAME),
            cards: drm_cards(Path::new(DRM_CLASS)),
            ggml_backends: ggml_backends(Path::new(GGML_BACKENDS)),
        }
    }

    /// The tier these facts put the machine in.
    pub fn tier(&self) -> HardwareTier {
        tier_of(&self.cards)
    }

    /// The backends this tier wants, in the order `doctor` names them.
    ///
    /// Every tier wants `ggml-cpu`, because llama.cpp runs the parts no other
    /// backend takes on the CPU. A graphics processor wants `ggml-vulkan` too.
    pub fn wanted_backends(&self) -> Vec<Backend> {
        match self.tier() {
            HardwareTier::Cpu => vec![Backend::Cpu],
            HardwareTier::DiscreteGpu | HardwareTier::IntegratedGpu => {
                vec![Backend::Cpu, Backend::Vulkan]
            }
        }
    }

    /// The wanted backends this machine does not have.
    pub fn missing_backends(&self) -> Vec<Backend> {
        self.wanted_backends()
            .into_iter()
            .filter(|backend| !has_backend(&self.ggml_backends, *backend))
            .collect()
    }
}

fn existing_file(path: &str) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

/// Ask systemd whether one transient unit runs.
///
/// `is-active` exits non-zero for an inactive unit, so the exit status says
/// nothing useful and only the word on stdout does.
fn unit_state(unit: &str) -> UnitState {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", unit])
        .output();
    match output {
        Ok(output) => match String::from_utf8_lossy(&output.stdout).trim() {
            "active" | "activating" | "reloading" => UnitState::Running,
            "" => UnitState::Unknown,
            _ => UnitState::Stopped,
        },
        Err(_) => UnitState::Unknown,
    }
}

/// The backend library file names under one `ggml` directory.
///
/// A missing directory reads as no backend at all, which is what a machine
/// with `llama-cpp` and neither ggml package looks like.
fn ggml_backends(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut libraries: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("libggml-") && name.contains(".so"))
        .collect();
    libraries.sort();
    libraries
}

/// The graphics devices under one `/sys/class/drm` directory.
///
/// The directory holds one entry per card and one per connector, such as
/// `card1-HDMI-A-1`. Only the bare `card<N>` entries are devices.
fn drm_cards(class: &Path) -> Vec<DrmCard> {
    let Ok(entries) = std::fs::read_dir(class) else {
        return Vec::new();
    };

    let mut cards: Vec<(String, DrmCard)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            is_card(&name).then(|| (name, read_card(&entry.path())))
        })
        .collect();
    cards.sort_by(|left, right| left.0.cmp(&right.0));
    cards.into_iter().map(|(_, card)| card).collect()
}

/// `card1` is a device; `card1-DP-1` is one of its connectors.
fn is_card(name: &str) -> bool {
    match name.strip_prefix("card") {
        Some(rest) => !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()),
        None => false,
    }
}

fn read_card(path: &Path) -> DrmCard {
    let device = path.join("device");
    DrmCard {
        driver: link_name(&device.join("driver")).unwrap_or_default(),
        pci_address: link_name(&device),
    }
}

/// The last component of what one symlink points at.
fn link_name(path: &Path) -> Option<String> {
    let target = std::fs::read_link(path).ok()?;
    Some(target.file_name()?.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(driver: &str, pci_address: Option<&str>) -> DrmCard {
        DrmCard {
            driver: driver.to_string(),
            pci_address: pci_address.map(str::to_string),
        }
    }

    #[test]
    fn a_card_on_its_own_bus_is_the_discrete_tier() {
        // This machine: an amdgpu card at 0000:65:00.0.
        let cards = [card("amdgpu", Some("0000:65:00.0"))];

        assert_eq!(tier_of(&cards), HardwareTier::DiscreteGpu);
        assert_eq!(tier_of(&cards).backend_package(), "ggml-vulkan");
    }

    #[test]
    fn a_card_on_the_cpu_package_is_the_integrated_tier() {
        // An Intel laptop: the graphics processor answers bus 00.
        let cards = [card("i915", Some("0000:00:02.0"))];

        assert_eq!(tier_of(&cards), HardwareTier::IntegratedGpu);
        assert_eq!(tier_of(&cards).backend_package(), "ggml-vulkan");
    }

    #[test]
    fn only_a_framebuffer_is_the_cpu_tier() {
        // A headless server: the kernel framebuffer and nothing else.
        let cards = [card("simpledrm", None)];

        assert_eq!(tier_of(&cards), HardwareTier::Cpu);
        assert_eq!(tier_of(&cards).backend_package(), "ggml-cpu");
    }

    #[test]
    fn no_card_at_all_is_the_cpu_tier() {
        assert_eq!(tier_of(&[]), HardwareTier::Cpu);
        assert_eq!(HardwareTier::Cpu.backend_package(), "ggml-cpu");
    }

    #[test]
    fn a_discrete_card_beside_an_integrated_one_wins() {
        // A laptop with switchable graphics lists both.
        let cards = [
            card("i915", Some("0000:00:02.0")),
            card("nvidia", Some("0000:01:00.0")),
        ];

        assert_eq!(tier_of(&cards), HardwareTier::DiscreteGpu);
    }

    #[test]
    fn a_virtual_card_does_not_earn_the_vulkan_backend() {
        let cards = [card("virtio_gpu", Some("0000:07:00.0"))];

        assert_eq!(tier_of(&cards), HardwareTier::Cpu);
    }

    #[test]
    fn every_tier_has_its_own_envelope_value() {
        let values = [
            HardwareTier::DiscreteGpu.as_str(),
            HardwareTier::IntegratedGpu.as_str(),
            HardwareTier::Cpu.as_str(),
        ];

        assert_eq!(values, ["discrete-gpu", "integrated-gpu", "cpu"]);
    }

    #[test]
    fn connectors_are_not_devices() {
        assert!(is_card("card0"));
        assert!(is_card("card12"));
        assert!(!is_card("card1-HDMI-A-1"));
        assert!(!is_card("card1-Writeback-1"));
        assert!(!is_card("renderD128"));
        assert!(!is_card("card"));
    }

    #[test]
    fn a_missing_drm_directory_reads_as_no_card() {
        assert!(drm_cards(Path::new("/sys/class/no-such-drm-class")).is_empty());
    }

    #[test]
    fn a_missing_ggml_directory_reads_as_no_backend() {
        let libraries = ggml_backends(Path::new("/usr/lib/no-such-ggml"));

        assert!(libraries.is_empty());
        assert!(!has_backend(&libraries, Backend::Cpu));
        assert!(!has_backend(&libraries, Backend::Vulkan));
    }

    #[test]
    fn a_cpu_backend_is_read_from_any_of_its_microarchitecture_libraries() {
        // ggml-cpu ships one library per microarchitecture, never a bare name.
        let libraries = ["libggml-cpu-zen4.so".to_string()];

        assert!(has_backend(&libraries, Backend::Cpu));
        assert!(!has_backend(&libraries, Backend::Vulkan));
    }

    #[test]
    fn the_vulkan_backend_is_one_library() {
        let libraries = ["libggml-vulkan.so".to_string()];

        assert!(has_backend(&libraries, Backend::Vulkan));
        assert!(!has_backend(&libraries, Backend::Cpu));
    }

    #[test]
    fn every_backend_names_its_package() {
        assert_eq!(Backend::Cpu.package(), "ggml-cpu");
        assert_eq!(Backend::Vulkan.package(), "ggml-vulkan");
    }
}
