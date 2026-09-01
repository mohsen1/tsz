# TSZ installer availability gate for Windows / PowerShell.
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = "",
    [string]$Owner = "tsz-org",
    [string]$Repo = "tsz"
)

$null = $Version, $InstallDir, $Owner, $Repo
[Console]::Error.WriteLine("error: TSZ installation is unavailable during the clean-slate rewrite.")
[Console]::Error.WriteLine("The current R0 compiler is a validation artifact and is not published for installation.")
exit 1
