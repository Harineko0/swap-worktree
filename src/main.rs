use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

macro_rules! git_args {
    ($($arg:expr),* $(,)?) => {{
        let mut args = Vec::<OsString>::new();
        $(args.push(OsString::from($arg));)*
        args
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

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut argv = env::args();
    let program = argv.next().unwrap_or_else(|| "swap-worktree".to_string());
    let dest_arg = match argv.next() {
        Some(value) => value,
        None => return Err(usage_error(&program)),
    };
    let src_branch = match argv.next() {
        Some(value) => value,
        None => return Err(usage_error(&program)),
    };
    if argv.next().is_some() {
        return Err(usage_error(&program));
    }

    let dest_dir = canonicalize_dir(&dest_arg)?;
    ensure_git_worktree(&dest_dir)?;

    let repo_root = determine_repo_root(&dest_dir)?;
    println!("Operating in repository: {}", repo_root.display());
    println!("---");

    println!(
        "Step 1: Fetching branch for destination directory '{}'...",
        dest_dir.display()
    );
    let dest_branch = current_branch(&dest_dir)?;
    println!("Found destination branch: '{dest_branch}'");
    println!("---");

    println!("Step 2: Fetching directory for source branch '{src_branch}'...");
    let src_dir = find_worktree_for_branch(&dest_dir, &src_branch)?;
    println!("Found source directory: '{}'", src_dir.display());
    println!("---");

    let dest_dir_canon = dest_dir.canonicalize()?;
    let src_dir_canon = src_dir.canonicalize()?;
    if dest_dir_canon == src_dir_canon {
        return Err("Source and destination directories are the same. Nothing to swap.".into());
    }

    println!("Step 3: Stashing changes in both worktrees (including untracked files)...");
    let dest_stash = stash_worktree(&dest_dir, &dest_branch)?;
    let src_stash = stash_worktree(&src_dir, &src_branch)?;
    println!("---");

    println!("Step 4: Swapping branches between worktrees...");
    detach_worktree(&dest_dir, &dest_branch)?;
    if let Err(err) = detach_worktree(&src_dir, &src_branch) {
        eprintln!("Error: {err}");
        println!(
            "Attempting to restore '{}' to '{}'...",
            dest_dir.display(),
            dest_branch
        );
        let _ = run_git(Some(&dest_dir), git_args!["switch", &dest_branch]);
        return Err("Failed to detach source worktree. Aborting.".into());
    }
    println!("Both worktrees detached. Proceeding with swap.");

    switch_worktree(&dest_dir, &src_branch)?;
    if let Err(err) = switch_worktree(&src_dir, &dest_branch) {
        return Err(format!(
            "Error: {err}\nCRITICAL STATE: '{}' is on '{src_branch}', but '{}' is still detached.\nPlease manually run:\n  git -C '{}' switch '{src_branch}'\n  git -C '{}' switch '{dest_branch}'",
            dest_dir.display(),
            src_dir.display(),
            dest_dir.display(),
            src_dir.display(),
        ).into());
    }

    println!("Branch swap successful.");
    println!(
        "  '{}' is now on branch '{src_branch}'.",
        dest_dir.display()
    );
    println!(
        "  '{}' is now on branch '{dest_branch}'.",
        src_dir.display()
    );
    println!("---");

    println!("Step 5: Applying stashes to their new locations...");
    apply_and_drop_stash(&dest_dir, &src_branch, src_stash.as_ref());
    apply_and_drop_stash(&src_dir, &dest_branch, dest_stash.as_ref());
    println!("---");
    println!("Worktree swap complete.");

    Ok(())
}

fn usage_error(program: &str) -> Box<dyn Error> {
    format!("Usage: {program} <destination_worktree_dir> <source_branch_name>").into()
}

fn canonicalize_dir(path: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(path);
    if !dir.exists() {
        return Err(format!("Destination directory '{path}' does not exist.").into());
    }
    if !dir.is_dir() {
        return Err(format!("'{path}' is not a directory.").into());
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

fn normalize_path(base: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn stash_worktree(dir: &Path, branch: &str) -> Result<Option<StashRecord>, Box<dyn Error>> {
    println!("Stashing '{}' (Branch: {branch})...", dir.display());
    let message = format!("swap-stash-{branch}");
    let output = run_git(Some(dir), git_args!["stash", "push", "-u", "-m", &message])?;
    let combined = combined_output(&output);
    if combined.trim() == "No local changes to save" {
        println!("No changes to stash in '{}'.", dir.display());
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
    println!("Stashed changes from '{}' as {hash}.", dir.display());
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

fn detach_worktree(dir: &Path, branch: &str) -> Result<(), Box<dyn Error>> {
    println!(
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

fn switch_worktree(dir: &Path, branch: &str) -> Result<(), Box<dyn Error>> {
    println!("Switching '{}' -> to '{branch}'...", dir.display());
    run_git_success(
        Some(dir),
        git_args!["switch", branch],
        "Failed to switch worktree branch.",
    )?;
    Ok(())
}

fn apply_and_drop_stash(dir: &Path, branch: &str, stash: Option<&StashRecord>) {
    if let Some(stash) = stash {
        println!(
            "Applying stash {} (from {}) to '{}'...",
            stash.hash,
            stash.branch,
            dir.display()
        );
        let result = run_git(Some(dir), git_args!["stash", "apply", &stash.hash]);
        match result {
            Ok(output) if output.status.success() => {
                println!("Successfully applied stash.");
                if let Some(reference) = &stash.reference {
                    if let Err(err) = drop_stash(dir, reference) {
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
        println!("No stash from '{branch}' to apply to '{}'.", dir.display());
    }
}

fn drop_stash(dir: &Path, reference: &str) -> Result<(), Box<dyn Error>> {
    let output = run_git(Some(dir), git_args!["stash", "drop", reference])?;
    if output.status.success() {
        println!("Dropped stash {reference}.");
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
            combined.push_str("\n");
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
