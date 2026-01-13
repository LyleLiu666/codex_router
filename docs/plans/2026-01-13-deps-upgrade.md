# Dependency Upgrade Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Upgrade the UI dependency stack to eliminate the macOS 26.2 `objc2`/`winit` crash and verify with a reproducible smoke example.

**Architecture:** Add a small `winit` smoke example that reproduces the crash, then upgrade `eframe`/`tray-icon` and update any API changes so the example and tests pass. Keep changes minimal and focused on dependency compatibility.

**Tech Stack:** Rust, eframe/egui, winit, tray-icon, cargo.

### Task 1: Add failing smoke example

**Files:**
- Modify: `Cargo.toml`
- Create: `examples/winit_monitor_smoke.rs`

**Step 1: Write the failing test (smoke example)**

```rust
fn main() {
    let event_loop = winit::event_loop::EventLoop::new()
        .expect("EventLoop must be created on main thread");
    if let Some(monitor) = event_loop.primary_monitor() {
        let _ = monitor.scale_factor();
    }
}
```

**Step 2: Run example to verify it fails**

Run: `cargo run --example winit_monitor_smoke`
Expected: Panic from `objc2`/`icrate` about invalid message send (type code mismatch).

**Step 3: Commit**

```bash
git add Cargo.toml examples/winit_monitor_smoke.rs
git commit -m "test: add winit monitor smoke example"
```

### Task 2: Upgrade dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Update dependency versions**

```toml
eframe = "0.33.3"
tray-icon = "0.21.3"
```

Also update the dev-dependency for the smoke example:

```toml
[dev-dependencies]
winit = "0.30"
```

**Step 2: Update lockfile and build**

Run: `cargo build`
Expected: Build succeeds or produces API errors to fix in the next task.

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: upgrade eframe and tray-icon"
```

### Task 3: Fix compile errors and verify

**Files:**
- Modify: `src/**/*.rs` (only if needed by new APIs)

**Step 1: Fix compile errors**

Make minimal API adjustments for `eframe`/`tray-icon` changes.

**Step 2: Run example to verify it passes**

Run: `cargo run --example winit_monitor_smoke`
Expected: Exit cleanly with no panic.

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add src Cargo.lock
git commit -m "fix: align code with upgraded UI stack"
```
