use std::{env, io};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let cwd = env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let exe = env::current_exe().unwrap_or_else(|_| "acc".into());
    let exe_str = exe.to_string_lossy().to_string();

    match args.get(1).map(|s| s.as_str()) {
        Some("list" | "ls") => {
            acc::cli::list_agents();
            return Ok(());
        }
        Some("read") => {
            acc::cli::read_agent(&args[2..]);
            return Ok(());
        }
        Some("send") => {
            acc::cli::send_to_agent(&args[2..]);
            return Ok(());
        }
        Some("root") => {
            acc::cli::start_root();
            return Ok(());
        }
        Some("sidebar-toggle") => {
            // Called from tmux keybinding — toggle sidebar
            if acc::tmux::inside_tmux() {
                if acc::tmux::sidebar_pane_id().is_some() {
                    acc::tmux::kill_sidebar();
                } else {
                    let cmd = format!("{} sidebar '{}'", exe_str, cwd);
                    acc::tmux::create_sidebar(&cmd);
                }
            }
            return Ok(());
        }
        Some("kill" | "stop") => {
            if acc::tmux::inside_tmux() { acc::tmux::kill_sidebar(); }
            println!("Stopped.");
            return Ok(());
        }
        Some("help" | "-h" | "--help") => {
            println!("ACC — Agent Command Center by FeynixAI\n");
            println!("  acc           Launch (or toggle sidebar)");
            println!("  acc kill      Close sidebar");
            println!("  acc list      List all agents");
            println!("  acc read      Read agent output");
            println!("  acc send      Send prompt to agent");
            println!("  acc root      Start root commander\n");
            println!("Keys: 1-9=jump [/]=win n/p=pane :=cmd /=find >=send q=quit");
            println!("\nhttps://github.com/feynixai/acc");
            return Ok(());
        }
        Some("sidebar") => {
            let workspace = args.get(2).cloned().unwrap_or(cwd);
            return acc::event::run_sidebar(&workspace);
        }
        _ => {}
    }

    if acc::tmux::inside_tmux() {
        if acc::tmux::sidebar_pane_id().is_some() {
            acc::tmux::kill_sidebar();
        } else {
            let cmd = format!("{} sidebar '{}'", exe_str, cwd);
            if acc::tmux::create_sidebar(&cmd).is_none() {
                eprintln!("Failed to create sidebar.");
            }
        }
    } else {
        if !acc::tmux::is_installed() {
            eprintln!("tmux not installed. brew install tmux");
            return Ok(());
        }
        let session = acc::tmux::session_name_for(&cwd);
        if acc::tmux::session_exists(&session) {
            acc::tmux::attach(&session);
        } else {
            acc::tmux::create_and_attach(&session, &cwd, &exe_str);
        }
    }
    Ok(())
}
