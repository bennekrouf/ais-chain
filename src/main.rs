mod parser;
mod graph;
mod html;

use clap::Parser;
use colored::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ais-chain", about = "Discover workflow chains in Azure Logic Apps Standard projects")]
struct Cli {
    /// Path to the logic_apps folder
    #[arg(default_value = "logic_apps")]
    path: PathBuf,

    /// Show only the chain containing this workflow
    #[arg(long, short)]
    filter: Option<String>,

    /// Output as Mermaid diagram
    #[arg(long)]
    mermaid: bool,

    /// Generate an interactive HTML graph and open in browser
    #[arg(long)]
    html: bool,

    /// Show the queue map
    #[arg(long)]
    queues: bool,

    /// Manual links for connections invisible in code (e.g. EventGrid subscriptions).
    /// Format: "Source->Target:label" e.g. "Rcv-Event-Pivot->Routing-Pivot-Invoice:EventGrid"
    /// Can be specified multiple times.
    #[arg(long, short = 'l')]
    link: Vec<String>,

    /// Path to a links file (one link per line, same format as --link)
    #[arg(long)]
    links_file: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    if !cli.path.is_dir() {
        eprintln!("{} {} is not a directory", "Error:".red().bold(), cli.path.display());
        std::process::exit(1);
    }

    let workflows = parser::parse_all(&cli.path);
    if workflows.is_empty() {
        eprintln!("{} No workflow.json files found in {}", "Warning:".yellow().bold(), cli.path.display());
        std::process::exit(1);
    }

    // Collect manual links
    let mut manual_links = cli.link.clone();
    if let Some(ref links_path) = cli.links_file {
        if let Ok(content) = std::fs::read_to_string(links_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    manual_links.push(trimmed.to_string());
                }
            }
        }
    }
    // Also auto-detect a .ais-chain file in the logic_apps folder
    let auto_links_path = cli.path.join(".ais-chain");
    if auto_links_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&auto_links_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    manual_links.push(trimmed.to_string());
                }
            }
        }
    }

    let g = graph::build(&workflows, &manual_links);

    if cli.html {
        output_html(&g, &workflows, &cli.filter);
    } else if cli.mermaid {
        output_mermaid(&g, &cli.filter);
    } else {
        output_text(&g, &workflows, &cli.filter, cli.queues);
    }
}

fn output_text(g: &graph::Graph, workflows: &[parser::Workflow], filter: &Option<String>, show_queues: bool) {
    let chains = g.find_chains();

    println!("{}", "=".repeat(70).dimmed());
    println!("{}", "WORKFLOW CHAINS".bold());
    println!("{}", "=".repeat(70).dimmed());

    let mut count = 0;
    for chain in &chains {
        if chain.steps.len() <= 1 {
            continue;
        }

        if let Some(ref f) = filter {
            let f_lower = f.to_lowercase();
            if !chain.steps.iter().any(|s| s.workflow.to_lowercase().contains(&f_lower)) {
                continue;
            }
        }

        count += 1;
        let root = &chain.steps[0];
        let trigger_str = workflows.iter()
            .find(|w| w.name == root.workflow)
            .map(|w| format_triggers(&w.triggers))
            .unwrap_or_else(|| "?".into());

        println!("\n{} Chain {}: {}", "🔗".bold(), count, root.workflow.cyan().bold());
        println!("   Trigger: {}", trigger_str.dimmed());

        for (i, step) in chain.steps.iter().enumerate() {
            if i == 0 {
                println!("     {}", step.workflow.white());
            } else {
                let link = &step.link_type;
                let link_colored = match link.as_str() {
                    l if l.starts_with("queue:") => format!("--[{}]-->", l).yellow(),
                    "EventGrid" => format!("--[{}]-->", link).magenta(),
                    "invoke" => format!("--[{}]-->", link).blue(),
                    _ => format!("--[{}]-->", link).white(),
                };
                println!("     {} {}", link_colored, step.workflow.white());
            }
        }
    }

    // Standalone
    let standalone: Vec<_> = chains.iter()
        .filter(|c| c.steps.len() == 1)
        .filter(|c| {
            if let Some(ref f) = filter {
                c.steps[0].workflow.to_lowercase().contains(&f.to_lowercase())
            } else {
                true
            }
        })
        .collect();

    if !standalone.is_empty() && filter.is_none() {
        println!("\n{}", "=".repeat(70).dimmed());
        println!("{}", "STANDALONE WORKFLOWS".bold());
        println!("{}", "=".repeat(70).dimmed());
        for chain in &standalone {
            let wf_name = &chain.steps[0].workflow;
            let trigger_str = workflows.iter()
                .find(|w| w.name == *wf_name)
                .map(|w| format_triggers(&w.triggers))
                .unwrap_or_else(|| "?".into());
            println!("  {} {}  [{}]", "•".dimmed(), wf_name, trigger_str.dimmed());
        }
    }

    if show_queues {
        println!("\n{}", "=".repeat(70).dimmed());
        println!("{}", "QUEUE MAP".bold());
        println!("{}", "=".repeat(70).dimmed());
        for (queue, producers, consumers) in g.queue_map() {
            println!("  {}", queue.green());
            let prod_str = if producers.is_empty() { "(external)".dimmed().to_string() } else { producers.join(", ") };
            let cons_str = if consumers.is_empty() { "(none)".dimmed().to_string() } else { consumers.join(", ") };
            println!("    {} {}", "←".yellow(), prod_str);
            println!("    {} {}", "→".cyan(), cons_str);
        }
    }

    println!("\n{} {} chains, {} workflows", "Summary:".bold(), count, workflows.len());
}

fn output_mermaid(g: &graph::Graph, filter: &Option<String>) {
    let chains = g.find_chains();

    println!("graph LR");

    for chain in &chains {
        if chain.steps.len() <= 1 {
            continue;
        }

        if let Some(ref f) = filter {
            let f_lower = f.to_lowercase();
            if !chain.steps.iter().any(|s| s.workflow.to_lowercase().contains(&f_lower)) {
                continue;
            }
        }

        for (i, step) in chain.steps.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let prev = &chain.steps[i - 1].workflow;
            let link = &step.link_type;
            let curr = &step.workflow;
            let safe = |s: &str| s.replace('-', "_");
            println!("    {}[\"{}\"] -->|\"{}\"| {}[\"{}\"]", safe(prev), prev, link, safe(curr), curr);
        }
    }
}

fn output_html(g: &graph::Graph, workflows: &[parser::Workflow], filter: &Option<String>) {
    let content = html::generate(g, workflows, filter);
    let path = std::env::temp_dir().join("ais-chain-graph.html");
    std::fs::write(&path, &content).expect("Failed to write HTML file");
    println!("Graph written to {}", path.display());

    // Open in browser
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(&path).spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("cmd").args(["/C", "start", &path.to_string_lossy()]).spawn(); }
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdg-open").arg(&path).spawn(); }
}

fn format_triggers(triggers: &[parser::Link]) -> String {
    if triggers.is_empty() {
        return "?".into();
    }
    triggers.iter()
        .map(|t| format!("{}:{}", t.kind, t.target))
        .collect::<Vec<_>>()
        .join(", ")
}
