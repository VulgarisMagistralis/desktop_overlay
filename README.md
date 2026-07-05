# Desktop Overlay

A lightweight, hardware-accelerated desktop overlay built with Rust and GTK4. It provides real-time system monitoring (CPU/GPU) and an interactive "snowflake" animation module, designed to stay unobtrusive on your desktop.

## Demo

![Project Demo](assets/overlay.webm)

## Features

- **System Monitoring**: Real-time tracking of CPU and GPU metrics using `sysinfo`.
- **GTK4 & Layer Shell**: Uses `gtk4-layer-shell` for high-quality, desktop-integrated window layers (ideal for Wayland).
- **Interactive Snowflake Module**: An animated snowflake overlay that can be toggled via the UI.
- **Low Overhead**: Built with Rust for maximum performance and minimal system impact.

## Prerequisites

Before building, ensure you have the following installed:
- [Rust & Cargo](https://rustup.rs/)
- GTK4 development libraries
- `gtk4-layer-shell` library (for Wayland support)

## Getting Started

### Build and Run

To run the application directly with the Wayland backend (recommended):

```bash
GDK_BACKEND=wayland cargo run --bin desktop_overlay
```

### Development

The project uses a modular structure:
- `src/main.rs`: Entry point and GTK application setup.
- `src/app.rs`: Central application state and builder pattern implementation.
- `src/monitor.rs`: System monitoring logic (CPU, GPU).
- `src/snowflake.rs`: Implementation of the interactive snowflake animation.
- `src/ui/`: UI definitions using XML and CSS.

## Dependencies

- `sysinfo`: System information retrieval.
- `gtk4-layer-shell`: Wayland layer shell integration.
- `gtk4`: GTK4 toolkit for the user interface.
- `winit`: Window handling.
