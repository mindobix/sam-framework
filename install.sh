#!/bin/bash
set -e

# SAM Framework Installer
# Installs: sam CLI, monowatch, monograph

BOLD='\033[1m'
GREEN='\033[32m'
CYAN='\033[36m'
DIM='\033[2m'
RESET='\033[0m'

INSTALL_DIR="${SAM_INSTALL_DIR:-/usr/local/bin}"

echo -e "${BOLD}SAM Framework Installer${RESET}"
echo ""

# Check prerequisites
check_dep() {
    if ! command -v "$1" &>/dev/null; then
        echo "  ✗ $1 not found. Install with: $2"
        MISSING=1
    else
        echo "  ✓ $1 $(command -v "$1")"
    fi
}

echo -e "${CYAN}Checking prerequisites...${RESET}"
MISSING=0
check_dep git "brew install git"
check_dep cargo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
check_dep go "brew install go"
check_dep python3 "brew install python@3.12"

if [ "$MISSING" -eq 1 ]; then
    echo ""
    echo "Install missing dependencies and re-run."
    exit 1
fi
echo ""

# Build sam CLI
echo -e "${CYAN}Building sam CLI (Rust)...${RESET}"
cargo build --release --quiet
cp target/release/sam "$INSTALL_DIR/sam"
echo -e "  ${GREEN}✓${RESET} sam installed to $INSTALL_DIR/sam"

# Build monowatch
echo -e "${CYAN}Building monowatch (Go)...${RESET}"
cd monowatch
go build -o "$INSTALL_DIR/monowatch" .
cd ..
echo -e "  ${GREEN}✓${RESET} monowatch installed to $INSTALL_DIR/monowatch"

# Install monograph
echo -e "${CYAN}Installing monograph (Python)...${RESET}"
if command -v uv &>/dev/null; then
    cd monograph && uv sync --quiet && cd ..
    # Create a wrapper script
    cat > "$INSTALL_DIR/monograph" << 'WRAPPER'
#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SAM_DIR="${SAM_FRAMEWORK_DIR:-$(dirname "$SCRIPT_DIR")/vibecode/sam-framework}"
cd "$SAM_DIR/monograph" && uv run monograph "$@"
WRAPPER
    chmod +x "$INSTALL_DIR/monograph"
else
    cd monograph && pip3 install -e . --quiet && cd ..
fi
echo -e "  ${GREEN}✓${RESET} monograph installed"

# Install MonoLens VS Code extension
echo -e "${CYAN}Building MonoLens VS Code extension...${RESET}"
if command -v code &>/dev/null; then
    cd monolens && npm install --silent 2>/dev/null && npm run compile --silent 2>/dev/null && cd ..
    echo -e "  ${GREEN}✓${RESET} monolens compiled (install via: cd monolens && code --install-extension .)"
else
    echo -e "  ${DIM}  skipped (VS Code not found)${RESET}"
fi

echo ""
echo -e "${BOLD}${GREEN}SAM Framework installed!${RESET}"
echo ""
echo -e "  ${BOLD}Quick start:${RESET}"
echo "    sam init git@github.com:your-org/monorepo.git"
echo "    sam use --profile your-team-api"
echo "    sam setup     # Create Finder ghost folders"
echo "    sam watch &   # Auto-hydrate on folder open"
echo ""
echo -e "  ${DIM}Binary locations:${RESET}"
echo "    sam:        $INSTALL_DIR/sam"
echo "    monowatch:  $INSTALL_DIR/monowatch"
echo "    monograph:  $INSTALL_DIR/monograph"
