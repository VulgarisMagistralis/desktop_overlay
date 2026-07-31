use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug, Default)]
pub struct GpuData {
    pub vendor: String,
    pub gpu_busy: u64,
    pub vram_total_kb: u64,
    pub vram_used_kb: u64,
    pub temp_junction: f64,
    pub temp_memory: f64,
    pub power_draw: f64,
    pub voltage: f64,
    pub shader_clock: String,
    pub memory_clock: String,
    pub fragment_clock: String,
    pub soc_clock: String,
    pub fan_speed: f64,
    pub pcie_lane: String,
    pub power_state: String,
    pub performance_state: String,
}

/// A GPU reader that can probe for availability and read sensor data.
/// Uses `&mut self` so probe results (device paths) can be cached.
pub trait GpuReader {
    fn probe(&mut self) -> bool;
    fn read(&mut self) -> GpuData;
}

/// Auto-detect and return the first matching reader's data.
pub fn read_gpu(readers: &mut [Box<dyn GpuReader>]) -> GpuData {
    for reader in readers.iter_mut() {
        if (*reader).probe() {
            return (*reader).read();
        }
    }
    GpuData::default()
}

/// Default device order: AMD sysfs → NVIDIA nvidia-smi.
pub fn default_gpu_readers() -> Vec<Box<dyn GpuReader>> {
    vec![
        Box::new(AMDSystemFileReader::default()),
        Box::new(NvidiaSMIReader),
    ]
}

#[derive(Default)]
struct AMDSystemFileReader {
    cached_device: Option<PathBuf>,
}

impl GpuReader for AMDSystemFileReader {
    fn probe(&mut self) -> bool {
        if let Some(ref cached) = self.cached_device {
            return cached.join("vendor").exists();
        }

        let drm_base = Path::new("/sys/class/drm");
        if let Ok(entries) = fs::read_dir(drm_base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("card") || name.contains('-') {
                    continue;
                }
                let dev = entry.path().join("device");
                if let Some(vendor) = read_sysfs_string(&dev.join("vendor")) {
                    if vendor.trim() == "0x1002" {
                        self.cached_device = Some(dev);
                        return true;
                    }
                }
            }
        }
        false
    }

    fn read(&mut self) -> GpuData {
        let dev = match &self.cached_device {
            Some(p) => p.clone(),
            None => {
                if !self.probe() {
                    return GpuData::default();
                }
                self.cached_device.clone().unwrap()
            }
        };

        let mut gpu = GpuData::default();

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

        // Try hwmon directly (amdgpu driver, newer kernels) first.
        let hwmon_direct = dev.join("hwmon");
        if hwmon_direct.is_dir() && hwmon_direct.join("temp1_input").exists() {
            read_hwmon_sensors(&mut gpu, &hwmon_direct);
        } else {
            // Fall back to iterating subdirectories (radeon / older kernels).
            if let Ok(hwmons) = fs::read_dir(&hwmon_direct) {
                for hwmon in hwmons.flatten() {
                    read_hwmon_sensors(&mut gpu, &hwmon.path());
                }
            }
        }

        gpu.shader_clock = read_sysfs_active_clock(&dev.join("pp_dpm_sclk"));
        gpu.memory_clock = read_sysfs_active_clock(&dev.join("pp_dpm_mclk"));
        gpu.fragment_clock = read_sysfs_active_clock(&dev.join("pp_dpm_fclk"));
        gpu.soc_clock = read_sysfs_active_clock(&dev.join("pp_dpm_socclk"));

        let pcie_info = read_sysfs_string(&dev.join("current_link_speed"));
        let pcie_width = read_sysfs_string(&dev.join("current_link_width"));
        gpu.pcie_lane = match (pcie_info, pcie_width) {
            (Some(speed), Some(width)) => format!("{} x{}", speed.trim(), width.trim()),
            (Some(speed), None) => speed.trim().to_string(),
            (None, Some(width)) => format!("x{}", width.trim()),
            _ => String::new(),
        };

        gpu.performance_state =
            read_sysfs_first_line(&dev.join("pp_cur_state")).unwrap_or_default();
        gpu.power_state =
            read_sysfs_string(&dev.join("power_dpm_force_performance_level")).unwrap_or_default();

        gpu
    }
}

/// Read hwmon sensor files into GpuData, only overwriting unset values.
fn read_hwmon_sensors(gpu: &mut GpuData, hwmon: &Path) {
    let t = read_sysfs_u64(&hwmon.join("temp1_input"));
    if t > 0 && gpu.temp_junction == 0.0 {
        gpu.temp_junction = t as f64 / 1_000.0;
    }

    let t2 = read_sysfs_u64(&hwmon.join("temp2_input"));
    if t2 > 0 && gpu.temp_memory == 0.0 {
        gpu.temp_memory = t2 as f64 / 1_000.0;
    }

    let fan = read_sysfs_u64(&hwmon.join("fan1_input")) as f64;

    let pwr = read_sysfs_u64(&hwmon.join("power1_average"));
    if pwr > 0 {
        gpu.power_draw = pwr as f64 / 1_000_000.0;
    }

    let volt = read_sysfs_u64(&hwmon.join("in0_input"));
    if volt > 0 {
        gpu.voltage = volt as f64 / 1_000.0;
    }

    // fan1_input is RPM on AMD sysfs — store directly (as f64 for uniformity with NVIDIA %).
    if fan > 0.0 {
        gpu.fan_speed = fan;
    }
}

struct NvidiaSMIReader;

impl GpuReader for NvidiaSMIReader {
    fn probe(&mut self) -> bool {
        std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=driver_version", "--format=csv,noheader"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn read(&mut self) -> GpuData {
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
                let parts: Vec<&str> = s.trim().split(',').map(|p| p.trim()).collect();
                if parts.len() >= 10 {
                    gpu.gpu_busy = parts[0].parse().unwrap_or(0);
                    gpu.vram_used_kb = parts[1].parse::<u64>().unwrap_or(0) * 1024;
                    gpu.vram_total_kb = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                    gpu.temp_junction = parts[3].parse().unwrap_or(0.0);
                    gpu.shader_clock = format!("{} MHz", parts[4]);
                    gpu.memory_clock = format!("{} MHz", parts[5]);
                    gpu.power_draw = parts[6].parse().unwrap_or(0.0);
                    // nvidia-smi fan.speed is a percentage (0-100), not RPM.
                    gpu.fan_speed = parts[8].parse().unwrap_or(0.0);
                    gpu.performance_state = parts[9].to_string();
                }
            }
        }

        gpu
    }
}

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

/// Parse AMD sysfs DPM clock files, preferring the `[currently active]` marker.
fn read_sysfs_active_clock(path: &Path) -> String {
    if let Ok(content) = fs::read_to_string(path) {
        let mut prev_freq = String::new();
        for line in content.lines() {
            for part in line.split_whitespace() {
                // Marker tokens indicate the previous part is the active frequency.
                if part.starts_with("[current") || part == "active]" || part == "*" {
                    if !prev_freq.is_empty() {
                        return prev_freq.clone();
                    }
                // Any token ending with a frequency unit is a candidate.
                } else if part.ends_with("MHz") || part.ends_with("Mhz") || part.ends_with("kHz") {
                    prev_freq = part.to_string();
                }
            }
        }
    }
    String::new()
}
