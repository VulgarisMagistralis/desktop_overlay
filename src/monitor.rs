use std::collections::HashMap;
use std::fs;
use std::path::Path;
use sysinfo::{Disk, Disks, Networks, System};

/// Mount points we track — defined once, used everywhere.
pub const TRACKED_MOUNTS: &[&str] = &["/", "/mnt/data", "/mnt/warehouse"];

// -----------------------------------------------------------------------
// Monitor
// -----------------------------------------------------------------------

/// Cached references to disks for our tracked mount points.
#[derive(Default)]
pub struct DiskCache {
    /// Index into the Disks list for each tracked mount point.
    /// `None` means the mount point wasn't found during last refresh.
    indices: [Option<usize>; TRACKED_MOUNTS.len()],
}

impl DiskCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-resolve which disk objects belong to our tracked mount points.
    /// Call this after `disks` is refreshed — mount points rarely change
    /// so the indices stay valid across many ticks.
    pub fn resolve(&mut self, disks: &Disks) {
        for (i, mount) in TRACKED_MOUNTS.iter().enumerate() {
            self.indices[i] = disks
                .iter()
                .position(|d| d.mount_point().to_string_lossy() == *mount);
        }
    }

    /// Get the Disk for each tracked mount point (in order).
    pub fn get<'a>(&self, disks: &'a Disks) -> [Option<&'a Disk>; TRACKED_MOUNTS.len()] {
        let mut result = [None; TRACKED_MOUNTS.len()];
        for (i, &idx) in self.indices.iter().enumerate() {
            if let Some(idx) = idx {
                result[i] = disks.get(idx);
            }
        }
        result
    }
}

pub struct MonitorState {
    pub sys: System,
    pub disks: Disks,
    pub disk_cache: DiskCache,
    pub networks: Networks,
    pub prev_rx: HashMap<String, u64>,
    pub prev_tx: HashMap<String, u64>,
}

impl MonitorState {
    pub fn new() -> Self {
        let sys = System::new_all();

        let disks = Disks::new_with_refreshed_list();
        let mut disk_cache = DiskCache::new();
        disk_cache.resolve(&disks);

        let networks = Networks::new_with_refreshed_list();

        let prev_rx = networks
            .iter()
            .map(|(name, data)| (name.clone(), data.total_received()))
            .collect();
        let prev_tx = networks
            .iter()
            .map(|(name, data)| (name.clone(), data.total_transmitted()))
            .collect();

        MonitorState {
            sys,
            disks,
            disk_cache,
            networks,
            prev_rx,
            prev_tx,
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.disks = Disks::new_with_refreshed_list();
        self.disk_cache.resolve(&self.disks);
        self.networks = Networks::new_with_refreshed_list();
    }
}

// -----------------------------------------------------------------------
// Scalar readers (CPU, memory, disks, network, uptime …)
// -----------------------------------------------------------------------

pub fn read_uptime() -> String {
    let secs = sysinfo::System::uptime();
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{d}d {h:02}h {m:02}m {s:02}s")
}

pub fn read_cpu_freq() -> String {
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("cpu MHz\t\t: ") {
            let mhz: f64 = val.parse().unwrap_or(0.0);
            return format!("{:.1} GHz", mhz / 1000.0);
        }
    }
    "—".to_string()
}

pub fn read_memory(sys: &System) -> String {
    let total = sys.total_memory();
    let used = sys.used_memory();
    let total_gb = total as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_gb = used as f64 / 1024.0 / 1024.0 / 1024.0;
    let pct = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, pct)
}

pub fn read_swap(sys: &System) -> String {
    let total = sys.total_swap();
    let used = sys.used_swap();
    if total == 0 {
        return "—".to_string();
    }
    let total_gb = total as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_gb = used as f64 / 1024.0 / 1024.0 / 1024.0;
    let pct = (used as f64 / total as f64) * 100.0;
    format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, pct)
}

pub fn read_cpu_usage(sys: &System) -> String {
    let cpus = sys.cpus();
    let total: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
    let avg = total / cpus.len() as f32;
    format!("{:.1}%", avg)
}

pub fn read_processes(sys: &System) -> String {
    format!("{}", sys.processes().len())
}

pub fn read_host() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "—".to_string())
}

/// Format a single disk's usage string.
pub fn format_disk(disk: &Disk) -> String {
    let total = disk.total_space();
    let avail = disk.available_space();
    let used = total.saturating_sub(avail);
    let total_gb = total as f64 / 1_000_000_000.0;
    let used_gb = used as f64 / 1_000_000_000.0;
    let pct = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, pct)
}

/// Read and format the tracked mount points using cached disk indices.
/// Returns one String per entry in TRACKED_MOUNTS ("—" if not mounted).
pub fn read_tracked_disks(
    cache: &DiskCache,
    disks: &Disks,
) -> [String; TRACKED_MOUNTS.len()] {
    let gotten = cache.get(disks);
    gotten.map(|disk| disk.map_or_else(|| "—".to_string(), format_disk))
}

pub fn read_network(
    networks: &Networks,
    prev_rx: &mut HashMap<String, u64>,
    prev_tx: &mut HashMap<String, u64>,
    interval_secs: u64,
) -> (String, String) {
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    for (name, data) in networks {
        let rx = data.total_received();
        let tx = data.total_transmitted();
        let drx = rx.saturating_sub(*prev_rx.get(name).unwrap_or(&0));
        let dtx = tx.saturating_sub(*prev_tx.get(name).unwrap_or(&0));
        if name != "lo" {
            total_rx += drx;
            total_tx += dtx;
        }
        prev_rx.insert(name.clone(), rx);
        prev_tx.insert(name.clone(), tx);
    }
    let rx_speed = if interval_secs > 0 {
        total_rx / interval_secs
    } else {
        0
    };
    let tx_speed = if interval_secs > 0 {
        total_tx / interval_secs
    } else {
        0
    };
    let up = format_speed(tx_speed);
    let down = format_speed(rx_speed);
    (format!("↑ {}", up), format!("↓ {}", down))
}

fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_000_000 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_000_000.0)
    } else if bytes_per_sec >= 1_000 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1_000.0)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

// -----------------------------------------------------------------------
// GPU Backend Trait & Implementations
// -----------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct GpuData {
    pub vendor: String,
    pub gpu_busy: u64,
    pub vram_total_kb: u64,
    pub vram_used_kb: u64,
    pub temp_junction: f64,
    pub temp_memory: f64,
    pub power_draw: f64,
    pub power_cap: f64,
    pub voltage: f64,
    pub sclk: String,
    pub mclk: String,
    pub fclk: String,
    pub socclk: String,
    pub fan_rpm: u64,
    pub pcie_width: String,
    pub state: String,
    pub pstate: String,
}

/// A GPU backend that can probe for availability and read sensor data.
pub trait GpuBackend {
    /// Returns true if this backend detects a GPU of its type.
    fn probe(&self) -> bool;

    /// Read GPU metrics. Only meaningful after a successful `probe()`.
    fn read(&self) -> GpuData;
}

/// Auto-detect and return the first matching backend's data.
pub fn read_gpu(backends: &[Box<dyn GpuBackend>]) -> GpuData {
    for backend in backends {
        if backend.probe() {
            return backend.read();
        }
    }
    GpuData::default()
}

/// Default device order: AMD sysfs → NVIDIA nvidia-smi.
pub fn default_backends() -> Vec<Box<dyn GpuBackend>> {
    vec![Box::new(AmDsysfsBackend), Box::new(NvidiaSmiBackend)]
}

// -----------------------------------------------------------------------
// AMD sysfs Backend
// -----------------------------------------------------------------------

struct AmDsysfsBackend;

impl GpuBackend for AmDsysfsBackend {
    fn probe(&self) -> bool {
        let drm_base = Path::new("/sys/class/drm");
        if let Ok(entries) = fs::read_dir(drm_base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("card") || name.contains("-") {
                    continue;
                }
                let dev = entry.path().join("device");
                if let Some(vendor) = read_sysfs_string(&dev.join("vendor")) {
                    if vendor.trim() == "0x1002" {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn read(&self) -> GpuData {
        let mut gpu = GpuData::default();
        let drm_base = Path::new("/sys/class/drm");

        if let Ok(entries) = fs::read_dir(drm_base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("card") || name.contains("-") {
                    continue;
                }
                let dev = entry.path().join("device");

                gpu.vendor = read_sysfs_string(&dev.join("vendor"))
                    .map(|v| match v.trim() {
                        "0x1002" => "AMD".to_string(),
                        "0x10de" => "NVIDIA".to_string(),
                        "0x8086" => "Intel".to_string(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();

                gpu.gpu_busy = read_sysfs_u64(&dev.join("gpu_busy_percent"));
                gpu.vram_total_kb = read_sysfs_u64(&dev.join("mem_info_vram_total")) / 1024;
                gpu.vram_used_kb = read_sysfs_u64(&dev.join("mem_info_vram_used")) / 1024;

                let hwmon_dir = dev.join("hwmon");
                if let Ok(hwmons) = fs::read_dir(&hwmon_dir) {
                    for hwmon in hwmons.flatten() {
                        let t = read_sysfs_u64(&hwmon.path().join("temp1_input"));
                        if t > 0 && gpu.temp_junction == 0.0 {
                            gpu.temp_junction = t as f64 / 1000.0;
                        }
                        let t2 = read_sysfs_u64(&hwmon.path().join("temp2_input"));
                        if t2 > 0 && gpu.temp_memory == 0.0 {
                            gpu.temp_memory = t2 as f64 / 1000.0;
                        }
                        let fan = read_sysfs_u64(&hwmon.path().join("fan1_input"));
                        if fan > 0 {
                            gpu.fan_rpm = fan;
                        }
                        let pwr = read_sysfs_u64(&hwmon.path().join("power1_average"));
                        if pwr > 0 {
                            gpu.power_draw = pwr as f64 / 1_000_000.0;
                        }
                        let pwr_cap = read_sysfs_u64(&hwmon.path().join("power1_cap"));
                        if pwr_cap > 0 {
                            gpu.power_cap = pwr_cap as f64 / 1_000_000.0;
                        }
                        let volt = read_sysfs_u64(&hwmon.path().join("in0_input"));
                        if volt > 0 {
                            gpu.voltage = volt as f64 / 1_000_000.0;
                        }
                    }
                }
                gpu.sclk = read_sysfs_active_clock(&dev.join("pp_dpm_sclk"));
                gpu.mclk = read_sysfs_active_clock(&dev.join("pp_dpm_mclk"));
                gpu.fclk = read_sysfs_active_clock(&dev.join("pp_dpm_fclk"));
                gpu.socclk = read_sysfs_active_clock(&dev.join("pp_dpm_socclk"));
                let pcie_info = read_sysfs_string(&dev.join("current_link_speed"));
                let pcie_width = read_sysfs_string(&dev.join("current_link_width"));
                gpu.pcie_width = match (pcie_info, pcie_width) {
                    (Some(speed), Some(width)) => format!("{} x{}", speed.trim(), width.trim()),
                    (Some(speed), None) => speed.trim().to_string(),
                    (None, Some(width)) => format!("x{}", width.trim()),
                    _ => "—".to_string(),
                };

                gpu.pstate = read_sysfs_first_line(&dev.join("pp_cur_state"))
                    .unwrap_or_else(|| "—".to_string());

                gpu.state = read_sysfs_string(&dev.join("power_dpm_force_performance_level"))
                    .unwrap_or_else(|| "—".to_string());
                break;
            }
        }

        gpu
    }
}

// -----------------------------------------------------------------------
// NVIDIA nvidia-smi Backend
// -----------------------------------------------------------------------

struct NvidiaSmiBackend;

impl GpuBackend for NvidiaSmiBackend {
    fn probe(&self) -> bool {
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=driver_version", "--format=csv,noheader"])
            .output()
        {
            return output.status.success();
        }
        false
    }

    fn read(&self) -> GpuData {
        let mut gpu = GpuData::default();
        gpu.vendor = "NVIDIA".to_string();

        if let Ok(out) = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,clocks.current.graphics,clocks.current.memory,power.draw,power.limit,fan.speed,pstate",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            if let Ok(s) = String::from_utf8(out.stdout) {
                let parts: Vec<&str> = s.trim().split(',').map(|s| s.trim()).collect();
                if parts.len() >= 10 {
                    gpu.gpu_busy = parts[0].parse().unwrap_or(0);
                    gpu.vram_used_kb = parts[1].parse::<u64>().unwrap_or(0) * 1024;
                    gpu.vram_total_kb = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                    gpu.temp_junction = parts[3].parse().unwrap_or(0.0);
                    gpu.sclk = format!("{} MHz", parts[4]);
                    gpu.mclk = format!("{} MHz", parts[5]);
                    gpu.power_draw = parts[6].parse().unwrap_or(0.0);
                    gpu.power_cap = parts[7].parse().unwrap_or(0.0);
                    gpu.fan_rpm = parts[8].parse().unwrap_or(0);
                    gpu.pstate = parts[9].to_string();
                }
            }
        }

        gpu
    }
}

// -----------------------------------------------------------------------
// sysfs helpers (private)
// -----------------------------------------------------------------------

fn read_sysfs_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_sysfs_u64(path: &Path) -> u64 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn read_sysfs_first_line(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.to_string()))
}

fn read_sysfs_active_clock(path: &Path) -> String {
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // We want the part that looks like a frequency, e.g., "2141Mhz"
            // In "1: 2141Mhz *", parts are ["1:", "2141Mhz", "*"]
            for part in &parts {
                if part.ends_with("Mhz") || part.ends_with("MHz") {
                    return part.to_string();
                }
            }
        }
    }
    "—".to_string()
}
