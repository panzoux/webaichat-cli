@echo off
echo Running Web LLM Runtime Automated E2E Test Suite (Option B)
echo ==========================================================
cd /d "%~dp0tests"
node run-e2e.js
