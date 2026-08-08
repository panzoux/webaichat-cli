import sqlite3
import json
from datetime import datetime, timedelta

DB_PATH = r"C:\Users\user\.local\share\mimocode\mimocode.db"
conn = sqlite3.connect(DB_PATH)
cursor = conn.cursor()

thirty_days_ago = datetime.now() - timedelta(days=30)
cutoff_ms = int(thirty_days_ago.timestamp() * 1000)

print("=== FINAL WORKFLOW ANALYSIS ===\n")

# Get all assistant bash commands
cursor.execute("""
    SELECT json_extract(p.data, '$.state.input') as input_data
    FROM message m
    JOIN part p ON p.message_id = m.id
    WHERE json_extract(m.data, '$.role') = 'assistant'
      AND json_extract(p.data, '$.type') = 'tool'
      AND json_extract(p.data, '$.tool') = 'Bash'
      AND m.time_created > ?
""", (cutoff_ms,))

commands = []
for (input_data,) in cursor.fetchall():
    try:
        data = json.loads(input_data)
        cmd = data.get('command', '')
        commands.append(cmd)
    except:
        pass

print(f"Total bash commands analyzed: {len(commands)}\n")

# Analyze cargo commands
cargo_cmds = [c for c in commands if 'cargo' in c]
print(f"Rust (cargo) commands: {len(cargo_cmds)}")

cargo_patterns = {}
for cmd in cargo_cmds:
    # Normalize: remove specific package names and paths
    if 'cargo build' in cmd:
        pattern = 'cargo build'
    elif 'cargo test' in cmd:
        pattern = 'cargo test'
    elif 'cargo clippy' in cmd:
        pattern = 'cargo clippy'
    elif 'cargo fmt' in cmd:
        pattern = 'cargo fmt'
    else:
        pattern = 'cargo other'
    cargo_patterns[pattern] = cargo_patterns.get(pattern, 0) + 1

print("  Patterns:")
for pattern, count in sorted(cargo_patterns.items(), key=lambda x: -x[1]):
    print(f"    {pattern}: {count}")

# Analyze node commands
node_cmds = [c for c in commands if 'node' in c]
print(f"\nNode.js commands: {len(node_cmds)}")

node_patterns = {}
for cmd in node_cmds:
    if 'node --test' in cmd:
        pattern = 'node --test'
    elif 'node ' in cmd:
        pattern = 'node other'
    else:
        pattern = 'other'
    node_patterns[pattern] = node_patterns.get(pattern, 0) + 1

print("  Patterns:")
for pattern, count in sorted(node_patterns.items(), key=lambda x: -x[1]):
    print(f"    {pattern}: {count}")

# Analyze git commands
git_cmds = [c for c in commands if 'git' in c]
print(f"\nGit commands: {len(git_cmds)}")

git_patterns = {}
for cmd in git_cmds:
    if 'git status' in cmd:
        pattern = 'git status'
    elif 'git add' in cmd:
        pattern = 'git add'
    elif 'git commit' in cmd:
        pattern = 'git commit'
    elif 'git rev-parse' in cmd:
        pattern = 'git rev-parse'
    elif 'git log' in cmd:
        pattern = 'git log'
    else:
        pattern = 'git other'
    git_patterns[pattern] = git_patterns.get(pattern, 0) + 1

print("  Patterns:")
for pattern, count in sorted(git_patterns.items(), key=lambda x: -x[1]):
    print(f"    {pattern}: {count}")

# Analyze test patterns
print("\n=== QUALITY GATE PATTERNS ===")

# Find combinations that suggest quality gates
quality_patterns = []
for cmd in commands:
    if 'cargo fmt' in cmd and 'cargo clippy' in cmd:
        quality_patterns.append('fmt+clippy combo')
    elif 'cargo fmt' in cmd:
        quality_patterns.append('fmt only')
    elif 'cargo clippy' in cmd:
        quality_patterns.append('clippy only')
    elif 'cargo test' in cmd:
        quality_patterns.append('test only')

from collections import Counter
quality_counts = Counter(quality_patterns)
print("Quality gate patterns:")
for pattern, count in quality_counts.most_common(10):
    print(f"  {pattern}: {count}")

# Check for worktree usage
worktree_cmds = [c for c in commands if 'worktree' in c.lower()]
print(f"\nWorktree commands: {len(worktree_cmds)}")
if worktree_cmds:
    print("  Sample commands:")
    for cmd in worktree_cmds[:5]:
        print(f"    {cmd[:100]}")

conn.close()
