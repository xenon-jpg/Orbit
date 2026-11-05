param(
  [Parameter(Mandatory=$true)][string]$Repository,
  [ValidateSet('private','public')][string]$Visibility = 'private'
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Use OWNER/REPOSITORY.' }
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'Install GitHub CLI, then run gh auth login.' }
gh auth status
if (-not (Test-Path '.git')) { git init -b main }
if (git status --porcelain) {
  git add -- README.md LICENSE SECURITY.md .gitignore .github docs scripts src src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/build.rs src-tauri/tauri.conf.json src-tauri/capabilities src-tauri/icons src-tauri/.cargo package.json package-lock.json tsconfig.json vite.config.ts vitest.config.ts playwright.config.ts index.html tests
  if ($LASTEXITCODE -ne 0) { throw 'git add failed.' }
  git commit -m "Initial independent AltDeck experimental source"
  if ($LASTEXITCODE -ne 0) { throw 'Commit failed; inspect staged files.' }
}
if (git status --porcelain) { throw 'Working tree is not clean. Review remaining files before publishing.' }
gh repo create $Repository --source . --remote origin "--$Visibility" --push
if ($LASTEXITCODE -ne 0) { throw 'Repository creation/push failed. No credentials were saved by this script.' }
Write-Host "Published $Repository ($Visibility). Check GitHub Actions before distributing a build."
