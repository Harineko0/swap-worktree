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

Pushes to `main` publish release artifacts for Linux, macOS, and Windows. Download the latest archive from the GitHub Releases page and place the binary on your `PATH`.

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
