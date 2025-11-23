use anyhow::Result;
use clap::Parser;
use quocli::{cache, config, executor, llm, parser, shell, tui};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "quocli")]
#[command(about = "AI-powered CLI form generator")]
#[command(version)]
struct Args {
    /// Command to wrap with interactive form
    #[arg(required = true)]
    command: Vec<String>,

    /// Refresh cache for this command
    #[arg(long)]
    refresh_cache: bool,

    /// Clear cached values for this command
    #[arg(long)]
    clear_values: bool,

    /// Execute directly without TUI (use cached/default values)
    #[arg(long)]
    direct: bool,

    /// Show the generated spec without executing
    #[arg(long)]
    show_spec: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "quocli=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let args = Args::parse();

    // Load configuration
    let config = config::load_config()?;

    // Initialize cache
    let cache = cache::Cache::new(&config.cache.path).await?;

    // Get command name and any subcommands
    let command_parts = &args.command;
    if command_parts.is_empty() {
        anyhow::bail!("No command specified");
    }

    let command_name = &command_parts[0];
    let subcommands = &command_parts[1..];

    // Handle cache operations
    if args.clear_values {
        cache.clear_values(command_name).await?;
        println!("Cleared cached values for: {}", command_name);
        return Ok(());
    }

    // Get or generate command spec
    let spec = get_or_generate_spec(
        &cache,
        &config,
        command_name,
        subcommands,
        args.refresh_cache,
    )
    .await?;

    if args.show_spec {
        println!("{}", serde_json::to_string_pretty(&spec)?);
        return Ok(());
    }

    // Load cached values
    let cached_values = cache.get_values(command_name).await?;

    if args.direct {
        // Execute with cached/default values
        let command_line = executor::build_command(&spec, &cached_values);
        let result = executor::execute(&command_line).await?;

        // Export to shell history
        shell::export_to_history(&config.shell, &command_line)?;

        std::process::exit(result.code.unwrap_or(0));
    }

    // Run interactive TUI
    let form_result = tui::run_form(&config, &spec, cached_values).await?;

    if let Some(values) = form_result {
        // Build and execute command
        let command_line = executor::build_command(&spec, &values);

        // Show danger warning for high-risk commands
        if spec.danger_level == parser::DangerLevel::High
            || spec.danger_level == parser::DangerLevel::Critical
        {
            if !tui::confirm_dangerous(&spec, &command_line)? {
                println!("Execution cancelled.");
                return Ok(());
            }
        }

        let result = executor::execute(&command_line).await?;

        // Cache non-sensitive values
        cache
            .save_values(command_name, &values, &spec.options)
            .await?;

        // Export to shell history
        shell::export_to_history(&config.shell, &command_line)?;

        std::process::exit(result.code.unwrap_or(0));
    }

    Ok(())
}

async fn get_or_generate_spec(
    cache: &cache::Cache,
    config: &config::Config,
    command_name: &str,
    subcommands: &[String],
    force_refresh: bool,
) -> Result<parser::CommandSpec> {
    let full_command = if subcommands.is_empty() {
        command_name.to_string()
    } else {
        format!("{}:{}", command_name, subcommands.join(":"))
    };

    // Get help text
    let help_text = parser::get_help_text(command_name, subcommands)?;
    let help_hash = parser::hash_help_text(&help_text);

    // Check cache
    if !force_refresh {
        if let Some(cached_spec) = cache.get_spec(&full_command).await? {
            if cached_spec.version_hash == help_hash {
                tracing::info!("Using cached spec for: {}", full_command);
                cache.update_usage(&full_command).await?;
                return Ok(cached_spec);
            }
            tracing::info!("Help text changed, regenerating spec for: {}", full_command);
        }
    }

    // Generate spec using LLM
    tracing::info!("Generating spec for: {}", full_command);
    let llm_client = llm::create_client(config)?;
    let spec = llm_client
        .generate_spec(command_name, subcommands, &help_text, &help_hash)
        .await?;

    // Cache the spec
    cache.save_spec(&full_command, &spec).await?;

    Ok(spec)
}
