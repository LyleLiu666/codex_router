# Dependency Upgrade Design

## Goal
Stop the macOS 26.2 startup crash caused by `objc2` signature verification inside `winit` by upgrading the UI stack to a newer, macOS-compatible set of crates.

## Context and Root Cause
The crash occurs during `winit` monitor enumeration (`NSScreen` fast enumeration) with `objc2 0.4.1`/`icrate 0.0.4`, which expects a signed return type (`q`) but receives an unsigned (`Q`) on macOS 26.2. This is a runtime ABI mismatch, not app logic. The most reliable fix is to move the UI stack forward to versions that use newer `objc2`/`icrate` bindings that track current macOS signatures.

## Proposed Approach
Upgrade `eframe` to `0.33.3` (pulling `winit 0.30.x` and `objc2 0.5.x`) and `tray-icon` to `0.21.3` (pulling `objc2 0.6.x`). Add a minimal, reproducible smoke example (`examples/winit_monitor_smoke.rs`) that creates a `winit` event loop and calls `primary_monitor().scale_factor()`; this crashes today and should succeed after the upgrade. Then resolve any API changes in `eframe`/`tray-icon` by compiling and making minimal compatibility edits.

## Risks and Mitigations
- **API changes** in `tray-icon`/`eframe`: keep changes minimal and focused, compile frequently.
- **Behavior changes** in UI/tray: rely on the smoke example plus app launch to validate.
- **Dependency conflicts**: pin to known versions and update `Cargo.lock` via build.

## Testing Strategy
1. Run `cargo run --example winit_monitor_smoke` before upgrading to confirm the crash (failing test).
2. After upgrading, re-run the example to ensure it exits cleanly.
3. Run `cargo test` to ensure existing unit tests remain green.
