//! Oxidium Windows Per-User (HKCU) Self-Extracting Installer & Uninstaller.
//!
//! Built by `scripts/package-windows.ps1` with `OXIDIUM_PAYLOAD_ZIP` pointing to
//! the verified release portable archive and `OXIDIUM_VERSION` set to the workspace version.
//!
//! Usage:
//!   Oxidium-<VER>-windows-x86_64-setup.exe [--silent] [--dir <PATH>] [--no-shortcuts] [--no-registry] [--wait-pid <PID>] [--relaunch]
//!   Oxidium-<VER>-windows-x86_64-setup.exe --uninstall [--silent] [--dir <PATH>] [--no-shortcuts] [--no-registry]

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

const APP_NAME: &str = "Oxidium";
const APP_VERSION: &str = match option_env!("OXIDIUM_VERSION") {
    Some(v) => v,
    None => "0.1.3",
};
const APP_PUBLISHER: &str = "Oxidium Contributors";
const UNINSTALL_REG_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Oxidium";

static PAYLOAD_ZIP: &[u8] = include_bytes!(env!("OXIDIUM_PAYLOAD_ZIP"));

fn default_install_dir() -> PathBuf {
    if let Ok(local) = env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("Programs").join(APP_NAME)
    } else {
        PathBuf::from(".").join(APP_NAME)
    }
}

fn start_menu_dir() -> Option<PathBuf> {
    env::var("APPDATA").ok().map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(APP_NAME)
    })
}

fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

fn run_powershell(script: &str) -> io::Result<()> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "PowerShell step exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    Ok(())
}

fn is_pid_running(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let Ok(output) = Command::new("tasklist")
        .args(["/FI", &filter, "/NH", "/FO", "CSV"])
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let needle = format!(",\"{pid}\",");
    text.lines().any(|line| line.contains(&needle))
}

fn wait_for_pid_exit(pid: u32) {
    println!("Waiting for running Oxidium process (PID {pid}) to exit...");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !is_pid_running(pid) {
            // Brief settle pause for Windows PE handle release
            thread::sleep(Duration::from_millis(300));
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn park_existing_executables(install_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut parked = Vec::new();
    let pid = std::process::id();
    for exe in ["student-journal.exe", "student-journal-mcp.exe", "uninstall.exe"] {
        let live = install_dir.join(exe);
        if !live.exists() {
            continue;
        }
        let default_old = install_dir.join(format!("{exe}.old"));
        if default_old.exists() {
            let _ = fs::remove_file(&default_old);
        }
        if fs::rename(&live, &default_old).is_ok() {
            parked.push((live, default_old));
            continue;
        }
        let pid_old = install_dir.join(format!("{exe}.{pid}.old"));
        if pid_old.exists() {
            let _ = fs::remove_file(&pid_old);
        }
        if fs::rename(&live, &pid_old).is_ok() {
            parked.push((live, pid_old));
        }
    }
    parked
}

fn cleanup_old_files(install_dir: &Path, parked: Vec<(PathBuf, PathBuf)>) {
    for (_, old) in parked {
        let _ = fs::remove_file(old);
    }
    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".old"))
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn create_shortcut(
    lnk_path: &Path,
    target: &Path,
    args: &str,
    work_dir: &Path,
    desc: &str,
) -> io::Result<()> {
    if let Some(parent) = lnk_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $sc = $ws.CreateShortcut('{lnk}'); \
         $sc.TargetPath = '{target}'; \
         $sc.Arguments = '{args}'; \
         $sc.WorkingDirectory = '{wd}'; \
         $sc.Description = '{desc}'; \
         $sc.IconLocation = '{icon},0'; \
         $sc.Save()",
        lnk = ps_escape(&lnk_path.display().to_string()),
        target = ps_escape(&target.display().to_string()),
        args = ps_escape(args),
        wd = ps_escape(&work_dir.display().to_string()),
        desc = ps_escape(desc),
        icon = ps_escape(&work_dir.join("student-journal.exe").display().to_string()),
    );
    run_powershell(&script)
}

fn register_uninstall(install_dir: &Path, uninstaller: &Path) -> io::Result<()> {
    let exe_path = install_dir.join("student-journal.exe");
    let entries: &[(&str, &str, &str)] = &[
        ("DisplayName", "REG_SZ", "Oxidium — Электронный журнал ученика"),
        ("DisplayVersion", "REG_SZ", APP_VERSION),
        ("Publisher", "REG_SZ", APP_PUBLISHER),
        ("InstallLocation", "REG_SZ", &install_dir.display().to_string()),
        ("DisplayIcon", "REG_SZ", &exe_path.display().to_string()),
        (
            "UninstallString",
            "REG_SZ",
            &format!(
                "\"{}\" --uninstall --dir \"{}\"",
                uninstaller.display(),
                install_dir.display()
            ),
        ),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ];
    for (name, ty, val) in entries {
        let status = Command::new("reg")
            .args(["add", UNINSTALL_REG_KEY, "/v", name, "/t", ty, "/d", val, "/f"])
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!("reg add failed for {name}")));
        }
    }
    Ok(())
}

fn unregister_uninstall() {
    let _ = Command::new("reg")
        .args(["delete", UNINSTALL_REG_KEY, "/f"])
        .status();
}

fn do_install(
    install_dir: &Path,
    create_shortcuts: bool,
    register_hkcu: bool,
    wait_pid: Option<u32>,
    relaunch: bool,
) -> io::Result<()> {
    if let Some(pid) = wait_pid {
        wait_for_pid_exit(pid);
    }

    println!("Installing {APP_NAME} v{APP_VERSION} to {}...", install_dir.display());
    fs::create_dir_all(install_dir)?;

    // Rename running payload binaries aside so upgrades succeed cleanly even if Windows keeps handles brief
    let parked_old = park_existing_executables(install_dir);

    // Write embedded portable ZIP to a temporary file and extract cleanly
    let tmp_zip = env::temp_dir().join(format!("oxidium-setup-{}.zip", std::process::id()));
    fs::write(&tmp_zip, PAYLOAD_ZIP)?;

    let extract_script = format!(
        "Expand-Archive -LiteralPath '{zip}' -DestinationPath '{dest}' -Force",
        zip = ps_escape(&tmp_zip.display().to_string()),
        dest = ps_escape(&install_dir.display().to_string()),
    );
    let extract_res = run_powershell(&extract_script);
    let _ = fs::remove_file(&tmp_zip);
    if let Err(e) = extract_res {
        // Attempt rollback of parked binaries if extraction failed
        for (live, old) in &parked_old {
            if !live.exists() && old.exists() {
                let _ = fs::rename(old, live);
            }
        }
        return Err(e);
    }

    // Copy self as uninstall.exe for standard Windows Apps & Features removal
    let uninstaller_path = install_dir.join("uninstall.exe");
    if let Ok(current_exe) = env::current_exe()
        && current_exe != uninstaller_path
    {
        let _ = fs::copy(&current_exe, &uninstaller_path);
    }

    // Best-effort cleanup of .old binaries
    cleanup_old_files(install_dir, parked_old);

    if create_shortcuts
        && let Some(sm_dir) = start_menu_dir()
    {
        let gui = install_dir.join("student-journal.exe");
        create_shortcut(
            &sm_dir.join("Oxidium.lnk"),
            &gui,
            "",
            install_dir,
            "Oxidium — Электронный журнал ученика",
        )?;
        create_shortcut(
            &sm_dir.join("Uninstall Oxidium.lnk"),
            &uninstaller_path,
            &format!("--uninstall --dir \"{}\"", install_dir.display()),
            install_dir,
            "Удалить Oxidium",
        )?;
    }

    if register_hkcu {
        register_uninstall(install_dir, &uninstaller_path)?;
    }

    let gui_path = install_dir.join("student-journal.exe");
    println!("Oxidium v{APP_VERSION} successfully installed to {}", install_dir.display());
    println!("  GUI: {}", gui_path.display());
    println!("  MCP: {}", install_dir.join("student-journal-mcp.exe").display());

    if relaunch && gui_path.exists() {
        println!("Launching updated Oxidium...");
        let _ = Command::new(&gui_path).current_dir(install_dir).spawn();
    }

    Ok(())
}

fn do_uninstall(install_dir: &Path, remove_shortcuts: bool, unregister_hkcu: bool) -> io::Result<()> {
    println!("Uninstalling {APP_NAME} from {}...", install_dir.display());
    if unregister_hkcu {
        unregister_uninstall();
    }
    if remove_shortcuts
        && let Some(sm_dir) = start_menu_dir()
        && sm_dir.exists()
    {
        let _ = fs::remove_dir_all(&sm_dir);
    }

    // Remove installed payload files (leave user database in %LOCALAPPDATA%\oxidium untouched!)
    let files = [
        "student-journal.exe",
        "student-journal.exe.old",
        "student-journal-mcp.exe",
        "student-journal-mcp.exe.old",
        "uninstall.exe.old",
        "LICENSE.txt",
        "THIRD_PARTY_NOTICES.md",
        "MCP-QUICKSTART.md",
        "RELEASE-NOTES.md",
    ];
    for f in files {
        let p = install_dir.join(f);
        if p.exists() {
            let _ = fs::remove_file(&p);
        }
    }
    cleanup_old_files(install_dir, Vec::new());
    let uninst = install_dir.join("uninstall.exe");
    if uninst.exists() && env::current_exe().ok().as_deref() != Some(uninst.as_path()) {
        let _ = fs::remove_file(&uninst);
    }
    let _ = fs::remove_dir(install_dir);
    println!("Oxidium removed cleanly. User journal data in %LOCALAPPDATA%\\oxidium was preserved.");
    Ok(())
}

fn pause_before_exit() {
    print!("Press Enter to close...");
    let _ = io::stdout().flush();
    let mut dummy = String::new();
    let _ = io::stdin().read_line(&mut dummy);
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut silent = false;
    let mut uninstall = false;
    let mut no_shortcuts = false;
    let mut no_registry = false;
    let mut relaunch = false;
    let mut wait_pid: Option<u32> = None;
    let mut custom_dir: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--silent" | "/S" | "/SILENT" => silent = true,
            "--uninstall" => uninstall = true,
            "--no-shortcuts" => no_shortcuts = true,
            "--no-registry" => no_registry = true,
            "--relaunch" => relaunch = true,
            "--wait-pid" if i + 1 < args.len() => {
                i += 1;
                wait_pid = args[i].trim().parse::<u32>().ok();
            }
            "--dir" if i + 1 < args.len() => {
                i += 1;
                custom_dir = Some(PathBuf::from(&args[i]));
            }
            "--help" | "-h" | "/?" => {
                println!("Oxidium v{APP_VERSION} Windows Setup (Per-User HKCU, No UAC Required)");
                println!("Usage:");
                println!("  setup.exe [--silent] [--dir <PATH>] [--no-shortcuts] [--no-registry] [--wait-pid <PID>] [--relaunch]");
                println!("  setup.exe --uninstall [--silent] [--dir <PATH>] [--no-shortcuts] [--no-registry]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let install_dir = custom_dir.unwrap_or_else(default_install_dir);

    if !silent {
        if uninstall {
            print!("Uninstall Oxidium from {}? [y/N]: ", install_dir.display());
        } else {
            print!(
                "Install Oxidium v{APP_VERSION} (per-user, no admin required) to {}? [Y/n]: ",
                install_dir.display()
            );
        }
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_ok() {
            let trimmed = line.trim().to_lowercase();
            if uninstall && trimmed != "y" && trimmed != "yes" && trimmed != "д" {
                println!("Cancelled.");
                return ExitCode::SUCCESS;
            }
            if !uninstall && (trimmed == "n" || trimmed == "no" || trimmed == "н") {
                println!("Cancelled.");
                return ExitCode::SUCCESS;
            }
        }
    }

    let res = if uninstall {
        do_uninstall(&install_dir, !no_shortcuts, !no_registry)
    } else {
        do_install(&install_dir, !no_shortcuts, !no_registry, wait_pid, relaunch)
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Setup error: {e}");
            if !silent {
                pause_before_exit();
            }
            ExitCode::FAILURE
        }
    }
}
