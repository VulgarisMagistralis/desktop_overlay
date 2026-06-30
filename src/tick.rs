use crate::app::App;
use crate::monitor;
use crate::ui::Labels;

/// Formatted values for one tick cycle.
pub struct TickOutput {
    pub uptime: String,
    pub cpu_freq: String,
    pub ram: String,
    pub swap: String,
    pub cpu_usage: String,
    pub processes: String,
    pub host: String,
    /// One entry per TRACKED_MOUNTS — "—" if not mounted.
    pub disks: [String; monitor::TRACKED_MOUNTS.len()],
    pub net_up: String,
    pub net_down: String,
    // GPU fields — only populated every 4th tick
    pub gpu_dirty: bool,
    pub gpu_vendor: String,
    pub gpu_usage: String,
    pub vram: String,
    pub vram_busy: String,
    pub temp_junction: String,
    pub temp_die: String,
    pub temp_memory: String,
    pub power_draw: Option<String>,
    pub power_cap: Option<String>,
    pub voltage: Option<String>,
    pub sclk: String,
    pub mclk: String,
    pub fclk: String,
    pub socclk: String,
    pub fan: Option<String>,
    pub pcie: String,
    pub state: String,
    pub pstate: String,
    pub gpu_clock: String,
}

/// Run one monitoring tick: refresh data, format strings.
pub fn update(app: &mut App) -> TickOutput {
    app.tick_count += 1;
    app.monitor_state.refresh();

    let mut out = TickOutput {
        uptime: monitor::read_uptime(),
        cpu_freq: monitor::read_cpu_freq(),
        ram: monitor::read_memory(&app.monitor_state.sys),
        swap: monitor::read_swap(&app.monitor_state.sys),
        cpu_usage: monitor::read_cpu_usage(&app.monitor_state.sys),
        processes: monitor::read_processes(&app.monitor_state.sys),
        host: monitor::read_host(),
        // disks — filled immediately below; String can't be const-initialized
        #[allow(clippy::field_reassign_with_default)]
        disks: Default::default(),
        net_up: String::new(),
        net_down: String::new(),
        gpu_dirty: false,
        gpu_vendor: String::new(),
        gpu_usage: String::new(),
        vram: String::new(),
        vram_busy: String::new(),
        temp_junction: "—".to_string(),
        temp_die: "—".to_string(),
        temp_memory: "—".to_string(),
        power_draw: None,
        power_cap: None,
        voltage: None,
        sclk: "—".to_string(),
        mclk: "—".to_string(),
        fclk: "—".to_string(),
        socclk: "—".to_string(),
        fan: None,
        pcie: "—".to_string(),
        state: "—".to_string(),
        pstate: "—".to_string(),
        gpu_clock: "—".to_string(),
    };

    // Disks — read from cached indices, no HashMap allocation
    out.disks = monitor::read_tracked_disks(
        &app.monitor_state.disk_cache,
        &app.monitor_state.disks,
    );

    // Network
    let interval = if app.tick_count == 1 { 0 } else { 2 };
    let (up, down) = monitor::read_network(
        &app.monitor_state.networks,
        &mut app.monitor_state.prev_rx,
        &mut app.monitor_state.prev_tx,
        interval,
    );
    out.net_up = up;
    out.net_down = down;

    // GPU — every 4th tick
    if app.tick_count % 4 == 1 {
        let gpu = monitor::read_gpu(&app.backends);
        out.gpu_dirty = true;

        out.gpu_vendor = gpu.vendor.clone();
        out.gpu_usage = format!("{}%", gpu.gpu_busy);

        let vram_total_mb = gpu.vram_total_kb as f64 / 1024.0;
        let vram_used_mb = gpu.vram_used_kb as f64 / 1024.0;
        let vram_pct = if gpu.vram_total_kb > 0 {
            (gpu.vram_used_kb as f64 / gpu.vram_total_kb as f64) * 100.0
        } else {
            0.0
        };
        out.vram = format!(
            "{:.0}/{:.0} MB ({:.0}%)",
            vram_used_mb, vram_total_mb, vram_pct
        );
        out.vram_busy = format!("{}%", gpu.gpu_busy);

        out.temp_junction = if gpu.temp_junction > 0.0 {
            format!("{:.0} °C", gpu.temp_junction)
        } else {
            "—".to_string()
        };
        out.temp_die = out.temp_junction.clone();

        out.temp_memory = if gpu.temp_memory > 0.0 {
            format!("{:.0} °C", gpu.temp_memory)
        } else {
            "—".to_string()
        };

        out.power_draw = if gpu.power_draw > 0.0 {
            Some(format!("{:.1} W", gpu.power_draw))
        } else {
            None
        };
        out.power_cap = if gpu.power_cap > 0.0 {
            Some(format!("{:.0} W", gpu.power_cap))
        } else {
            None
        };
        out.voltage = if gpu.voltage > 0.0 {
            Some(format!("{:.3} V", gpu.voltage))
        } else {
            None
        };

        out.sclk = if gpu.sclk != "—" {
            gpu.sclk.clone()
        } else {
            "—".to_string()
        };
        out.mclk = if gpu.mclk != "—" {
            gpu.mclk.clone()
        } else {
            "—".to_string()
        };
        out.fclk = gpu.fclk;
        out.socclk = gpu.socclk;

        out.fan = if gpu.fan_rpm > 0 {
            Some(format!("{} RPM", gpu.fan_rpm))
        } else {
            None
        };

        out.pcie = gpu.pcie_width;
        out.state = gpu.state;
        out.pstate = gpu.pstate;
        out.gpu_clock = out.sclk.clone();
    }

    out
}

/// Apply formatted values to the GTK labels.
pub fn apply_labels(labels: &Labels, output: &TickOutput) {
    labels.uptime.set_label(&output.uptime);
    labels.cpu_freq.set_label(&output.cpu_freq);
    labels.ram.set_label(&output.ram);
    labels.swap.set_label(&output.swap);
    labels.cpu_usage.set_label(&output.cpu_usage);
    labels.processes.set_label(&output.processes);
    labels.host.set_label(&output.host);

    labels.disk_root.set_label(&output.disks[0]);
    labels.disk_mnt_data.set_label(&output.disks[1]);
    labels.disk_mnt_warehouse.set_label(&output.disks[2]);

    labels.net_up.set_label(&output.net_up);
    labels.net_down.set_label(&output.net_down);

    if output.gpu_dirty {
        labels.gpu_renderer.set_label(&output.gpu_vendor);
        labels.gpu_usage.set_label(&output.gpu_usage);
        labels.vram.set_label(&output.vram);
        labels.vram_busy.set_label(&output.vram_busy);
        labels.temp_junction.set_label(&output.temp_junction);
        labels.temp_die.set_label(&output.temp_die);
        labels.temp_memory.set_label(&output.temp_memory);
        if let Some(s) = &output.power_draw {
            labels.power_draw.set_label(s);
        }
        if let Some(s) = &output.power_cap {
            labels.power_cap.set_label(s);
        }
        if let Some(s) = &output.voltage {
            labels.voltage.set_label(s);
        }
        labels.sclk.set_label(&output.sclk);
        labels.mclk.set_label(&output.mclk);
        labels.fclk.set_label(&output.fclk);
        labels.socclk.set_label(&output.socclk);
        if let Some(s) = &output.fan {
            labels.fan.set_label(s);
        }
        labels.pcie.set_label(&output.pcie);
        labels.state.set_label(&output.state);
        labels.pstate.set_label(&output.pstate);
        labels.gpu_clock.set_label(&output.gpu_clock);
    }
}
