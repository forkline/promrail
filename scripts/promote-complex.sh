#!/bin/bash
# promote-complex.sh - Multi-source promotion with promrail
# Usage: ./scripts/promote-complex.sh [OPTIONS]
#
# Options:
#   --dry-run        Preview changes without applying
#   --sources FILE   JSON file with source paths (one per line)
#   --dest PATH      Destination repository path
#
# Environment:
#   PROMRAIL_CONFIG  Path to promrail.yaml
#   DEST_PATH        Destination path (default from config or --dest)
#
# Example:
#   ./scripts/promote-complex.sh --dry-run

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
DRY_RUN=""
DEST_PATH="${DEST_PATH:-}"
SOURCES_FILE=""
EXPLAIN=1

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN="--dry-run"
            shift
            ;;
        --sources)
            SOURCES_FILE="$2"
            shift 2
            ;;
        --dest)
            DEST_PATH="$2"
            shift 2
            ;;
        --no-explain)
            EXPLAIN=""
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Multi-source promotion with promrail"
            echo ""
            echo "Options:"
            echo "  --dry-run        Preview changes without applying"
            echo "  --sources FILE   File with source paths (one per line)"
            echo "  --dest PATH      Destination repository path"
            echo "  --no-explain     Don't show merge explanation"
            echo "  -h, --help       Show this help"
            echo ""
            echo "Environment:"
            echo "  PROMRAIL_CONFIG  Path to promrail.yaml"
            echo "  DEST_PATH        Destination path (default from config or --dest)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo -e "${CYAN}=== Multi-Source Promotion ===${NC}"
echo ""

# Check promrail is available
if ! command -v promrail &> /dev/null; then
    echo -e "${RED}ERROR: promrail not found in PATH${NC}"
    echo "Install with: cargo install --path ."
    exit 1
fi

# Find config
if [ -z "$PROMRAIL_CONFIG" ]; then
    for candidate in promrail.yaml promrail.yml .promrail.yaml .promrail.yml; do
        if [ -f "$candidate" ]; then
            PROMRAIL_CONFIG="$candidate"
            break
        fi
    done
fi

if [ -z "$PROMRAIL_CONFIG" ]; then
    echo -e "${RED}ERROR: No promrail.yaml found${NC}"
    echo "Set PROMRAIL_CONFIG or create promrail.yaml"
    exit 1
fi

echo -e "Config: ${CYAN}$PROMRAIL_CONFIG${NC}"

# Get destination path from config if not set
if [ -z "$DEST_PATH" ]; then
    # Try to get first repo's path from config
    DEST_PATH=$(promrail config show 2>/dev/null | grep -A1 "dest_path" | tail -1 | awk '{print $2}' || echo "")
    if [ -z "$DEST_PATH" ]; then
        echo -e "${RED}ERROR: No destination path specified${NC}"
        echo "Set DEST_PATH environment variable or use --dest"
        exit 1
    fi
fi

echo -e "Destination: ${CYAN}$DEST_PATH${NC}"
echo ""

# Get sources from rules or file
SOURCES=()
if [ -n "$SOURCES_FILE" ] && [ -f "$SOURCES_FILE" ]; then
    # Read sources from file
    while IFS= read -r line; do
        [ -n "$line" ] && SOURCES+=("$line")
    done < "$SOURCES_FILE"
else
    # Extract source names from rules in config
    # This is a simplified extraction - adjust based on your config structure
    echo -e "${YELLOW}No sources file provided, extracting from rules...${NC}"

    # For now, require explicit sources
    echo -e "${RED}ERROR: No sources specified${NC}"
    echo "Create a sources file with one path per line:"
    echo "  ~/gitops/staging-homelab"
    echo "  ~/gitops/staging-work"
    echo ""
    echo "Then run:"
    echo "  $0 --sources sources.txt"
    exit 1
fi

if [ ${#SOURCES[@]} -lt 2 ]; then
    echo -e "${RED}ERROR: Need at least 2 sources for merge${NC}"
    exit 1
fi

echo -e "Sources:"
for src in "${SOURCES[@]}"; do
    echo -e "  - ${CYAN}$src${NC}"
done
echo ""

# Step 1: Extract versions from all sources
echo -e "${GREEN}Step 1: Extracting versions from sources...${NC}"
SOURCE_ARGS=()
TEMP_FILES=()

for src in "${SOURCES[@]}"; do
    # Expand ~ in path
    expanded_path="${src/#\~/$HOME}"

    if [ ! -d "$expanded_path" ]; then
        echo -e "${RED}ERROR: Source path not found: $src${NC}"
        exit 1
    fi

    temp_file=$(mktemp)
    echo -e "  Extracting from ${CYAN}$src${NC}..."

    promrail versions extract --path "$expanded_path" -o "$temp_file" 2>/dev/null

    SOURCE_ARGS+=("--source" "$expanded_path")
    TEMP_FILES+=("$temp_file")
done

echo ""

# Step 2: Merge versions
echo -e "${GREEN}Step 2: Merging versions...${NC}"

MERGE_OUTPUT=$(mktemp)
MERGE_EXPLAIN=$(mktemp)

if [ -n "$EXPLAIN" ]; then
    promrail versions merge "${SOURCE_ARGS[@]}" --explain > "$MERGE_EXPLAIN" 2>&1
    cat "$MERGE_EXPLAIN"
    echo ""
fi

promrail versions merge "${SOURCE_ARGS[@]}" -o "$MERGE_OUTPUT" 2>/dev/null

echo -e "  Merged versions written to temp file"
echo ""

# Step 3: Apply changes
echo -e "${GREEN}Step 3: Applying changes...${NC}"

if [ -n "$DRY_RUN" ]; then
    echo -e "  ${YELLOW}Dry run mode - not applying changes${NC}"
    promrail versions apply -f "$MERGE_OUTPUT" --path "$DEST_EXPANDED" --dry-run
else
    promrail versions apply \
        -f "$MERGE_OUTPUT" \
        --path "$DEST_EXPANDED" \
        --check-conflicts \
        --snapshot
fi

echo ""

# Step 5: Summary
echo -e "${GREEN}=== Promotion Complete ===${NC}"
echo ""

if [ -n "$DRY_RUN" ]; then
    echo -e "  ${YELLOW}This was a dry run. Remove --dry-run to apply changes.${NC}"
else
    echo -e "  ${GREEN}Changes applied to: $DEST_PATH${NC}"
    echo ""
    echo -e "  Review changes:"
    echo -e "    ${CYAN}git diff${NC}"
    echo ""
    echo -e "  Rollback if needed:"
    echo -e "    ${CYAN}promrail snapshot list --path $DEST_PATH${NC}"
    echo -e "    ${CYAN}promrail snapshot rollback <id> --path $DEST_PATH${NC}"
fi

# Cleanup temp files
for f in "${TEMP_FILES[@]}"; do
    rm -f "$f"
done
rm -f "$MERGE_OUTPUT" "$MERGE_EXPLAIN" "$DIFF_OUTPUT" 2>/dev/null || true
