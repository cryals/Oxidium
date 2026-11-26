//! Oxidium Windows Per-User (HKCU) Self-Extracting Installer & Uninstaller.
//!
//! Built by `scripts/package-windows.ps1` with `OXIDIUM_PAYLOAD_ZIP` pointing to
//! the verified release portable archive `Oxidium-0.1.0-windows-x86_64-portable.zip`.
//!
//! Usage:
//!   Oxidium-0.1.0-windows-x86_64-setup.exe [--silent] [--dir <PATH>] [--no-shortcuts]
//!   Oxidium-0.1.0-windows-x86_64-setup.exe --uninstall [--silent] [--dir <PATH>]

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const APP_NAME: &str = "Oxidium";
const APP_VERSION: &str = "0.1.0";
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
    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "PowerShell step exited with status {status}"
        )));
    }
    Ok(())
}

fn create_shortcut(lnk_path: &Path, target: &Path, args: &str, work_dir: &Path, desc: &str) -> io::Result<()> {
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
            &format!("\"{}\" --uninstall --dir \"{}\"", uninstaller.display(), install_dir.display()),
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

fn do_install(install_dir: &Path, create_shortcuts: bool, register_hkcu: bool) -> io::Result<()> {
    println!("Installing {APP_NAME} v{APP_VERSION} to {}...", install_dir.display());
    fs::create_dir_all(install_dir)?;

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
    extract_res?;

    // Copy self as uninstall.exe for standard Windows Apps & Features removal
    let uninstaller_path = install_dir.join("uninstall.exe");
    if let Ok(current_exe) = env::current_exe()
        && current_exe != uninstaller_path
    {
        let _ = fs::copy(&current_exe, &uninstaller_path);
    }

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

    println!("Oxidium v{APP_VERSION} successfully installed to {}", install_dir.display());
    println!("  GUI: {}", install_dir.join("student-journal.exe").display());
    println!("  MCP: {}", install_dir.join("student-journal-mcp.exe").display());
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
        "student-journal-mcp.exe",
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
    let uninst = install_dir.join("uninstall.exe");
    if uninst.exists() && env::current_exe().ok().as_deref() != Some(uninst.as_path()) {
        let _ = fs::remove_file(&uninst);
    }
    let _ = fs::remove_dir(install_dir);
    println!("Oxidium removed cleanly. User journal data in %LOCALAPPDATA%\\oxidium was preserved.");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut silent = false;
    let mut uninstall = false;
    let mut no_shortcuts = false;
    let mut no_registry = false;
    let mut custom_dir: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--silent" | "/S" | "/SILENT" => silent = true,
            "--uninstall" => uninstall = true,
            "--no-shortcuts" => no_shortcuts = true,
            "--no-registry" => no_registry = true,
            "--dir" if i + 1 < args.len() => {
                i += 1;
                custom_dir = Some(PathBuf::from(&args[i]));
            }
            "--help" | "-h" | "/?" => {
                println!("Oxidium v{APP_VERSION} Windows Setup (Per-User HKCU, No UAC Required)");
                println!("Usage:");
                println!("  setup.exe [--silent] [--dir <PATH>] [--no-shortcuts] [--no-registry]");
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
        do_install(&install_dir, !no_shortcuts, !no_registry)
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Setup error: {e}");
            ExitCode::FAILURE
        }
    }
}
