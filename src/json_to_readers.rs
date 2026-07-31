use crate::monitor::{self, MonitorState};
use once_cell::sync::Lazy;
use std::collections::HashMap;
type ReaderFunction = fn(&MonitorState) -> String;

macro_rules! dispatch_table {
    ( $( $json_name:expr => $fn:path ),* $(,)? ) => {
        static DISPATCH_TABLE: Lazy<HashMap<String, ReaderFunction>> = Lazy::new(|| {
            let mut m = HashMap::new();
            $(
                m.insert($json_name.to_string(), $fn as ReaderFunction);
            )*
            m
        });
    };
}

dispatch_table! {
    "read_uptime" => monitor::read_uptime,
    "read_cpu_freq" => monitor::read_cpu_freq,
    "read_ram" => monitor::read_ram,
    "read_swap" => monitor::read_swap,
    "read_cpu_usage" => monitor::read_cpu_usage,
    "read_processes_count" => monitor::read_processes_count,
    "read_host" => monitor::read_host,
    "read_disks" => monitor::read_disks,
    "read_network_upload" => monitor::read_network_upload,
    "read_network_download" => monitor::read_network_download,
    "read_gpu_renderer" => monitor::read_gpu_renderer,
    "read_gpu_usage" => monitor::read_gpu_usage,
    "read_vram" => monitor::read_vram,
    "read_vram_busy" => monitor::read_vram_busy,
    "read_gpu_clock" => monitor::read_gpu_clock,
    "read_temp_junction" => monitor::read_temp_junction,
    "read_temp_die" => monitor::read_temp_die,
    "read_temp_memory" => monitor::read_temp_memory,
    "read_power_draw" => monitor::read_power_draw,
    "read_voltage" => monitor::read_voltage,"read_shader_clock" => monitor::read_shader_clock,
    "read_memory_clock" => monitor::read_memory_clock,
    "read_fragment_clock" => monitor::read_fragment_clock,
    "read_soc_clock" => monitor::read_soc_clock,
    "read_fan" => monitor::read_fan,
    "read_pcie_lane" => monitor::read_pcie_lane,
    "read_power_state" => monitor::read_power_state,
    "read_performance_state" => monitor::read_performance_state,
}

pub fn call_by_name(state: &MonitorState, method_name: &str) -> String {
    DISPATCH_TABLE
        .get(method_name)
        .map(|f| f(state))
        .unwrap_or_else(|| format!("unknown method `{method_name}`"))
}
