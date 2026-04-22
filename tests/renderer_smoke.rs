//! Renderer smoke test (TMB-019 / Phase 7).
//!
//! Drives `examples/smoke_test.td` through the real Taida interpreter to
//! verify the renderer core (`BufferPut` / `BufferWrite` / `BufferFillRect`
//! / `RenderFull` / `BufferDiff` / `RenderOps` / `RenderFrame`) actually
//! mutates cells, emits ANSI, and diff-engines frames. The pre-existing
//! `tests/renderer_facade.rs` is a pseudo-test that recomputes expected
//! strings in Rust without calling `.td` code; this file complements it.
//!
//! Binary resolution:
//!   - `TAIDA_BIN` env var (absolute path)
//!   - `../../../../target/{debug,release}/taida` relative to the terminal crate
//!     (the path into the top-level taida monorepo)
//!   - `taida` on `$PATH`
//!
//! The test is skipped (with a loud `eprintln!`) if none of the above
//! resolve to an executable. This matches the skip conventions used by
//! `sigwinch_external_chain.rs` so the suite stays green in envs that
//! don't ship the `taida` CLI.

use std::path::PathBuf;
use std::process::Command;

const PASS_RENDERER: &str = "PASS:renderer_ops";
const PASS_ALL: &str = "PASS:all_smoke_tests";

fn locate_taida() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("TAIDA_BIN") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    // The terminal crate lives at:
    //   taida/.dev/official-package-repos/terminal/
    // so the main taida monorepo root is three parents up:
    //   terminal/ -> official-package-repos/ -> .dev/ -> taida/
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let taida_root = crate_dir.parent()?.parent()?.parent()?;
    for profile in &["debug", "release"] {
        let candidate = taida_root.join("target").join(profile).join("taida");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // $PATH fallback.
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in path_env.split(':') {
            let candidate = PathBuf::from(dir).join("taida");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn cdylib_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libtaida_lang_terminal.dylib"
    } else if cfg!(windows) {
        "taida_lang_terminal.dll"
    } else {
        "libtaida_lang_terminal.so"
    }
}

fn locate_cdylib(crate_dir: &std::path::Path) -> Option<PathBuf> {
    let name = cdylib_name();
    let candidates = [
        crate_dir.join("native").join(name),
        crate_dir.join("target").join("debug").join(name),
        crate_dir.join("target").join("release").join(name),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn stage_workspace(
    crate_dir: &std::path::Path,
    work: &std::path::Path,
    cdylib: &std::path::Path,
) -> std::io::Result<PathBuf> {
    let pkg_dir = work
        .join(".taida")
        .join("deps")
        .join("taida-lang")
        .join("terminal");
    let native_dir = pkg_dir.join("native");
    let taida_dir = pkg_dir.join("taida");
    std::fs::create_dir_all(&native_dir)?;
    std::fs::create_dir_all(&taida_dir)?;

    for entry in std::fs::read_dir(crate_dir.join("taida"))? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) == Some("td") {
            let dst = taida_dir.join(entry.file_name());
            std::fs::copy(entry.path(), dst)?;
        }
    }

    std::fs::copy(
        crate_dir.join("native").join("addon.toml"),
        native_dir.join("addon.toml"),
    )?;
    let lock_src = crate_dir.join("native").join("addon.lock.toml");
    if lock_src.exists() {
        std::fs::copy(lock_src, native_dir.join("addon.lock.toml"))?;
    }
    std::fs::copy(cdylib, native_dir.join(cdylib_name()))?;
    std::fs::copy(crate_dir.join("packages.tdm"), pkg_dir.join("packages.tdm"))?;

    std::fs::write(work.join("packages.tdm"), "<<<@a.4\n")?;
    let main_td = work.join("main.td");
    std::fs::copy(crate_dir.join("examples").join("smoke_test.td"), &main_td)?;
    Ok(main_td)
}

#[test]
fn renderer_smoke_executes_all_ops() {
    let taida = match locate_taida() {
        Some(p) => p,
        None => {
            eprintln!(
                "skipping renderer_smoke: taida binary not found — set TAIDA_BIN or build the main crate"
            );
            return;
        }
    };
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cdylib = match locate_cdylib(&crate_dir) {
        Some(p) => p,
        None => {
            eprintln!("skipping renderer_smoke: cdylib not built — run `cargo build` first");
            return;
        }
    };

    let work = tempfile::tempdir().expect("create temp workspace");
    let main_td = stage_workspace(&crate_dir, work.path(), &cdylib).expect("stage workspace");

    let output = Command::new(&taida)
        .arg(&main_td)
        .output()
        .expect("spawn taida");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "taida exited non-zero\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    assert!(
        stdout.contains(PASS_RENDERER),
        "renderer_ops marker missing — renderer.td regressed to no-op?\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains(PASS_ALL),
        "all_smoke_tests marker missing\nstdout:\n{}",
        stdout
    );

    // Spot-check concrete renderer assertions so regressions surface with a
    // precise error instead of just a missing PASS marker.
    let must_contain = [
        "BufferPut.text:X",
        "BufferWrite.0:A",
        "BufferWrite.1:B",
        "BufferWrite.2:C",
        "BufferWrite.truncated:C",
        "BufferWrite.styled_fg:red",
        "BufferFillRect.4:*",
        "BufferFillRect.5:*",
        "RenderFull.len_gt10:yes",
        "BufferDiff.identical.ops_len:0",
        "BufferDiff.identical.requires_full:false",
        "BufferDiff.single_change.ops_len:1",
        "BufferDiff.single_change.op_kind:1",
        "BufferDiff.single_change.op_col:2",
        "BufferDiff.single_change.op_row:1",
        "BufferDiff.single_change.op_text:X",
        "BufferDiff.size_change.requires_full:true",
        "RenderOps.len_gt0:yes",
        "RenderFrame.size_change.len_gt10:yes",
        "RenderFrame.diff.len_gt0:yes",
        "RenderFrame.same.empty:yes",
    ];
    for marker in must_contain {
        assert!(
            stdout.contains(marker),
            "missing assertion marker `{}`\nstdout:\n{}",
            marker,
            stdout
        );
    }
}
