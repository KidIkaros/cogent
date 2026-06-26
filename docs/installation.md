# Installation

Get Cogent up and running in under a minute.

---

## Quick Install

Choose your platform:

```bash
# macOS (Recommended)
brew install cogent

# Linux
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/

# Windows
# Download from https://github.com/KidIkaros/cogent/releases/latest
```

Verify:

```bash
cogent --version
# Output: cogent 1.2.0
```

---

## Platform-Specific Instructions

### macOS (Homebrew)

**Recommended for most Mac users.**

```bash
brew tap kidikaros/cogent
brew install cogent
```

**To upgrade later:**

```bash
brew upgrade cogent
```

**To uninstall:**

```bash
brew uninstall cogent
brew untap kidikaros/cogent
```

---

### Linux (Binary Download)

**Recommended for Linux servers and CI/CD.**

```bash
# Download
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz

# Extract
tar xzf cogent-linux-x86_64.tar.gz

# Install to PATH
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/

# Verify
cogent --version
```

**To upgrade later:**

```bash
curl -LO https://github.com/KidIkaros/cogent/releases/latest/download/cogent-linux-x86_64.tar.gz
tar xzf cogent-linux-x86_64.tar.gz
sudo cp cogent-linux-x86_64/cogent /usr/local/bin/
```

**To uninstall:**

```bash
sudo rm /usr/local/bin/cogent
```

---

### Windows

**Recommended for Windows developers.**

1. Download the latest `.zip` from https://github.com/KidIkaros/cogent/releases/latest
2. Extract to `C:\Program Files\cogent\`
3. Add `C:\Program Files\cogent\` to your PATH
4. Open a new Command Prompt and verify:

```cmd
cogent --version
```

---

### From Source (Cargo)

**Recommended for contributors or custom builds.**

```bash
# Clone the repo
git clone https://github.com/KidIkaros/cogent.git
cd cogent

# Build (optimized release)
cargo build --release --workspace

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Verify
cogent --version
```

**To upgrade later:**

```bash
git pull
cargo build --release --workspace
```

---

### Docker

**Recommended for CI/CD or air-gapped environments.**

```bash
# Pull the image
docker pull ghcr.io/kidikaros/cogent:latest

# Run against a mounted directory
docker run --rm -v /path/to/project:/project ghcr.io/kidikaros/cogent:latest check /project

# Run with custom thresholds
docker run --rm -v /path/to/project:/project -v /path/to/config:/config ghcr.io/kidikaros/cogent:latest check /project --config /config/.quality.toml
```

---

## Shell Completions

Cogent generates shell completions for bash, zsh, fish, PowerShell, and Elvish.

### Bash

```bash
# System-wide
cogent completions bash > /etc/bash_completion.d/cogent

# User-only
cogent completions bash > ~/.local/share/bash-completion/completions/cogent
```

### Zsh

```bash
cogent completions zsh > /usr/local/share/zsh/site-functions/_cogent
```

### Fish

```bash
cogent completions fish > ~/.config/fish/completions/cogent.fish
```

### PowerShell

```powershell
cogent completions powershell | Out-String | Invoke-Expression
```

### Elvish

```bash
cogent completions elvish > ~/.elvish/lib/cogent.elv
```

---

## Verifying Installation

Run a quick test:

```bash
# Initialize a demo config
mkdir ~/cogent-test && cd ~/cogent-test
echo 'fn main() { println!("Hello"); }' > main.rs

# Run Cogent
cogent check .
```

You should see output like:

```
  ✓ detected: Rust  (Cargo.toml not found, inferring)
  ✓ wrote .quality.toml  (0.5ms)

  ╔══════════════════════════════════════════════════════╗
  ║  COGENT CHECK  ·  PASSED ✓                          ║
  ╠══════════════════════════════════════════════════════╣
  ║  31/31 checks passed  ·  0.3s total                  ║
  ║  Score: 100/100  A                                   ║
  ║  Path: .                                             ║
  ╚══════════════════════════════════════════════════════╝
```

---

## Troubleshooting

### Permission denied (Linux)

**Error:** `bash: /usr/local/bin/cogent: Permission denied`

**Fix:**
```bash
sudo chmod +x /usr/local/bin/cogent
```

### Command not found

**Error:** `cogent: command not found`

**Fix:** Ensure `/usr/local/bin` is in your PATH:
```bash
export PATH="/usr/local/bin:$PATH"
echo 'export PATH="/usr/local/bin:$PATH"' >> ~/.bashrc
```

### Slow startup

**Cause:** First run builds a cache. Subsequent runs are fast.

**Fix:** Nothing to do. Cache is automatically built on first run.

### Outdated version

**Check:**
```bash
cogent --version
```

**Fix:** Re-install using your platform's upgrade command (see above).

---

## Next Steps

1. **Run your first audit:** See [Quickstart](./quickstart.md)
2. **Wire up CI/CD:** See [CI/CD Integration](./cicd.md)
3. **Customize thresholds:** See [Configuration](./configuration.md)
4. **Explore tools:** See [Tool Reference](./tools/)

---

## Get Help

- **Documentation:** https://kidikaros.github.io/cogent/
- **GitHub Issues:** https://github.com/KidIkaros/cogent/issues
- **Discord:** [Join our community](https://discord.gg/cogent)

---

**Installed? Great! Now run `cogent check .` to audit your code.** 🚀