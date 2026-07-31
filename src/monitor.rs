use crate::services::gpu::{default_gpu_readers, read_gpu, GpuData};
use std::collections::HashMap;
use std::fs;
use sysinfo::{Disk, Disks, Networks, System};
// todo add bluetooth devices, current playing song, connected devices
pub struct MonitorState {
    pub sys: System,
    pub disks: Disks,
    pub networks: Networks,
    pub prev_rx: HashMap<String, u64>,
    pub prev_tx: HashMap<String, u64>,
    pub gpu_data: GpuData,
    pub tick_count: u64,
    gpu_readers: Vec<Box<dyn crate::services::gpu::GpuReader>>,
}

impl MonitorState {
    pub fn new() -> Self {
        let sys = System::new_all();
        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();
        let prev_rx = networks
            .iter()
            .map(|(name, data)| (name.clone(), data.total_received()))
            .collect();
        let prev_tx = networks
            .iter()
            .map(|(name, data)| (name.clone(), data.total_transmitted()))
            .collect();
        let mut gpu_readers = default_gpu_readers();
        let gpu_data = read_gpu(&mut gpu_readers);
        MonitorState {
            sys,
            disks,
            networks,
            prev_rx,
            prev_tx,
            gpu_data,
            tick_count: 0,
            gpu_readers,
        }
    }

    pub fn refresh(&mut self) {
        self.tick_count += 1;
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.disks = Disks::new_with_refreshed_list();
        self.networks = Networks::new_with_refreshed_list();
        self.gpu_data = read_gpu(&mut self.gpu_readers);
    }

    fn interval_secs(&self) -> u64 {
        if self.tick_count <= 1 {
            0
        } else {
            2
        }
    }
}

pub fn read_uptime(_: &MonitorState) -> String {
    let seconds = sysinfo::System::uptime();
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let remaining_seconds = seconds % 60;
    format!("{days}d {hours:02}h {minutes:02}m {remaining_seconds:02}s")
}

pub fn read_cpu_freq(_: &MonitorState) -> String {
    let content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("cpu MHz\t\t: ") {
            let mhz: f64 = val.parse().unwrap_or(0.0);
            return format!("{:.1} GHz", mhz / 1000.0);
        }
    }
    "Loading..".to_string()
}

pub fn read_ram(monitor_state: &MonitorState) -> String {
    let total = monitor_state.sys.total_memory();
    let used = monitor_state.sys.used_memory();
    let total_gb = total as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_gb = used as f64 / 1024.0 / 1024.0 / 1024.0;
    if total > 0 {
        format!(
            "{:.1}/{:.1} GB ({:.0}%)",
            used_gb,
            total_gb,
            (used as f64 / total as f64) * 100.0
        )
    } else {
        "Loading...".to_string()
    }
}

pub fn read_swap(monitor_state: &MonitorState) -> String {
    let total = monitor_state.sys.total_swap();
    let used = monitor_state.sys.used_swap();
    if total == 0 {
        return "Loading...".to_string();
    }
    let total_gb = total as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_gb = used as f64 / 1024.0 / 1024.0 / 1024.0;
    let pct = (used as f64 / total as f64) * 100.0;
    format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, pct)
}

pub fn read_cpu_usage(monitor_state: &MonitorState) -> String {
    let cpus = monitor_state.sys.cpus();
    let total: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
    let avg = total / cpus.len() as f32;
    format!("{:.1}%", avg)
}

pub fn read_processes_count(monitor_state: &MonitorState) -> String {
    format!("{}", monitor_state.sys.processes().len())
}

pub fn read_host(_monitor_state: &MonitorState) -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "Loading...".to_string())
}

/// Format a single disk's usage string.
fn format_disk(disk: &Disk) -> String {
    let total = disk.total_space();
    let avail = disk.available_space();
    let used = total.saturating_sub(avail);
    let total_gb = total as f64 / 1_000_000_000.0;
    let used_gb = used as f64 / 1_000_000_000.0;
    let percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    if percent == 0.0 {
        "Loading ..".to_string()
    } else {
        format!("{:.1}/{:.1} GB ({:.0}%)", used_gb, total_gb, percent)
    }
}

pub fn read_disks(monitor_state: &MonitorState) -> String {
    let parts: Vec<String> = monitor_state
        .disks
        .iter()
        .filter(|d| {
            let fs = d.file_system();
            fs != "tmpfs" && fs != "devtmpfs" && fs != "overlay" && d.mount_point() != "/boot/efi"
        })
        .map(|d| {
            let mount = d.mount_point().to_string_lossy();
            let usage = format_disk(d);
            format!("{}: {}", mount, usage)
        })
        .collect();
    parts.join("\n • ")
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

pub fn read_network_upload(monitor_state: &MonitorState) -> String {
    let interval = monitor_state.interval_secs();
    let divisor = if interval == 0 { 1 } else { interval };
    let mut total_transmission: u64 = 0;
    for (name, data) in &monitor_state.networks {
        let upload = data.total_transmitted();
        let upload_delta = upload.saturating_sub(*monitor_state.prev_tx.get(name).unwrap_or(&0));
        if name != "lo" {
            total_transmission += upload_delta;
        }
    }
    format!("↑ {}", format_speed(total_transmission / divisor))
}

pub fn read_network_download(monitor_state: &MonitorState) -> String {
    let interval = monitor_state.interval_secs();
    let divisor = if interval == 0 { 1 } else { interval };
    let mut total_received: u64 = 0;
    for (name, data) in &monitor_state.networks {
        let received = data.total_received();
        let received_delta =
            received.saturating_sub(*monitor_state.prev_rx.get(name).unwrap_or(&0));
        if name != "lo" {
            total_received += received_delta;
        }
    }
    format!("↓ {}", format_speed(total_received / divisor))
}

pub fn read_gpu_renderer(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.vendor.clone()
}

pub fn read_gpu_usage(monitor_state: &MonitorState) -> String {
    format!("{}%", monitor_state.gpu_data.gpu_busy)
}

pub fn read_vram(monitor_state: &MonitorState) -> String {
    let g = &monitor_state.gpu_data;
    if g.vram_total_kb > 0 {
        let used_mb = g.vram_used_kb as f64 / 1024.0;
        let total_mb = g.vram_total_kb as f64 / 1024.0;
        let pct = (g.vram_used_kb as f64 / g.vram_total_kb as f64) * 100.0;
        format!("{:.0}/{:.0} MB ({:.0}%)", used_mb, total_mb, pct)
    } else {
        "".to_string()
    }
}

pub fn read_vram_busy(monitor_state: &MonitorState) -> String {
    format!("{}%", monitor_state.gpu_data.gpu_busy)
}

pub fn read_gpu_clock(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.shader_clock.clone()
}

pub fn read_temp_junction(monitor_state: &MonitorState) -> String {
    let t = monitor_state.gpu_data.temp_junction;
    if t > 0.0 {
        format!("{:.0} °C", t)
    } else {
        "".to_string()
    }
}

pub fn read_temp_die(monitor_state: &MonitorState) -> String {
    read_temp_junction(monitor_state)
}

pub fn read_temp_memory(monitor_state: &MonitorState) -> String {
    let t = monitor_state.gpu_data.temp_memory;
    if t > 0.0 {
        format!("{:.0} °C", t)
    } else {
        "".to_string()
    }
}

pub fn read_power_draw(monitor_state: &MonitorState) -> String {
    let p = monitor_state.gpu_data.power_draw;
    if p > 0.0 {
        format!("{:.1} W", p)
    } else {
        "".to_string()
    }
}

pub fn read_voltage(monitor_state: &MonitorState) -> String {
    let v = monitor_state.gpu_data.voltage;
    if v > 0.0 {
        format!("{:.3} V", v)
    } else {
        "".to_string()
    }
}

pub fn read_shader_clock(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.shader_clock.clone()
}

pub fn read_memory_clock(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.memory_clock.clone()
}

pub fn read_fragment_clock(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.fragment_clock.clone()
}

pub fn read_soc_clock(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.soc_clock.clone()
}

pub fn read_fan(monitor_state: &MonitorState) -> String {
    let f = monitor_state.gpu_data.fan_speed;
    if f > 0.0 {
        format!("{:.0}", f)
    } else {
        "".to_string()
    }
}

pub fn read_pcie_lane(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.pcie_lane.clone()
}

pub fn read_power_state(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.power_state.clone()
}

pub fn read_performance_state(monitor_state: &MonitorState) -> String {
    monitor_state.gpu_data.performance_state.clone()
}
