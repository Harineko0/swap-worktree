use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use clap::{CommandFactory, Parser, ValueHint};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::CompleteEnv;

#[derive(Debug, Parser)]
#[command(
    name = "swap-worktree",
    version,
    about = "Swap branches (and state) between two Git worktrees.",
    disable_help_subcommand = true
)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long)]
    debug: bool,

    /// Destination worktree directory
    #[arg(value_hint = ValueHint::DirPath, value_name = "DESTINATION_WORKTREE_DIR")]
    destination_worktree_dir: String,

    /// Source branch to take over the destination worktree
    #[arg(
        value_name = "SOURCE_BRANCH_NAME",
        add = ArgValueCompleter::new(branch_value_completer)
    )]
    source_branch_name: String,
}

macro_rules! git_args {
    ($($arg:expr),* $(,)?) => {{
        vec![$(OsString::from($arg)),*]
    }};
}

struct GitOutput {
    stdout: String,
    stderr: String,
    status: ExitStatus,
    command: String,
}

struct StashRecord {
    hash: String,
    reference: Option<String>,
    branch: String,
}

struct Logger {
    debug_enabled: bool,
}

impl Logger {
    fn new(debug_enabled: bool) -> Self {
        Self { debug_enabled }
    }

    fn is_enabled(&self) -> bool {
        self.debug_enabled
    }
}

macro_rules! debug_log {
    ($logger:expr, $($arg:tt)*) => {
        if $logger.is_enabled() {
            println!($($arg)*);
        }
    };
}

fn main() {
    CompleteEnv::with_factory(Cli::command).complete();

    if let Err(err) = run(Cli::parse()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    let dest_arg = cli.destination_worktree_dir;
    let src_branch = cli.source_branch_name;
    let logger = Logger::new(cli.debug);

    let dest_dir = canonicalize_dir(&dest_arg)?;
    ensure_git_worktree(&dest_dir)?;

    let repo_root = determine_repo_root(&dest_dir)?;
    debug_log!(&logger, "Operating in repository: {}", repo_root.display());
    debug_log!(&logger, "---");

    debug_log!(
        &logger,
        "Step 1: Fetching branch for destination directory '{}'...",
        dest_dir.display()
    );
    let dest_branch = current_branch(&dest_dir)?;
    debug_log!(&logger, "Found destination branch: '{dest_branch}'");
    debug_log!(&logger, "---");

    debug_log!(
        &logger,
        "Step 2: Fetching directory for source branch '{src_branch}'..."
    );
    let src_dir = find_worktree_for_branch(&dest_dir, &src_branch)?;
    debug_log!(&logger, "Found source directory: '{}'", src_dir.display());
    debug_log!(&logger, "---");

    let dest_dir_canon = dest_dir.canonicalize()?;
    let src_dir_canon = src_dir.canonicalize()?;
    if dest_dir_canon == src_dir_canon {
        return Err("Source and destination directories are the same. Nothing to swap.".into());
    }

    debug_log!(
        &logger,
        "Step 3: Stashing changes in both worktrees (including untracked files)..."
    );
    let dest_stash = stash_worktree(&dest_dir, &dest_branch, &logger)?;
    let src_stash = stash_worktree(&src_dir, &src_branch, &logger)?;
    debug_log!(&logger, "---");

    debug_log!(&logger, "Step 4: Swapping branches between worktrees...");
    detach_worktree(&dest_dir, &dest_branch, &logger)?;
    if let Err(err) = detach_worktree(&src_dir, &src_branch, &logger) {
        eprintln!("Error: {err}");
        eprintln!(
            "Attempting to restore '{}' to '{}'...",
            dest_dir.display(),
            dest_branch
        );
        let _ = run_git(Some(&dest_dir), git_args!["switch", &dest_branch]);
        return Err("Failed to detach source worktree. Aborting.".into());
    }
    debug_log!(&logger, "Both worktrees detached. Proceeding with swap.");

    switch_worktree(&dest_dir, &src_branch, &logger)?;
    if let Err(err) = switch_worktree(&src_dir, &dest_branch, &logger) {
        return Err(format!(
            "Error: {err}\nCRITICAL STATE: '{}' is on '{src_branch}', but '{}' is still detached.\nPlease manually run:\n  git -C '{}' switch '{src_branch}'\n  git -C '{}' switch '{dest_branch}'",
            dest_dir.display(),
            src_dir.display(),
            dest_dir.display(),
            src_dir.display(),
        ).into());
    }

    debug_log!(&logger, "Branch swap successful.");
    debug_log!(
        &logger,
        "  '{}' is now on branch '{src_branch}'.",
        dest_dir.display()
    );
    debug_log!(
        &logger,
        "  '{}' is now on branch '{dest_branch}'.",
        src_dir.display()
    );
    debug_log!(&logger, "---");

    debug_log!(
        &logger,
        "Step 5: Applying stashes to their new locations..."
    );
    apply_and_drop_stash(&dest_dir, &src_branch, src_stash.as_ref(), &logger);
    apply_and_drop_stash(&src_dir, &dest_branch, dest_stash.as_ref(), &logger);
    debug_log!(&logger, "---");
    debug_log!(&logger, "Worktree swap complete.");
    if !logger.is_enabled() {
        println!(
            "Swap complete: '{}' -> '{src_branch}', '{}' -> '{dest_branch}'.",
            dest_dir.display(),
            src_dir.display()
        );
    }

    Ok(())
}

fn canonicalize_dir(path: impl AsRef<Path>) -> Result<PathBuf, Box<dyn Error>> {
    let dir = path.as_ref();
    if !dir.exists() {
        return Err(format!("Destination directory '{}' does not exist.", dir.display()).into());
    }
    if !dir.is_dir() {
        return Err(format!("'{}' is not a directory.", dir.display()).into());
    }
    Ok(dir.canonicalize()?)
}

fn ensure_git_worktree(dir: &Path) -> Result<(), Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["rev-parse", "--is-inside-work-tree"],
        "Failed to determine whether destination is a git worktree.",
    )?;
    if output.stdout.trim() != "true" {
        return Err(format!("'{}' is not inside a git worktree.", dir.display()).into());
    }
    Ok(())
}

fn determine_repo_root(dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["rev-parse", "--git-common-dir"],
        "Failed to determine repository root.",
    )?;
    let git_dir = PathBuf::from(output.stdout.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        dir.join(git_dir)
    };
    let repo_root = git_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dir.to_path_buf());
    Ok(repo_root)
}

fn current_branch(dir: &Path) -> Result<String, Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["symbolic-ref", "--short", "HEAD"],
        "Failed to determine destination branch.",
    )?;
    let branch = output.stdout.trim();
    if branch.is_empty() {
        return Err(format!("Could not determine branch for '{}'.", dir.display()).into());
    }
    Ok(branch.to_string())
}

fn find_worktree_for_branch(dir: &Path, branch: &str) -> Result<PathBuf, Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["worktree", "list", "--porcelain"],
        "Failed to list worktrees.",
    )?;
    let mut worktree_path: Option<String> = None;
    let mut branch_name: Option<String> = None;
    for line in output.stdout.lines() {
        if line.trim().is_empty() {
            if branch_name
                .as_deref()
                .map(|name| name == branch)
                .unwrap_or(false)
            {
                if let Some(path) = worktree_path {
                    let path_buf = normalize_path(dir, &path);
                    if !path_buf.exists() {
                        return Err(format!(
                            "Source directory '{}' (for branch '{branch}') does not exist.",
                            path_buf.display()
                        )
                        .into());
                    }
                    return Ok(path_buf);
                }
            }
            worktree_path = None;
            branch_name = None;
            continue;
        }

        if let Some(rest) = line.strip_prefix("worktree ") {
            worktree_path = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let trimmed = rest.trim();
            branch_name = Some(
                trimmed
                    .strip_prefix("refs/heads/")
                    .unwrap_or(trimmed)
                    .to_string(),
            );
        }
    }

    if branch_name
        .as_deref()
        .map(|name| name == branch)
        .unwrap_or(false)
    {
        if let Some(path) = worktree_path {
            let path_buf = normalize_path(dir, &path);
            if !path_buf.exists() {
                return Err(format!(
                    "Source directory '{}' (for branch '{branch}') does not exist.",
                    path_buf.display()
                )
                .into());
            }
            return Ok(path_buf);
        }
    }

    Err(format!("Could not find worktree for branch '{branch}'.").into())
}

fn list_worktree_branches(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["worktree", "list", "--porcelain"],
        "Failed to list worktrees.",
    )?;
    Ok(parse_worktree_branches(&output.stdout))
}

fn parse_worktree_branches(porcelain: &str) -> Vec<String> {
    let mut branches = BTreeSet::new();
    for line in porcelain.lines() {
        let Some(rest) = line.strip_prefix("branch ") else {
            continue;
        };
        let trimmed = rest.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.strip_prefix("refs/heads/").unwrap_or(trimmed);
        branches.insert(normalized.to_string());
    }
    branches.into_iter().collect()
}

fn normalize_path(base: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn stash_worktree(
    dir: &Path,
    branch: &str,
    logger: &Logger,
) -> Result<Option<StashRecord>, Box<dyn Error>> {
    debug_log!(logger, "Stashing '{}' (Branch: {branch})...", dir.display());
    let message = format!("swap-stash-{branch}");
    let output = run_git(Some(dir), git_args!["stash", "push", "-u", "-m", &message])?;
    let combined = combined_output(&output);
    if combined.trim() == "No local changes to save" {
        debug_log!(logger, "No changes to stash in '{}'.", dir.display());
        return Ok(None);
    }
    if !output.status.success() {
        return Err(format!(
            "Failed to create stash in '{}': {}",
            dir.display(),
            combined
        )
        .into());
    }

    let rev = run_git_success(
        Some(dir),
        git_args!["rev-parse", "stash@{0}"],
        "Failed to determine stash SHA.",
    )?;
    let hash = rev.stdout.trim().to_string();
    debug_log!(
        logger,
        "Stashed changes from '{}' as {hash}.",
        dir.display()
    );
    let reference = find_stash_reference(dir, &hash)?;
    Ok(Some(StashRecord {
        hash,
        reference,
        branch: branch.to_string(),
    }))
}

fn find_stash_reference(dir: &Path, hash: &str) -> Result<Option<String>, Box<dyn Error>> {
    let output = run_git_success(
        Some(dir),
        git_args!["stash", "list", "--format=%H:%gd"],
        "Failed to list stashes.",
    )?;
    for line in output.stdout.lines() {
        if let Some((commit, reference)) = line.split_once(':') {
            if commit == hash {
                return Ok(Some(reference.trim().to_string()));
            }
        }
    }
    Ok(None)
}

fn detach_worktree(dir: &Path, branch: &str, logger: &Logger) -> Result<(), Box<dyn Error>> {
    debug_log!(
        logger,
        "Detaching HEAD in '{}' (freeing {branch})...",
        dir.display()
    );
    run_git_success(
        Some(dir),
        git_args!["switch", "--detach"],
        "Failed to detach worktree.",
    )?;
    Ok(())
}

fn switch_worktree(dir: &Path, branch: &str, logger: &Logger) -> Result<(), Box<dyn Error>> {
    debug_log!(logger, "Switching '{}' -> to '{branch}'...", dir.display());
    run_git_success(
        Some(dir),
        git_args!["switch", branch],
        "Failed to switch worktree branch.",
    )?;
    Ok(())
}

fn apply_and_drop_stash(dir: &Path, branch: &str, stash: Option<&StashRecord>, logger: &Logger) {
    if let Some(stash) = stash {
        debug_log!(
            logger,
            "Applying stash {} (from {}) to '{}'...",
            stash.hash,
            stash.branch,
            dir.display()
        );
        let result = run_git(Some(dir), git_args!["stash", "apply", &stash.hash]);
        match result {
            Ok(output) if output.status.success() => {
                debug_log!(logger, "Successfully applied stash.");
                if let Some(reference) = &stash.reference {
                    if let Err(err) = drop_stash(dir, reference, logger) {
                        eprintln!("Warning: Failed to drop applied stash {reference}: {err}");
                    }
                } else {
                    eprintln!(
                        "Warning: Could not determine stash reference for {}. The stash remains in the list.",
                        stash.hash
                    );
                }
            }
            Ok(output) => {
                eprintln!(
                    "Warning: Failed to apply stash {} to '{}'.\nOutput: {}",
                    stash.hash,
                    dir.display(),
                    combined_output(&output)
                );
                eprintln!(
                    "The stash has been kept. Please resolve manually in '{}'.",
                    dir.display()
                );
            }
            Err(err) => {
                eprintln!(
                    "Warning: Failed to apply stash {} to '{}': {err}",
                    stash.hash,
                    dir.display()
                );
                eprintln!(
                    "The stash has been kept. Please resolve manually in '{}'.",
                    dir.display()
                );
            }
        }
    } else {
        debug_log!(
            logger,
            "No stash from '{branch}' to apply to '{}'.",
            dir.display()
        );
    }
}

fn drop_stash(dir: &Path, reference: &str, logger: &Logger) -> Result<(), Box<dyn Error>> {
    let output = run_git(Some(dir), git_args!["stash", "drop", reference])?;
    if output.status.success() {
        debug_log!(logger, "Dropped stash {reference}.");
        Ok(())
    } else {
        Err(format!(
            "git stash drop {reference} failed: {}",
            combined_output(&output)
        )
        .into())
    }
}

fn combined_output(output: &GitOutput) -> String {
    let mut combined = String::new();
    if !output.stdout.trim().is_empty() {
        write!(&mut combined, "{}", output.stdout.trim()).ok();
    }
    if !output.stderr.trim().is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        write!(&mut combined, "{}", output.stderr.trim()).ok();
    }
    combined
}

fn run_git(dir: Option<&Path>, args: Vec<OsString>) -> Result<GitOutput, Box<dyn Error>> {
    let command = describe_args(&args);
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    let output = cmd.args(&args).output()?;
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status,
        command,
    })
}

fn run_git_success(
    dir: Option<&Path>,
    args: Vec<OsString>,
    context: &str,
) -> Result<GitOutput, Box<dyn Error>> {
    let output = run_git(dir, args)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{context}\nCommand: git {}\nstdout: {}\nstderr: {}",
            output.command,
            output.stdout.trim(),
            output.stderr.trim()
        )
        .into())
    }
}

fn describe_args(args: &[OsString]) -> String {
    let mut rendered = String::new();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            rendered.push(' ');
        }
        rendered.push_str(&arg.to_string_lossy());
    }
    rendered
}

fn branch_value_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut results = Vec::new();
    let dest_dir = match completion_destination_dir() {
        Some(dir) => dir,
        None => return results,
    };
    let prefix = current.to_string_lossy();
    if let Ok(branches) = list_worktree_branches(&dest_dir) {
        results.extend(
            branches
                .into_iter()
                .filter(|name| name.starts_with(prefix.as_ref()))
                .map(CompletionCandidate::new),
        );
    }
    results
}

fn completion_destination_dir() -> Option<PathBuf> {
    let words = completion_words()?;
    let dest = completion_destination(&words)?;
    let dest_path = PathBuf::from(dest);
    canonicalize_dir(&dest_path).ok()
}

fn completion_words() -> Option<Vec<OsString>> {
    if env::var("_CLAP_COMPLETE_INDEX").is_err() {
        return None;
    }
    let args: Vec<OsString> = env::args_os().collect();
    let marker = args.iter().position(|arg| arg == "--")?;
    Some(args[(marker + 1)..].to_vec())
}

fn completion_destination(words: &[OsString]) -> Option<OsString> {
    let mut iter = words.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--" {
            return iter.next().cloned();
        }
        match arg.to_str() {
            Some("-d") | Some("--debug") => continue,
            _ => return Some(arg.clone()),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_worktree_branches;

    #[test]
    fn parses_branches_from_porcelain() {
        let fixture = r#"worktree /repos/main
HEAD e1e1b70d2e8c133c96ab8050cc582f88aa83ef77
branch refs/heads/main

worktree /repos/feature-a
HEAD 1c1cdd9c68b3bd55a72efa87c67fd03c4b5aa20c
branch refs/heads/feature/a

worktree /repos/detached
HEAD 9a9a71114237d6a1f2ba4d0332eec2a3edf1b738

"#;
        let branches = parse_worktree_branches(fixture);
        assert_eq!(branches, vec!["feature/a".to_string(), "main".to_string()]);
    }

    #[test]
    fn dedupes_and_sorts_branch_names() {
        let fixture = r#"branch refs/heads/main
branch refs/heads/main
branch feature/b
branch   
"#;
        let branches = parse_worktree_branches(fixture);
        assert_eq!(branches, vec!["feature/b".to_string(), "main".to_string()]);
    }
}
