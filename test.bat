@echo off
echo Web LLM Runtime - Test Script
echo ============================
echo.

echo 1. Building the project...
cargo build --release 2>&1
if %errorlevel% neq 0 (
    echo Build failed!
    exit /b 1
)
echo Build successful!
echo.

echo 2. Listing available providers...
cargo run -p web-llm-cli -- list-providers
echo.

echo 3. To test the full flow:
echo    a. Start the Browser Bridge:
echo       cargo run -p browser-bridge
echo.
echo    b. Install the Tampermonkey userscript in your browser
echo       (Copy browser/userscript.js to Tampermonkey)
echo.
echo    c. Open ChatGPT or Gemini in your browser and log in
echo.
echo    d. Send a message:
echo       cargo run -p web-llm-cli -- send --provider chatgpt --message "Hello!"
echo.
echo Done!
