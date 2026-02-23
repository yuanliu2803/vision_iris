cargo build --lib --release
$source = "target/release/moga_iris.dll"
$dest = "../externLib/moga_iris.dll"

if (!(Test-Path "../externLib")) { New-Item -ItemType Directory "../externLib" }
Copy-Item -Path $source -Destination $dest -Force
Write-Host "DLL clone to $dest"