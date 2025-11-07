## swap-worktree

`swap-worktree` is a Rust CLI that swaps the branches (and all local worktree state, including staged and untracked files) between two Git worktrees. It mirrors and extends the original `swap.sh` script with improved error handling, logging, and cross-platform binaries.

### Requirements

- Git 2.37+ with worktree support enabled.
- Rust toolchain (`rustup` + `cargo`) if you plan to build from source.
- macOS, Linux, or Windows.

### Installation

#### 1. Install from source

```bash
cargo install --path .
# or, from your own clone/fork:
cargo install --git https://github.com/<you>/swap-worktree.git
```

#### 2. Use prebuilt binaries

Pushes to `main` publish release artifacts for Linux, macOS, and Windows. To install:

1. Download the archive that matches your OS from the latest GitHub Release.
2. Extract it locally; the archive contains a single executable named `swap-worktree` (or `swap-worktree.exe` on Windows).
3. Move the executable to a directory on your `PATH`.

##### macOS & Linux

```bash
tar -xzf swap-worktree-macos-universal.tar.gz   # or the linux archive
sudo install -m 755 swap-worktree /usr/local/bin/
```

You can choose any directory that is already on your `PATH` (e.g., `/usr/local/bin`, `$HOME/.cargo/bin`, `$HOME/bin`). Verify with `which swap-worktree`.

##### Windows

```powershell
Expand-Archive -Path swap-worktree-windows-x86_64.zip -DestinationPath C:\tools\swap-worktree
```

Add the folder containing `swap-worktree.exe` to the `PATH` environment variable (Control Panel → System → Advanced system settings → Environment Variables → Edit the `Path` entry → Add the folder). Alternatively, from PowerShell:

```powershell
setx PATH "$($Env:PATH);C:\tools\swap-worktree"
```

Move any existing PowerShell windows to pick up the new `PATH`, or start a fresh terminal and run `swap-worktree --help` to verify installation. You can also keep the binary alongside your repositories and invoke it with an explicit path if you prefer not to modify `PATH`.

### Usage

```bash
swap-worktree <destination_worktree_dir> <source_branch_name>
```

Examples:

```bash
swap-worktree /path/to/worktrees/feature-a feature/b
swap-worktree ../myrepo-worktrees/review-wt main
```

The tool performs the following steps with detailed logging:

1. Validates the destination worktree directory and detects its branch.
2. Locates the worktree hosting the source branch.
3. Stashes both worktrees (including untracked files) when changes exist.
4. Detaches both worktrees, swaps their branches, and reapplies/drops the captured stashes.

If a stash fails to apply, the CLI keeps it and prints actionable guidance so you can resolve conflicts manually.

### Development workflow

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release
```

GitHub Actions automatically runs the same commands on pull requests and pushes to `main`. A separate workflow builds release artifacts for macOS, Linux, and Windows whenever `main` is updated.

### Contributing

1. Fork the repository and create a new branch.
2. Make your changes, keeping commits focused.
3. Run the commands listed in the Development workflow section to ensure formatting, linting, tests, and release builds succeed locally.
4. Open a pull request—CI will validate your changes automatically.

Bug reports and feature ideas are welcome! Please include reproduction steps and relevant context to help triage the request quickly.
