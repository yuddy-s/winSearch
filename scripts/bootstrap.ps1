$ErrorActionPreference = "Stop"

function Require-Command {
  param([string]$CommandName)

  if (-not (Get-Command $CommandName -ErrorAction SilentlyContinue)) {
    throw "Missing required command: $CommandName"
  }
}

Write-Host "Checking local toolchain..."
Require-Command "node"
Require-Command "npm"
Require-Command "rustc"
Require-Command "cargo"

Write-Host "Node: $(node --version)"
Write-Host "npm: $(npm --version)"
Write-Host "rustc: $(rustc --version)"
Write-Host "cargo: $(cargo --version)"

Write-Host "Installing npm dependencies..."
npm install

Write-Host "Bootstrap complete. Run 'npm run tauri:dev' to launch WinSearch."
