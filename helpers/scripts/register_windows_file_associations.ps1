param(
  [string]$ExecutablePath = "",
  [switch]$Machine
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
  $ExecutablePath = Join-Path (Resolve-Path ".").Path "target\release\flowstate.exe"
}

$ExecutablePath = (Resolve-Path -LiteralPath $ExecutablePath).Path
$root = if ($Machine) { "Registry::HKEY_LOCAL_MACHINE\Software\Classes" } else { "Registry::HKEY_CURRENT_USER\Software\Classes" }

function Set-KeyValue {
  param([string]$Path, [string]$Name, [string]$Value)
  New-Item -Path $Path -Force | Out-Null
  if ($Name -eq "") {
    Set-Item -Path $Path -Value $Value
  } else {
    New-ItemProperty -Path $Path -Name $Name -Value $Value -PropertyType String -Force | Out-Null
  }
}

function Register-Extension {
  param([string]$Extension, [string]$ProgId, [string]$Description, [bool]$MakeDefault)
  if ($MakeDefault) {
    Set-KeyValue -Path "$root\$Extension" -Name "" -Value $ProgId
  } else {
    New-Item -Path "$root\$Extension\OpenWithProgids" -Force | Out-Null
    New-ItemProperty -Path "$root\$Extension\OpenWithProgids" -Name $ProgId -Value "" -PropertyType String -Force | Out-Null
  }
  Set-KeyValue -Path "$root\$ProgId" -Name "" -Value $Description
  Set-KeyValue -Path "$root\$ProgId\shell\open\command" -Name "" -Value "`"$ExecutablePath`" `"%1`""
}

Register-Extension -Extension ".db8" -ProgId "Flowstate.db8" -Description "Flowstate Debate Document" -MakeDefault $true
Register-Extension -Extension ".docx" -ProgId "Flowstate.docx.import" -Description "Microsoft Word Document imported by Flowstate" -MakeDefault $false

Write-Host "Registered .db8 as a Flowstate document and added Flowstate to the .docx Open With list for $ExecutablePath"
