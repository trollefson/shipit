use owo_colors::OwoColorize;

/// Print the AI-generated content (PR description, tag notes) with a styled header.
pub fn print_content(header: &str, body: &str) {
    println!("{}\n\n{}", header.bold(), body);
}

/// Print a final resource URL after a successful remote action.
pub fn print_url(url: &str) {
    println!("\n\nAvailable at:\n\n  {}", url.cyan().underline());
}

/// Print a dry-run completion notice. `verb` is the action that was skipped,
/// e.g. "open a request" or "create the tag".
pub fn print_dryrun(verb: &str) {
    println!(
        "\n\n{} Re-run without the dry-run flag to {}.",
        "Dry run complete!".yellow().bold(),
        verb
    );
}

/// Print a success confirmation line with a green checkmark.
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print a dimmed notice for a skipped or no-op action.
pub fn print_skipped(msg: &str) {
    println!("{}", msg.dimmed());
}

/// Print a token scope/permission section label (e.g. "Required token scopes:").
pub fn print_token_scope_label(label: &str) {
    println!("  {}:", label.dimmed());
}

/// Print a single token scope/permission item with the scope name highlighted.
pub fn print_token_scope_item(scope: &str, desc: &str) {
    println!("    - {}  {}", scope.yellow().bold(), desc.dimmed());
}

/// Print an interactive token prompt (no newline) and flush stdout.
pub fn print_token_prompt(label: &str) {
    use std::io::Write;
    print!("  {} ", label.bold());
    std::io::stdout().flush().ok();
}

/// Print the interactive push prompt when the local branch is ahead of remote.
pub fn print_push_prompt(remote: &str, branch: &str) {
    println!(
        "\n\n{}\n\n  git push {} {}\n",
        "Your local source branch is ahead of the remote. Please push it, then press Enter to continue:"
            .yellow(),
        remote,
        branch
    );
}
