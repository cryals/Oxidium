#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use journal_app::AppServices;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

enum CliMode {
    Gui { db_path: Option<PathBuf> },
    McpStdio { db_path: Option<PathBuf> },
    RenderScreenshots { out_dir: PathBuf },
    Help,
}

fn parse_cli() -> CliMode {
    let mut args = std::env::args().skip(1);
    let mut db_path: Option<PathBuf> = None;
    let mut mcp_stdio = false;
    let mut render_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                if let Some(p) = args.next() {
                    db_path = Some(PathBuf::from(p));
                }
            }
            "--mcp-stdio" => mcp_stdio = true,
            "--render-screenshots" => {
                render_dir = args.next().map(PathBuf::from);
            }
            "-h" | "--help" => return CliMode::Help,
            _ => {}
        }
    }

    if let Some(out_dir) = render_dir {
        CliMode::RenderScreenshots { out_dir }
    } else if mcp_stdio {
        CliMode::McpStdio { db_path }
    } else {
        CliMode::Gui { db_path }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,journal_app=info,journal_mcp=info")),
        )
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(false))
        .init();

    match parse_cli() {
        CliMode::Help => {
            eprintln!(
                "Oxidium — Student Journal v{}\n\nUsage:\n  student-journal [--db <PATH>]\n  student-journal --mcp-stdio [--db <PATH>]\n  student-journal --render-screenshots <DIR>",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        CliMode::McpStdio { db_path } => {
            let db = db_path.unwrap_or_else(AppServices::default_db_path);
            let app = AppServices::open_file(&db)?;
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(journal_mcp::run_stdio_server(app))
        }
        CliMode::RenderScreenshots { out_dir } => journal_ui::render_screenshots_headless(&out_dir),
        CliMode::Gui { db_path } => {
            let db = db_path.unwrap_or_else(AppServices::default_db_path);
            let app = AppServices::open_file(&db)?;
            journal_ui::run_desktop_app(app)
        }
    }
}
