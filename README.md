# quocli

AI-powered CLI form generator that wraps arbitrary commands and generates interactive TUI forms from their help text.

## Features

- **Automatic Help Parsing**: Executes `command --help` and uses AI to understand the command structure
- **Interactive TUI Forms**: Generates intuitive forms with appropriate widgets for each argument type
- **Smart Caching**: Caches command specs using SQLite for instant reuse
- **Sensitive Data Handling**: Never caches or logs sensitive values like passwords and API keys
- **Shell History Integration**: Exports commands to your shell history (bash/zsh/fish)
- **Danger Detection**: Warns about potentially dangerous commands before execution

## Installation

### Using Nix

```bash
nix run github:robert-e-davidson3/quocli -- <command>
```

### From Source

```bash
cargo install --path .
```

## Usage

```bash
# Generate form for curl
quocli curl

# Generate form for git commit
quocli git commit

# Show the generated spec
quocli --show-spec curl

# Refresh cached spec
quocli --refresh-cache curl

# Clear cached values
quocli --clear-values curl

# Execute with cached values (no TUI)
quocli --direct curl
```

## Configuration

Configuration file: `~/.config/quocli/config.toml`

```toml
[llm]
provider = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-sonnet-4-5-20250929"

[cache]
path = "~/.local/share/quocli/cache.db"
ttl_days = 30

[ui]
theme = "dark"
preview_command = true

[shell]
type = "auto"
export_envvars = true

[security]
confirm_dangerous = true
audit_log = true
```

## Environment Variables

- `ANTHROPIC_API_KEY`: Your Anthropic API key (required)

## TUI Controls

- `↑/↓` or `j/k`: Navigate between fields
- `Enter`: Edit field / Toggle boolean / Cycle enum
- `Tab/Shift+Tab`: Next/previous field
- `Ctrl+E`: Execute command
- `Esc` or `q`: Cancel

## How It Works

1. Run `quocli <command>`
2. Fetches help text from `command --help`
3. Hashes help text and checks cache
4. If cache miss: sends help text to Claude API to parse into structured spec
5. Generates interactive form from spec
6. User fills in values using TUI
7. Builds and executes command
8. Caches non-sensitive values for future use
9. Exports command to shell history

## Security

- Sensitive values (passwords, tokens, keys) are never cached
- Dangerous commands show confirmation dialog
- All values cleared from memory after execution
- Commands logged with redacted sensitive values

## License

MIT
