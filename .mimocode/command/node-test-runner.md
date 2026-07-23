---
description: Run Node.js tests with consistent output handling
---

Run Node.js tests in the current directory with standardized output handling.

**Command:**
```bash
node --test 2>&1 | tail -20
```

**What this does:**
1. Executes all test files matching Node.js test runner patterns
2. Captures both stdout and stderr
3. Shows the last 20 lines (test summary and any failures)

**Variants:**
- Run specific test file: `node --test <file> 2>&1 | tail -20`
- Run with verbose output: `node --test --test-reporter=spec 2>&1 | tail -50`
- Run with coverage: `node --test --experimental-test-coverage 2>&1 | tail -30`

**Usage:**
```
node-test
```

**Example output:**
```
ℹ tests 42
ℹ suites 5
ℹ pass 42
ℹ fail 0
ℹ cancellation 0
ℹ duration 1234ms
```
